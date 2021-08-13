use super::{NetworkState, PywrError, RecorderMeta, Timestep, _Recorder};
use crate::metric::Metric;
use crate::model::Model;
use crate::scenario::ScenarioIndex;
use crate::state::ParameterState;
use ndarray::{s, Array2};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) struct HDF5Recorder {
    meta: RecorderMeta,
    filename: PathBuf,
    file: Option<hdf5::File>,
    datasets: Option<Vec<(Metric, hdf5::Dataset)>>,
    array: Option<ndarray::Array2<f64>>,
}

impl HDF5Recorder {
    pub fn new(name: &str, filename: PathBuf) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            filename,
            file: None,
            datasets: None,
            array: None,
        }
    }
}

impl _Recorder for HDF5Recorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }
    fn setup(
        &mut self,
        model: &Model,
        timesteps: &Vec<Timestep>,
        scenario_indices: &Vec<ScenarioIndex>,
    ) -> Result<(), PywrError> {
        let file = match hdf5::File::create(&self.filename) {
            Ok(f) => f,
            Err(e) => return Err(PywrError::HDF5Error(e.to_string())),
        };
        let mut datasets = Vec::new();

        let shape = (timesteps.len(), scenario_indices.len());

        for node in &model.nodes {
            let metric = node.default_metric();
            let name = node.name().to_string();
            println!("Adding metric with name: {}", name);
            let ds = match file.new_dataset::<f64>().shape(shape).create(&*name) {
                Ok(ds) => ds,
                Err(e) => return Err(PywrError::HDF5Error(e.to_string())),
            };
            datasets.push((metric, ds));
        }

        self.array = Some(Array2::zeros((datasets.len(), scenario_indices.len())));
        self.datasets = Some(datasets);
        self.file = Some(file);

        Ok(())
    }
    fn save(
        &mut self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Model,
        network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<(), PywrError> {
        match (&mut self.array, &self.datasets) {
            (Some(array), Some(datasets)) => {
                for (idx, (metric, _ds)) in datasets.iter().enumerate() {
                    let value = metric.get_value(model, network_state, parameter_state)?;
                    array[[idx, scenario_index.index]] = value
                }
                Ok(())
            }
            _ => Err(PywrError::RecorderNotInitialised),
        }
    }

    fn after_save(&mut self, timestep: &Timestep) -> Result<(), PywrError> {
        match (&self.array, &mut self.datasets) {
            (Some(array), Some(datasets)) => {
                for (node_idx, (_metric, dataset)) in datasets.iter_mut().enumerate() {
                    if let Err(e) = dataset.write_slice(array.slice(s![node_idx, ..]), s![timestep.index, ..]) {
                        return Err(PywrError::HDF5Error(e.to_string()));
                    }
                }
                Ok(())
            }
            _ => Err(PywrError::RecorderNotInitialised),
        }
    }

    fn finalise(&mut self) -> Result<(), PywrError> {
        match self.file.take() {
            Some(file) => {
                file.close();
                Ok(())
            }
            None => Err(PywrError::RecorderNotInitialised),
        }
    }
}
