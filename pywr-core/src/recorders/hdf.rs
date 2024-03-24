use super::{MetricSetState, PywrError, Recorder, RecorderMeta, Timestep};
use crate::models::ModelDomain;
use crate::network::Network;
use crate::recorders::MetricSetIndex;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use chrono::{Datelike, Timelike};
use hdf5::{Extents, Group};
use ndarray::{s, Array1};
use std::any::Any;
use std::ops::Deref;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct HDF5Recorder {
    meta: RecorderMeta,
    filename: PathBuf,
    // TODO this could support saving multiple metric sets in different groups
    metric_set_idx: MetricSetIndex,
}

struct Internal {
    file: hdf5::File,
    datasets: Vec<hdf5::Dataset>,
}

#[derive(hdf5::H5Type, Copy, Clone, Debug)]
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
        let file = match hdf5::File::create(&self.filename) {
            Ok(f) => f,
            Err(e) => return Err(PywrError::HDF5Error(e.to_string())),
        };
        let mut datasets = Vec::new();

        // Create the time table
        let dates: Array1<_> = domain.time().timesteps().iter().map(DateTime::from_timestamp).collect();
        if let Err(e) = file.deref().new_dataset_builder().with_data(&dates).create("time") {
            return Err(PywrError::HDF5Error(e.to_string()));
        }

        let shape = (domain.time().len(), domain.scenarios().len());

        let root_grp = file.deref();

        let metric_set = network.get_metric_set(self.metric_set_idx)?;

        for metric in metric_set.iter_metrics() {
            let name = metric.name(network)?;
            let sub_name = metric.sub_name(network)?;
            let attribute = metric.attribute();

            let ds = require_node_dataset(root_grp, shape, name, sub_name, attribute)?;

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

            if let Err(e) = dataset.write_slice(&values, s![timestep.index, ..]) {
                return Err(PywrError::HDF5Error(e.to_string()));
            }
        }

        Ok(())
    }

    fn finalise(
        &self,
        _metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        // This will leave the internal state with a `None` because we need to take
        // ownership of the file handle in order to close it.
        match internal_state.take() {
            Some(internal) => {
                if let Ok(internal) = internal.downcast::<Internal>() {
                    internal.file.close().map_err(|e| PywrError::HDF5Error(e.to_string()))
                } else {
                    panic!("Internal state did not downcast to the correct type! :(");
                }
            }
            None => panic!("No internal state defined when one was expected! :("),
        }
    }
}

fn require_dataset<S: Into<Extents>>(parent: &Group, shape: S, name: &str) -> Result<hdf5::Dataset, PywrError> {
    parent
        .new_dataset::<f64>()
        .shape(shape)
        .create(name)
        .map_err(|e| PywrError::HDF5Error(e.to_string()))
}

/// Create a node dataset in /parent/name/sub_name/attribute
fn require_node_dataset<S: Into<Extents>>(
    parent: &Group,
    shape: S,
    name: &str,
    sub_name: Option<&str>,
    attribute: &str,
) -> Result<hdf5::Dataset, PywrError> {
    match sub_name {
        None => {
            let grp = require_group(parent, name)?;
            require_dataset(&grp, shape, attribute)
        }
        Some(sn) => {
            let grp = require_group(parent, name)?;
            let grp = require_group(&grp, sn)?;
            require_dataset(&grp, shape, attribute)
        }
    }
}

fn require_group(parent: &Group, name: &str) -> Result<Group, PywrError> {
    match parent.group(name) {
        Ok(g) => Ok(g),
        Err(_) => {
            // Group could not be retrieved already, try to create it instead
            parent
                .create_group(name)
                .map_err(|e| PywrError::HDF5Error(e.to_string()))
        }
    }
}
