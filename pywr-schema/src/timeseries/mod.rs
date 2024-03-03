mod align_and_resample;
mod polars_dataset;

use ndarray::Array2;
use polars::error::PolarsError;
use polars::prelude::DataType::Float64;
use polars::prelude::{DataFrame, Float64Type, IndexOrder};
use pywr_core::models::ModelDomain;
use pywr_core::parameters::{Array1Parameter, Array2Parameter, ParameterIndex};
use pywr_core::PywrError;
use std::{collections::HashMap, path::Path};
use thiserror::Error;

use crate::parameters::ParameterMeta;

use self::polars_dataset::PolarsDataset;

#[derive(Error, Debug)]
pub enum TimeseriesError {
    #[error("Timeseries '{0} not found")]
    TimeseriesNotFound(String),
    #[error("The duration of timeseries '{0}' could not be determined.")]
    TimeseriesDurationNotFound(String),
    #[error("Column '{col}' not found in timeseries input '{name}'")]
    ColumnNotFound { col: String, name: String },
    #[error("Timeseries provider '{provider}' does not support '{fmt}' file types")]
    TimeseriesUnsupportedFileFormat { provider: String, fmt: String },
    #[error("Timeseries provider '{provider}' cannot parse file: '{path}'")]
    TimeseriesUnparsableFileFormat { provider: String, path: String },
    #[error("A scenario group with name '{0}' was not found")]
    ScenarioGroupNotFound(String),
    #[error("Polars error: {0}")]
    PolarsError(#[from] PolarsError),
    #[error("Pywr core error: {0}")]
    PywrCore(#[from] pywr_core::PywrError),
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(tag = "type")]
enum TimeseriesProvider {
    Pandas,
    Polars(PolarsDataset),
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Timeseries {
    #[serde(flatten)]
    meta: ParameterMeta,
    provider: TimeseriesProvider,
}

impl Timeseries {
    pub fn load(&self, domain: &ModelDomain, data_path: Option<&Path>) -> Result<DataFrame, TimeseriesError> {
        match &self.provider {
            TimeseriesProvider::Polars(dataset) => dataset.load(self.meta.name.as_str(), data_path, domain),
            TimeseriesProvider::Pandas => todo!(),
        }
    }
}

pub struct LoadedTimeseriesCollection {
    timeseries: HashMap<String, DataFrame>,
}

impl LoadedTimeseriesCollection {
    pub fn from_schema(
        timeseries_defs: Option<&[Timeseries]>,
        domain: &ModelDomain,
        data_path: Option<&Path>,
    ) -> Result<Self, TimeseriesError> {
        let mut timeseries = HashMap::new();
        if let Some(timeseries_defs) = timeseries_defs {
            for ts in timeseries_defs {
                let df = ts.load(domain, data_path)?;
                // TODO error if key already exists
                timeseries.insert(ts.meta.name.clone(), df);
            }
        }
        Ok(Self { timeseries })
    }

    pub fn load_column(
        &self,
        network: &mut pywr_core::network::Network,
        name: &str,
        col: &str,
    ) -> Result<ParameterIndex, TimeseriesError> {
        let df = self
            .timeseries
            .get(name)
            .ok_or(TimeseriesError::TimeseriesNotFound(name.to_string()))?;
        let series = df.column(col)?;

        let array = series.cast(&Float64)?.f64()?.to_ndarray()?.to_owned();
        let name = format!("{}_{}", name, col);

        match network.get_parameter_index_by_name(&name) {
            Ok(idx) => Ok(idx),
            Err(e) => match e {
                PywrError::ParameterNotFound(_) => {
                    let p = Array1Parameter::new(&name, array, None);
                    Ok(network.add_parameter(Box::new(p))?)
                }
                _ => Err(TimeseriesError::PywrCore(e)),
            },
        }
    }

    pub fn load_df(
        &self,
        network: &mut pywr_core::network::Network,
        name: &str,
        domain: &ModelDomain,
        scenario: &str,
    ) -> Result<ParameterIndex, TimeseriesError> {
        let scenario_group_index = domain
            .scenarios()
            .group_index(scenario)
            .ok_or(TimeseriesError::ScenarioGroupNotFound(scenario.to_string()))?;

        let df = self
            .timeseries
            .get(name)
            .ok_or(TimeseriesError::TimeseriesNotFound(name.to_string()))?;

        let array: Array2<f64> = df.to_ndarray::<Float64Type>(IndexOrder::default()).unwrap();
        let name = format!("timeseries.{}_{}", name, scenario);

        match network.get_parameter_index_by_name(&name) {
            Ok(idx) => Ok(idx),
            Err(e) => match e {
                PywrError::ParameterNotFound(_) => {
                    let p = Array2Parameter::new(&name, array, scenario_group_index, None);
                    Ok(network.add_parameter(Box::new(p))?)
                }
                _ => Err(TimeseriesError::PywrCore(e)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::{Datelike, NaiveDate};
    use ndarray::Array;
    use pywr_core::{metric::Metric, recorders::AssertionRecorder, test_utils::run_all_solvers};

    use crate::PywrModel;

    fn model_str() -> &'static str {
        include_str!("../test_models/timeseries.json")
    }

    #[test]
    fn test_timeseries_polars() {
        let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");

        let model_dir = PathBuf::from(cargo_manifest_dir).join("src/test_models");

        dbg!(&model_dir);

        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let mut model = schema.build_model(Some(model_dir.as_path()), None).unwrap();

        let expected = Array::from_shape_fn((365, 1), |(x, _)| {
            (NaiveDate::from_yo_opt(2021, (x + 1) as u32).unwrap().day() + 2) as f64
        });
        let idx = model.network().get_node_by_name("output1", None).unwrap().index();

        let recorder = AssertionRecorder::new("output-flow", Metric::NodeInFlow(idx), expected.clone(), None, None);
        model.network_mut().add_recorder(Box::new(recorder)).unwrap();

        run_all_solvers(&model)
    }
}
