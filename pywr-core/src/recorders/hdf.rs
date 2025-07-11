use super::{MetricSetState, OutputMetric, PywrError, Recorder, RecorderMeta, Timestep};
use crate::models::ModelDomain;
use crate::network::Network;
use crate::recorders::MetricSetIndex;
use crate::scenario::{ScenarioDomain, ScenarioIndex};
use crate::state::State;
use chrono::{Datelike, Timelike};
use hdf5_metno::{Extents, Group};
use ndarray::{Array1, s};
use std::any::Any;
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;

/// A recorder that saves model outputs to an HDF5 file.
///
/// This recorder saves the model outputs to an HDF5 file. The file will contain a number of groups
/// and datasets that correspond to the metrics in the metric set. Additionally, the file will
/// contain metadata about the time steps and scenarios that were used in the model simulation.
///
#[derive(Clone, Debug)]
pub struct HDF5Recorder {
    meta: RecorderMeta,
    filename: PathBuf,
    // TODO this could support saving multiple metric sets in different groups
    metric_set_idx: MetricSetIndex,
}

struct Internal {
    file: hdf5_metno::File,
    datasets: Vec<hdf5_metno::Dataset>,
}

#[derive(hdf5_metno::H5Type, Copy, Clone, Debug)]
#[repr(C)]
pub struct DateTime {
    index: usize,
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
}

impl DateTime {
    fn from_timestamp(ts: &Timestep) -> Self {
        Self {
            index: ts.index,
            year: ts.date.year(),
            month: ts.date.month() as u8,
            day: ts.date.day() as u8,
            hour: ts.date.time().hour() as u8,
            minute: ts.date.time().minute() as u8,
            second: ts.date.time().second() as u8,
        }
    }
}

impl HDF5Recorder {
    pub fn new<P: Into<PathBuf>>(name: &str, filename: P, metric_set_idx: MetricSetIndex) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            filename: filename.into(),
            metric_set_idx,
        }
    }
}

impl Recorder for HDF5Recorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }
    fn setup(&self, domain: &ModelDomain, network: &Network) -> Result<Option<Box<(dyn Any)>>, PywrError> {
        let file = hdf5_metno::File::create(&self.filename)?;

        write_pywr_metadata(&file)?;
        write_scenarios_metadata(&file, domain.scenarios())?;

        // Create the time table
        let dates: Array1<_> = domain.time().timesteps().iter().map(DateTime::from_timestamp).collect();
        file.deref().new_dataset_builder().with_data(&dates).create("time")?;

        let shape = (domain.time().len(), domain.scenarios().len());

        let root_grp = file.deref();

        let metric_set = network.get_metric_set(self.metric_set_idx)?;

        let mut datasets = Vec::new();

        for metric in metric_set.iter_metrics() {
            let ds = require_metric_dataset(root_grp, shape, metric)?;
            datasets.push(ds);
        }

        let internal = Internal { datasets, file };

        Ok(Some(Box::new(internal)))
    }
    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        model: &Network,
        state: &[State],
        _metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        let internal = match internal_state {
            Some(internal) => match internal.downcast_mut::<Internal>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        let metric_set = model.get_metric_set(self.metric_set_idx)?;

        for (dataset, metric) in internal.datasets.iter_mut().zip(metric_set.iter_metrics()) {
            // Combine all the values for metric across all of the scenarios
            let values = scenario_indices
                .iter()
                .zip(state)
                .map(|(_, s)| metric.get_value(model, s))
                .collect::<Result<Vec<_>, _>>()?;

            dataset.write_slice(&values, s![timestep.index, ..])?;
        }

        Ok(())
    }

    fn finalise(
        &self,
        _network: &Network,
        _scenario_indices: &[ScenarioIndex],
        _metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        // This will leave the internal state with a `None` because we need to take
        // ownership of the file handle in order to close it.
        match internal_state.take() {
            Some(internal) => {
                if let Ok(internal) = internal.downcast::<Internal>() {
                    Ok(internal.file.close()?)
                } else {
                    panic!("Internal state did not downcast to the correct type! :(");
                }
            }
            None => panic!("No internal state defined when one was expected! :("),
        }
    }
}

fn require_dataset<S: Into<Extents>>(parent: &Group, shape: S, name: &str) -> Result<hdf5_metno::Dataset, PywrError> {
    Ok(parent.new_dataset::<f64>().shape(shape).create(name)?)
}

/// Create a node dataset in /parent/name/sub_name/attribute
fn require_metric_dataset<S: Into<Extents>>(
    parent: &Group,
    shape: S,
    metric: &OutputMetric,
) -> Result<hdf5_metno::Dataset, PywrError> {
    let grp = require_group(parent, metric.name())?;
    let ds = require_dataset(&grp, shape, metric.attribute())?;

    // Write the type and subtype as attributes
    let ty = hdf5_metno::types::VarLenUnicode::from_str(metric.ty())
        .map_err(|e| PywrError::HDF5VarLenUnicode(e.to_string()))?;
    let attr = ds
        .new_attr::<hdf5_metno::types::VarLenUnicode>()
        .shape(())
        .create("pywr-type")?;
    attr.as_writer().write_scalar(&ty)?;

    if let Some(sub_type) = metric.sub_type() {
        let sub_type = hdf5_metno::types::VarLenUnicode::from_str(sub_type)
            .map_err(|e| PywrError::HDF5VarLenUnicode(e.to_string()))?;
        let attr = ds
            .new_attr::<hdf5_metno::types::VarLenUnicode>()
            .shape(())
            .create("pywr-subtype")?;
        attr.as_writer().write_scalar(&sub_type)?;
    }
    Ok(ds)
}

fn require_group(parent: &Group, name: &str) -> Result<Group, PywrError> {
    match parent.group(name) {
        Ok(g) => Ok(g),
        Err(_) => {
            // Group could not be retrieved already, try to create it instead
            Ok(parent.create_group(name)?)
        }
    }
}

fn write_pywr_metadata(file: &hdf5_metno::File) -> Result<(), PywrError> {
    let root = file.deref();

    const VERSION: &str = env!("CARGO_PKG_VERSION");
    let version =
        hdf5_metno::types::VarLenUnicode::from_str(VERSION).map_err(|e| PywrError::HDF5VarLenUnicode(e.to_string()))?;

    let attr = root
        .new_attr::<hdf5_metno::types::VarLenUnicode>()
        .shape(())
        .create("pywr-version")?;
    attr.as_writer().write_scalar(&version)?;

    Ok(())
}

#[derive(hdf5_metno::H5Type, Clone, PartialEq, Debug)]
#[repr(C)]
pub struct ScenarioGroupEntry {
    pub name: hdf5_metno::types::VarLenUnicode,
    pub size: usize,
}

#[derive(hdf5_metno::H5Type, Clone, PartialEq, Debug)]
#[repr(C)]
pub struct H5ScenarioIndex {
    index: usize,
    indices: hdf5_metno::types::VarLenArray<usize>,
    label: hdf5_metno::types::VarLenUnicode,
}

/// Write scenario metadata to the HDF5 file.
///
/// This function will create the `/scenarios` group in the HDF5 file and write the scenario
/// groups and indices into `/scenarios/groups` and `/scenarios/indices` respectively.
fn write_scenarios_metadata(file: &hdf5_metno::File, domain: &ScenarioDomain) -> Result<(), PywrError> {
    // Create the scenario group and associated datasets
    let grp = require_group(file.deref(), "scenarios")?;

    let scenario_groups: Array1<ScenarioGroupEntry> = domain
        .groups()
        .iter()
        .map(|s| {
            let name = hdf5_metno::types::VarLenUnicode::from_str(s.name())
                .map_err(|e| PywrError::HDF5VarLenUnicode(e.to_string()))?;

            Ok(ScenarioGroupEntry { name, size: s.size() })
        })
        .collect::<Result<_, PywrError>>()?;

    grp.new_dataset_builder().with_data(&scenario_groups).create("groups")?;

    let scenarios: Array1<H5ScenarioIndex> = domain
        .indices()
        .iter()
        .map(|s| {
            let indices = hdf5_metno::types::VarLenArray::from_slice(s.simulation_indices());
            let label = hdf5_metno::types::VarLenUnicode::from_str(&s.label())
                .map_err(|e| PywrError::HDF5VarLenUnicode(e.to_string()))?;

            Ok(H5ScenarioIndex {
                index: s.simulation_id(),
                indices,
                label,
            })
        })
        .collect::<Result<_, PywrError>>()?;

    grp.new_dataset_builder().with_data(&scenarios).create("indices")?;

    Ok(())
}
