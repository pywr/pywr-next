mod align_and_resample;
mod polars_dataset;

use ndarray::Array2;
use polars::error::PolarsError;
use polars::prelude::DataType::Float64;
use polars::prelude::{DataFrame, Float64Type, IndexOrder};
use pywr_core::models::ModelDomain;
use pywr_core::parameters::{Array1Parameter, Array2Parameter, ParameterIndex};
use pywr_core::PywrError;
use pywr_v1_schema::tables::TableVec;
use std::{collections::HashMap, path::Path};
use thiserror::Error;

use crate::parameters::{ParameterMeta, TimeseriesV1Data, TimeseriesV1Source};
use crate::ConversionError;

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
    #[error("Duration could not be represented as nanoseconds")]
    NoDurationNanoSeconds,
    #[error("The length of the resampled timeseries dataframe '{0}' does not match the number of model timesteps.")]
    DataFrameTimestepMismatch(String),
    #[error("A timeseries dataframe with the name '{0}' already exists.")]
    TimeseriesDataframeAlreadyExists(String),
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

    pub fn name(&self) -> &str {
        &self.meta.name
    }
}

#[derive(Default)]
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
                if timeseries.contains_key(&ts.meta.name) {
                    return Err(TimeseriesError::TimeseriesDataframeAlreadyExists(ts.meta.name.clone()));
                }
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
    ) -> Result<ParameterIndex<f64>, TimeseriesError> {
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
    ) -> Result<ParameterIndex<f64>, TimeseriesError> {
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

pub fn convert_from_v1_data(
    df_data: Vec<TimeseriesV1Data>,
    v1_tables: &Option<TableVec>,
    errors: &mut Vec<ConversionError>,
) -> Vec<Timeseries> {
    let mut ts = HashMap::new();
    for data in df_data.into_iter() {
        match data.source {
            TimeseriesV1Source::Table(name) => {
                let tables = v1_tables.as_ref().unwrap();
                let table = tables.iter().find(|t| t.name == *name).unwrap();
                let name = table.name.clone();
                if ts.contains_key(&name) {
                    continue;
                }

                let time_col = None;
                let provider = PolarsDataset::new(time_col, table.url.clone());

                ts.insert(
                    name.clone(),
                    Timeseries {
                        meta: ParameterMeta { name, comment: None },
                        provider: TimeseriesProvider::Polars(provider),
                    },
                );
            }
            TimeseriesV1Source::Url(url) => {
                let name = match data.name {
                    Some(name) => name,
                    None => {
                        errors.push(ConversionError::MissingTimeseriesName(
                            url.to_str().unwrap_or("").to_string(),
                        ));
                        continue;
                    }
                };
                if ts.contains_key(&name) {
                    continue;
                }

                let provider = PolarsDataset::new(data.time_col, url);

                ts.insert(
                    name.clone(),
                    Timeseries {
                        meta: ParameterMeta { name, comment: None },
                        provider: TimeseriesProvider::Polars(provider),
                    },
                );
            }
        }
    }
    ts.into_values().collect::<Vec<Timeseries>>()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::{Datelike, NaiveDate};
    use ndarray::Array;
    use pywr_core::{metric::MetricF64, recorders::AssertionRecorder, test_utils::run_all_solvers};

    use crate::PywrModel;

    fn model_str() -> &'static str {
        include_str!("../test_models/timeseries.json")
    }

    #[test]
    fn test_timeseries_polars() {
        let cargo_manifest_dir = env!("CARGO_MANIFEST_DIR");

        let model_dir = PathBuf::from(cargo_manifest_dir).join("src/test_models");

        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let mut model = schema.build_model(Some(model_dir.as_path()), None).unwrap();

        let expected = Array::from_shape_fn((365, 1), |(x, _)| {
            let month_day = NaiveDate::from_yo_opt(2021, (x + 1) as u32).unwrap().day() as f64;
            month_day + month_day * 0.5
        });

        let idx = model.network().get_node_by_name("output1", None).unwrap().index();

        let recorder = AssertionRecorder::new("output-flow", MetricF64::NodeInFlow(idx), expected.clone(), None, None);
        model.network_mut().add_recorder(Box::new(recorder)).unwrap();

        run_all_solvers(&model)
    }
}
