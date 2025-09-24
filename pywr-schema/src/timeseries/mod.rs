#[cfg(feature = "core")]
mod align_and_resample;
mod pandas;
mod polars_dataset;

use crate::ConversionError;
use crate::digest::Checksum;
use crate::error::ComponentConversionError;
use crate::parameters::ParameterMeta;
use crate::v1::{ConversionData, IntoV2, TryFromV1};
use crate::visit::VisitPaths;
#[cfg(feature = "core")]
use ndarray::{Array2, ShapeError, s};
pub use pandas::PandasTimeseries;
#[cfg(feature = "core")]
use polars::error::PolarsError;
#[cfg(feature = "core")]
use polars::prelude::{
    DataFrame,
    DataType::{Float64, UInt64},
    Float64Type, IndexOrder, UInt64Type,
};
pub use polars_dataset::PolarsTimeseries;
#[cfg(feature = "pyo3")]
use pyo3::{PyErr, pyclass};
#[cfg(feature = "core")]
use pywr_core::{
    models::ModelDomain,
    parameters::{Array1Parameter, Array2Parameter, ParameterIndex, ParameterName},
};
use pywr_schema_macros::skip_serializing_none;
use pywr_v1_schema::parameters::DataFrameParameter as DataFrameParameterV1;
use schemars::JsonSchema;
#[cfg(feature = "core")]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
use thiserror::Error;

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
    #[error("The length of the resampled timeseries dataframe '{0}' does not match the number of model timesteps.")]
    DataFrameTimestepMismatch(String),
    #[error(
        "The timeseries dataset '{0}' has more than one column of data so a column or scenario name must be provided for any reference"
    )]
    TimeseriesColumnOrScenarioRequired(String),
    #[error("The timeseries dataset '{0}' has no columns")]
    TimeseriesDataframeHasNoColumns(String),
    #[error("Model time domain has no timesteps.")]
    NoTimestepsDefined,
    #[cfg(feature = "pyo3")]
    #[error("Python error: {0}")]
    PythonError(#[from] PyErr),
    #[error("Polars error: {0}")]
    #[cfg(feature = "core")]
    PolarsError(#[from] PolarsError),
    #[cfg(feature = "core")]
    #[error("Python not enabled.")]
    PythonNotEnabled,
    #[cfg(feature = "core")]
    #[error("Scenario error: {0}")]
    Scenario(#[from] pywr_core::scenario::ScenarioError),
    #[cfg(feature = "core")]
    #[error("Shape error: {0}")]
    NdarrayShape(#[from] ShapeError),
    #[error("Pywr core network error: {0}")]
    #[cfg(feature = "core")]
    CoreNetworkError(#[from] pywr_core::NetworkError),
    #[error("Checksum error: {0}")]
    #[cfg(feature = "core")]
    ChecksumError(#[from] crate::digest::ChecksumError),
}

#[cfg(feature = "pyo3")]
impl TryFrom<TimeseriesError> for PyErr {
    type Error = ();
    fn try_from(err: TimeseriesError) -> Result<Self, Self::Error> {
        match err {
            TimeseriesError::PythonError(py_err) => Ok(py_err),
            _ => Err(()),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, Display, EnumDiscriminants)]
#[serde(tag = "type")]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(TimeseriesType))]
pub enum Timeseries {
    Pandas(PandasTimeseries),
    Polars(PolarsTimeseries),
}

impl Timeseries {
    #[cfg(feature = "core")]
    pub fn load(&self, domain: &ModelDomain, data_path: Option<&Path>) -> Result<DataFrame, TimeseriesError> {
        match &self {
            Timeseries::Polars(dataset) => dataset.load(data_path, domain),
            Timeseries::Pandas(dataset) => dataset.load(data_path, domain),
        }
    }

    pub fn name(&self) -> &str {
        match &self {
            Timeseries::Polars(dataset) => dataset.meta.name.as_str(),
            Timeseries::Pandas(dataset) => dataset.meta.name.as_str(),
        }
    }
}

impl VisitPaths for Timeseries {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        match &self {
            Timeseries::Polars(dataset) => dataset.visit_paths(visitor),
            Timeseries::Pandas(dataset) => dataset.visit_paths(visitor),
        }
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        match self {
            Timeseries::Polars(dataset) => dataset.visit_paths_mut(visitor),
            Timeseries::Pandas(dataset) => dataset.visit_paths_mut(visitor),
        }
    }
}

#[derive(Error, Debug)]
#[cfg(feature = "core")]
pub enum LoadTimeseriesError {
    #[error("Failed to load timeseries dataframe from path '{name}': {source}")]
    TimeseriesError { name: String, source: TimeseriesError },
    #[error("A timeseries with name '{0}' already exists.")]
    DuplicateTimeseriesName(String),
}

#[cfg(all(feature = "core", feature = "pyo3"))]
impl TryFrom<LoadTimeseriesError> for PyErr {
    type Error = ();
    fn try_from(err: LoadTimeseriesError) -> Result<Self, Self::Error> {
        match err {
            LoadTimeseriesError::TimeseriesError { source, .. } => source.try_into(),
            _ => Err(()),
        }
    }
}

#[derive(Default)]
#[cfg(feature = "core")]
pub struct LoadedTimeseriesCollection {
    timeseries: HashMap<String, DataFrame>,
}

#[cfg(feature = "core")]
impl LoadedTimeseriesCollection {
    pub fn from_schema(
        timeseries_defs: Option<&[Timeseries]>,
        domain: &ModelDomain,
        data_path: Option<&Path>,
    ) -> Result<Self, LoadTimeseriesError> {
        let mut timeseries = HashMap::new();
        if let Some(timeseries_defs) = timeseries_defs {
            for ts in timeseries_defs {
                let df = ts
                    .load(domain, data_path)
                    .map_err(|source| LoadTimeseriesError::TimeseriesError {
                        name: ts.name().to_string(),
                        source,
                    })?;
                if timeseries.contains_key(ts.name()) {
                    return Err(LoadTimeseriesError::DuplicateTimeseriesName(ts.name().to_string()));
                }
                timeseries.insert(ts.name().to_string(), df);
            }
        }
        Ok(Self { timeseries })
    }

    pub fn load_column_f64(
        &self,
        network: &mut pywr_core::network::Network,
        name: &str,
        col: &str,
    ) -> Result<ParameterIndex<f64>, TimeseriesError> {
        let df = self
            .timeseries
            .get(name)
            .ok_or(TimeseriesError::TimeseriesNotFound(name.to_string()))?;
        let series = df.column(col)?.rechunk();

        let array = series.cast(&Float64)?.f64()?.to_ndarray()?.to_owned();
        let name = ParameterName::new(col, Some(name));

        match network.get_parameter_index_by_name(&name) {
            Some(idx) => Ok(idx),
            None => {
                let p = Array1Parameter::new(name, array, None);
                Ok(network.add_simple_parameter(Box::new(p))?)
            }
        }
    }

    pub fn load_column_usize(
        &self,
        network: &mut pywr_core::network::Network,
        name: &str,
        col: &str,
    ) -> Result<ParameterIndex<u64>, TimeseriesError> {
        let df = self
            .timeseries
            .get(name)
            .ok_or(TimeseriesError::TimeseriesNotFound(name.to_string()))?;
        let series = df.column(col)?.rechunk();

        let array = series.cast(&UInt64)?.u64()?.to_ndarray()?.to_owned();
        let name = ParameterName::new(col, Some(name));

        match network.get_index_parameter_index_by_name(&name) {
            Some(idx) => Ok(idx),
            None => {
                let p = Array1Parameter::new(name, array, None);
                Ok(network.add_simple_index_parameter(Box::new(p))?)
            }
        }
    }

    pub fn load_single_column_f64(
        &self,
        network: &mut pywr_core::network::Network,
        name: &str,
    ) -> Result<ParameterIndex<f64>, TimeseriesError> {
        let df = self
            .timeseries
            .get(name)
            .ok_or(TimeseriesError::TimeseriesNotFound(name.to_string()))?;

        let cols = df.get_column_names();

        if cols.len() > 1 {
            return Err(TimeseriesError::TimeseriesColumnOrScenarioRequired(name.to_string()));
        };

        let col = cols.first().ok_or(TimeseriesError::ColumnNotFound {
            col: "".to_string(),
            name: name.to_string(),
        })?;

        let series = df.column(col)?.rechunk();

        let array = series.cast(&Float64)?.f64()?.to_ndarray()?.to_owned();
        let name = ParameterName::new(col, Some(name));

        match network.get_parameter_index_by_name(&name) {
            Some(idx) => Ok(idx),
            None => {
                let p = Array1Parameter::new(name, array, None);
                Ok(network.add_simple_parameter(Box::new(p))?)
            }
        }
    }

    pub fn load_single_column_usize(
        &self,
        network: &mut pywr_core::network::Network,
        name: &str,
    ) -> Result<ParameterIndex<u64>, TimeseriesError> {
        let df = self
            .timeseries
            .get(name)
            .ok_or(TimeseriesError::TimeseriesNotFound(name.to_string()))?;

        let cols = df.get_column_names();

        if cols.len() > 1 {
            return Err(TimeseriesError::TimeseriesColumnOrScenarioRequired(name.to_string()));
        };

        let col = cols.first().ok_or(TimeseriesError::ColumnNotFound {
            col: "".to_string(),
            name: name.to_string(),
        })?;

        let series = df.column(col)?.rechunk();

        let array = series.cast(&UInt64)?.u64()?.to_ndarray()?.to_owned();
        let name = ParameterName::new(col, Some(name));

        match network.get_index_parameter_index_by_name(&name) {
            Some(idx) => Ok(idx),
            None => {
                let p = Array1Parameter::new(name, array, None);
                Ok(network.add_simple_index_parameter(Box::new(p))?)
            }
        }
    }

    /// Load a timeseries dataframe as a 2D array F64 parameter.
    pub fn load_df_f64(
        &self,
        network: &mut pywr_core::network::Network,
        name: &str,
        domain: &ModelDomain,
        scenario: &str,
    ) -> Result<ParameterIndex<f64>, TimeseriesError> {
        let scenario_group_index = domain.scenarios().group_index(scenario)?;
        let df = self
            .timeseries
            .get(name)
            .ok_or(TimeseriesError::TimeseriesNotFound(name.to_string()))?;

        // Original array as loaded from the timeseries
        let mut array: Array2<f64> = df.to_ndarray::<Float64Type>(IndexOrder::default())?;

        // If there is a scenario subset then we can reduce the data to align with the scenarios
        // that are actually used in the model.
        if let Some(subset) = domain.scenarios().group_scenario_subset(scenario)? {
            array = subset_array2(&array, subset)?;
        }

        let name = ParameterName::new(scenario, Some(name));

        match network.get_parameter_index_by_name(&name) {
            Some(idx) => Ok(idx),
            None => {
                let p = Array2Parameter::new(name, array, scenario_group_index, None);
                Ok(network.add_simple_parameter(Box::new(p))?)
            }
        }
    }

    /// Load a timeseries dataframe as a 2D array Usize parameter.
    pub fn load_df_usize(
        &self,
        network: &mut pywr_core::network::Network,
        name: &str,
        domain: &ModelDomain,
        scenario: &str,
    ) -> Result<ParameterIndex<u64>, TimeseriesError> {
        let scenario_group_index = domain.scenarios().group_index(scenario)?;

        let df = self
            .timeseries
            .get(name)
            .ok_or(TimeseriesError::TimeseriesNotFound(name.to_string()))?;

        let mut array: Array2<u64> = df.to_ndarray::<UInt64Type>(IndexOrder::default())?;

        // If there is a scenario subset then we can reduce the data to align with the scenarios
        // that are actually used in the model.
        if let Some(subset) = domain.scenarios().group_scenario_subset(scenario)? {
            array = subset_array2(&array, subset)?;
        }

        let name = ParameterName::new(scenario, Some(name));

        match network.get_index_parameter_index_by_name(&name) {
            Some(idx) => Ok(idx),
            None => {
                let p = Array2Parameter::new(name, array, scenario_group_index, None);
                Ok(network.add_simple_index_parameter(Box::new(p))?)
            }
        }
    }
}

/// Create a subset of a 2D array based on the column indices.
///
/// This function is used to reduce the size of a timeseries dataframe to only include the columns
/// that are used in a simulation.
#[cfg(feature = "core")]
pub fn subset_array2<T>(array: &Array2<T>, subset: &[usize]) -> Result<Array2<T>, ShapeError>
where
    T: Copy,
{
    // Slice the array to only include the columns that are used in the scenario
    let slices = subset.iter().map(|c| array.slice(s![.., *c])).collect::<Vec<_>>();
    // Stack the slices to create a new array; this should be infallible because
    // the slices are all the same length.
    ndarray::stack(ndarray::Axis(1), &slices)
}

/// Convert timeseries inputs to this schema.
///
/// The conversions
// pub fn __convert_from_v1_data(
//     df_data: Vec<TimeseriesV1Data>,
//     v1_tables: &Option<TableVec>,
//     errors: &mut Vec<ConversionError>,
// ) -> Vec<Timeseries> {
//     let mut ts = HashMap::new();
//     for data in df_data.into_iter() {
//         match data.source {
//             TimeseriesV1Source::Table(name) => {
//                 let tables = v1_tables.as_ref().unwrap();
//                 let table = tables.iter().find(|t| t.name == *name).unwrap();
//                 let name = table.name.clone();
//                 if ts.contains_key(&name) {
//                     continue;
//                 }
//
//                 let time_col = None;
//
//                 let provider = PandasDataset {
//                     time_col,
//                     url: table.url.clone(),
//                     kwargs: Some(data.pandas_kwargs),
//                 };
//
//                 ts.insert(
//                     name.clone(),
//                     Timeseries {
//                         meta: ParameterMeta { name, comment: None },
//                         provider: TimeseriesProvider::Pandas(provider),
//                     },
//                 );
//             }
//             TimeseriesV1Source::Url(url) => {
//                 let name = match data.name {
//                     Some(name) => name,
//                     None => {
//                         errors.push(ConversionError::MissingTimeseriesName(
//                             url.to_str().unwrap_or("").to_string(),
//                         ));
//                         continue;
//                     }
//                 };
//                 if ts.contains_key(&name) {
//                     continue;
//                 }
//
//                 let provider = PandasDataset {
//                     time_col: data.time_col,
//                     url,
//                     kwargs: Some(data.pandas_kwargs),
//                 };
//
//                 ts.insert(
//                     name.clone(),
//                     Timeseries {
//                         meta: ParameterMeta { name, comment: None },
//                         provider: TimeseriesProvider::Pandas(provider),
//                     },
//                 );
//             }
//         }
//     }
//     ts.into_values().collect::<Vec<Timeseries>>()
// }

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PartialEq, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(TimeseriesColumnsType))]
pub enum TimeseriesColumns {
    Scenario { name: String },
    Column { name: String },
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass)]
pub struct TimeseriesReference {
    pub name: String,
    pub columns: Option<TimeseriesColumns>,
}

impl TimeseriesReference {
    pub fn new(name: String, columns: Option<TimeseriesColumns>) -> Self {
        Self { name, columns }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn column(&self) -> Option<&str> {
        match &self.columns {
            Some(TimeseriesColumns::Column { name }) => Some(name.as_str()),
            _ => None,
        }
    }
}

/// Helper struct to convert references to timeseries.
///
/// Keeps a reference to the original parameter name and the new timeseries reference. If the
/// timeseries refers to a table then the original parameter name is no longer required in the
/// final model, but is needed during conversion to ensure that the table is correctly referenced.
#[derive(Clone)]
pub struct ConvertedTimeseriesReference {
    pub original_parameter_name: String,
    pub ts_ref: TimeseriesReference,
}

impl TryFromV1<DataFrameParameterV1> for ConvertedTimeseriesReference {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: DataFrameParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);
        let mut ts_name = meta.name.clone();

        if let Some(url) = v1.url {
            // If there is a URL then this entry must be converted into a timeseries
            let mut pandas_kwargs = v1.pandas_kwargs;

            let time_col = match pandas_kwargs.remove("index_col") {
                Some(v) => v.as_str().map(|s| s.to_string()),
                None => None,
            };
            // remove the parse_dates for CSV files as this is already passed to read_csv in
            // pandas_load.py. This prevents from raising a multiple keyword error.
            if let Some(ext) = url.extension() {
                if ext == "csv" && pandas_kwargs.contains_key("parse_dates") {
                    pandas_kwargs.remove("parse_dates");
                }
            }

            let checksum = match v1.checksum {
                Some(c) => Checksum::try_from_v1(c, parent_node, conversion_data).ok(),
                None => None,
            };

            let timeseries = PandasTimeseries {
                meta: meta.clone(),
                time_col,
                url,
                kwargs: Some(pandas_kwargs),
                checksum,
            };

            // The timeseries data that is extracted
            let timeseries = Timeseries::Pandas(timeseries);

            // Only add if the timeseries does not already exist
            if !conversion_data.timeseries.iter().any(|ts| ts.name() == meta.name) {
                conversion_data.timeseries.push(timeseries);
            }
        } else if let Some(table) = v1.table {
            // If this is a reference to a table then we need to point to the table by name, and
            // ignore the original parameter's name entirely.
            ts_name = table;
        } else {
            return Err(ComponentConversionError::Parameter {
                name: meta.name,
                attr: "url".to_string(),
                error: ConversionError::MissingAttribute {
                    attrs: vec!["url".to_string(), "table".to_string()],
                },
            });
        };

        // Create the reference to the timeseries data
        let columns = match (v1.column, v1.scenario) {
            (Some(name), None) => Some(TimeseriesColumns::Column { name }),
            (None, Some(name)) => Some(TimeseriesColumns::Scenario { name }),
            (Some(_), Some(_)) => {
                return Err(ComponentConversionError::Parameter {
                    name: meta.name.clone(),
                    attr: "column".to_string(),
                    error: ConversionError::AmbiguousAttributes {
                        attrs: vec!["column".to_string(), "scenario".to_string()],
                    },
                });
            }
            (None, None) => None,
        };
        // The reference that is returned
        let reference = TimeseriesReference { name: ts_name, columns };
        Ok(ConvertedTimeseriesReference {
            original_parameter_name: meta.name,
            ts_ref: reference,
        })
    }
}
