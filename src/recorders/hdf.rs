use super::{PywrError, Recorder, RecorderMeta, Timestep};
use crate::metric::Metric;
use crate::model::Model;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use hdf5::{Extents, Group};
use ndarray::{s, Array1};
use std::any::Any;
use std::ops::Deref;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct HDF5Recorder {
    meta: RecorderMeta,
    filename: PathBuf,
    metrics: Vec<(Metric, (String, Option<String>))>,
}

struct Internal {
    file: hdf5::File,
    datasets: Vec<hdf5::Dataset>,
    aggregated_datasets: Vec<(Vec<Metric>, hdf5::Dataset)>,
}

#[derive(hdf5::H5Type, Copy, Clone, Debug)]
#[repr(C)]
pub struct Date {
    index: usize,
    year: i32,
    month: u8,
    day: u8,
}

impl Date {
    fn from_timestamp(ts: &Timestep) -> Self {
        Self {
            index: ts.index,
            year: ts.date.year(),
            month: ts.date.month().into(),
            day: ts.date.day(),
        }
    }
}

impl HDF5Recorder {
    pub fn new(name: &str, filename: PathBuf, metrics: Vec<(Metric, (String, Option<String>))>) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            filename,
            metrics,
        }
    }
}

impl Recorder for HDF5Recorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }
    fn setup(
        &self,
        timesteps: &[Timestep],
        scenario_indices: &[ScenarioIndex],
        model: &Model,
    ) -> Result<Option<Box<(dyn Any)>>, PywrError> {
        let file = match hdf5::File::create(&self.filename) {
            Ok(f) => f,
            Err(e) => return Err(PywrError::HDF5Error(e.to_string())),
        };
        let mut datasets = Vec::new();
        let mut aggregated_datasets = Vec::new();

        // Create the time table
        let dates: Array1<_> = timesteps.iter().map(Date::from_timestamp).collect();
        if let Err(e) = file.deref().new_dataset_builder().with_data(&dates).create("time") {
            return Err(PywrError::HDF5Error(e.to_string()));
        }

        let shape = (timesteps.len(), scenario_indices.len());

        let root_grp = file.deref();

        for (metric, _) in &self.metrics {
            let ds = match metric {
                Metric::NodeInFlow(idx) => {
                    let node = model.get_node(idx)?;
                    require_node_dataset(root_grp, shape, node.name(), node.sub_name())?
                }
                Metric::NodeOutFlow(idx) => {
                    let node = model.get_node(idx)?;
                    require_node_dataset(root_grp, shape, node.name(), node.sub_name())?
                }
                Metric::NodeVolume(idx) => {
                    let node = model.get_node(idx)?;
                    require_node_dataset(root_grp, shape, node.name(), node.sub_name())?
                }
                Metric::NodeProportionalVolume(idx) => {
                    let node = model.get_node(idx)?;
                    require_node_dataset(root_grp, shape, node.name(), node.sub_name())?
                }
                Metric::AggregatedNodeVolume(_) => {
                    continue; // TODO
                }
                Metric::AggregatedNodeProportionalVolume(_) => {
                    continue; // TODO
                }
                Metric::EdgeFlow(_) => {
                    continue; // TODO
                }
                Metric::ParameterValue(idx) => {
                    let parameter = model.get_parameter(idx)?;
                    let parameter_group = require_group(root_grp, "parameters")?;
                    require_dataset(&parameter_group, shape, parameter.name())?
                }
                Metric::VirtualStorageVolume(_) => {
                    continue; // TODO
                }
                Metric::VirtualStorageProportionalVolume(_) => {
                    continue; // TODO
                }
                Metric::Constant(_) => {
                    continue; // TODO
                }
            };

            datasets.push(ds);
        }

        // TODO re-enable support for aggregated nodes.
        // for agg_node in model.aggregated_nodes.deref() {
        //     let metrics = agg_node.default_metric();
        //     let name = agg_node.name().to_string();
        //     println!("Adding _metric with name: {}", name);
        //     let ds = match file.new_dataset::<f64>().shape(shape).create(&*name) {
        //         Ok(ds) => ds,
        //         Err(e) => return Err(PywrError::HDF5Error(e.to_string())),
        //     };
        //     agg_datasets.push((metrics, ds));
        // }

        let internal = Internal {
            datasets,
            aggregated_datasets,
            file,
        };

        Ok(Some(Box::new(internal)))
    }
    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        model: &Model,
        state: &[State],
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        let internal = match internal_state {
            Some(internal) => match internal.downcast_mut::<Internal>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        for (dataset, (metric, _)) in internal.datasets.iter_mut().zip(&self.metrics) {
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

    fn finalise(&self, internal_state: &mut Option<Box<dyn Any>>) -> Result<(), PywrError> {
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

fn require_node_dataset<S: Into<Extents>>(
    parent: &Group,
    shape: S,
    name: &str,
    sub_name: Option<&str>,
) -> Result<hdf5::Dataset, PywrError> {
    match sub_name {
        None => require_dataset(parent, shape, name),
        Some(sn) => {
            let grp = require_group(parent, name)?;
            require_dataset(&grp, shape, sn)
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
