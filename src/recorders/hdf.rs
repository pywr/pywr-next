use super::{PywrError, Recorder, RecorderMeta, Timestep};
use crate::metric::Metric;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use ndarray::{s, Array2};
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
    ) -> Result<Option<Box<(dyn Any)>>, PywrError> {
        let file = match hdf5::File::create(&self.filename) {
            Ok(f) => f,
            Err(e) => return Err(PywrError::HDF5Error(e.to_string())),
        };
        let mut datasets = Vec::new();
        let mut aggregated_datasets = Vec::new();

        let shape = (timesteps.len(), scenario_indices.len());

        for (_metric, (name, sub_name)) in &self.metrics {
            let ds = match sub_name {
                Some(sn) => {
                    // This is a node with sub-nodes, create a group for the parent node
                    let grp = match require_group(file.deref(), name) {
                        Ok(g) => g,
                        Err(e) => return Err(PywrError::HDF5Error(e.to_string())),
                    };
                    match grp.new_dataset::<f64>().shape(shape).create(sn.as_str()) {
                        Ok(ds) => ds,
                        Err(e) => return Err(PywrError::HDF5Error(e.to_string())),
                    }
                }
                None => match file.new_dataset::<f64>().shape(shape).create(name.as_str()) {
                    Ok(ds) => ds,
                    Err(e) => return Err(PywrError::HDF5Error(e.to_string())),
                },
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
                .map(|(si, s)| metric.get_value(s))
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

fn require_group(parent: &hdf5::Group, name: &str) -> Result<hdf5::Group, hdf5::Error> {
    match parent.group(name) {
        Ok(g) => Ok(g),
        Err(_) => {
            // Group could not be retrieved already, try to create it instead
            parent.create_group(name)
        }
    }
}
