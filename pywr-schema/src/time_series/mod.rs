mod arrow_ts;
#[cfg(all(feature = "core", feature = "pyo3"))]
mod load_py;
mod pandas;
mod parquet_ts;
mod placeholder;
mod polars;
mod py;

use crate::ConversionError;
use crate::digest::Checksum;
use crate::error::ComponentConversionError;
use crate::parameters::ParameterMeta;
use crate::v1::{ConversionData, TryFromV1, TryIntoV2};
use crate::visit::{Reference, ReferenceMut, VisitPaths, VisitReferences};
#[cfg(feature = "core")]
use arrow::{
    array::{Array, ArrayRef},
    record_batch::RecordBatch,
};
pub use arrow_ts::{ArrowFormat, ArrowTimeSeries};
pub use pandas::PandasTimeSeries;
pub use parquet_ts::ParquetTimeSeries;
pub use placeholder::PlaceholderTimeSeries;
pub use polars::PolarsTimeSeries;
pub use py::PythonTimeSeries;
#[cfg(feature = "pyo3")]
use pyo3::{PyErr, pyclass};
#[cfg(feature = "core")]
use pywr_core::parameters::{Array1ParameterBuilder, Array2ParameterBuilder, ParameterName};
use pywr_schema_macros::skip_serializing_none;
use pywr_v1_schema::parameters::DataFrameParameter as DataFrameParameterV1;
use schemars::JsonSchema;
#[cfg(feature = "core")]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TimeSeriesError {
    #[error("TimeSeries provider '{provider}' does not support '{fmt}' file types")]
    UnsupportedFileFormat { provider: String, fmt: String },
    #[cfg(feature = "pyo3")]
    #[error("Python error: {0}")]
    PythonError(#[from] PyErr),
    #[cfg(feature = "core")]
    #[error("Python not enabled.")]
    PythonNotEnabled,
    #[error("Checksum error: {0}")]
    #[cfg(feature = "core")]
    ChecksumError(#[from] crate::digest::ChecksumError),
    #[error("Placeholder time series `{name}` cannot be loaded.")]
    PlaceholderTimeSeriesNotAllowed { name: String },
    #[error("IO error on path `{path}`: {source}")]
    #[cfg(feature = "core")]
    IOError {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Arrow error on path `{path}`: {source}")]
    ArrowError {
        path: PathBuf,
        #[source]
        source: arrow::error::ArrowError,
    },
    #[error("Parquet error on path `{path}`: {source}")]
    ParquetError {
        path: PathBuf,
        #[source]
        source: parquet::errors::ParquetError,
    },
}

#[cfg(feature = "pyo3")]
impl TryFrom<TimeSeriesError> for PyErr {
    type Error = ();
    fn try_from(err: TimeSeriesError) -> Result<Self, Self::Error> {
        match err {
            TimeSeriesError::PythonError(py_err) => Ok(py_err),
            _ => Err(()),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, Display, EnumDiscriminants)]
#[serde(tag = "type")]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(TimeSeriesType))]
pub enum TimeSeries {
    Pandas(PandasTimeSeries),
    Polars(PolarsTimeSeries),
    Python(PythonTimeSeries),
    Arrow(ArrowTimeSeries),
    Parquet(ParquetTimeSeries),
    Placeholder(PlaceholderTimeSeries),
}

impl TimeSeries {
    #[cfg(feature = "core")]
    pub fn load(&self, data_path: Option<&Path>) -> Result<LoadedTimeSeries, TimeSeriesError> {
        match &self {
            TimeSeries::Polars(dataset) => dataset.load(data_path),
            TimeSeries::Pandas(dataset) => dataset.load(data_path),
            TimeSeries::Python(dataset) => dataset.load(data_path),
            TimeSeries::Arrow(dataset) => dataset.load(data_path),
            TimeSeries::Parquet(dataset) => dataset.load(data_path),
            TimeSeries::Placeholder(dataset) => dataset.load(),
        }
    }

    pub fn name(&self) -> &str {
        match &self {
            TimeSeries::Polars(dataset) => dataset.meta.name.as_str(),
            TimeSeries::Pandas(dataset) => dataset.meta.name.as_str(),
            TimeSeries::Python(dataset) => dataset.meta.name.as_str(),
            TimeSeries::Arrow(dataset) => dataset.meta.name.as_str(),
            TimeSeries::Parquet(dataset) => dataset.meta.name.as_str(),
            TimeSeries::Placeholder(dataset) => dataset.meta.name.as_str(),
        }
    }

    pub fn meta(&self) -> &ParameterMeta {
        match &self {
            TimeSeries::Polars(dataset) => &dataset.meta,
            TimeSeries::Pandas(dataset) => &dataset.meta,
            TimeSeries::Python(dataset) => &dataset.meta,
            TimeSeries::Arrow(dataset) => &dataset.meta,
            TimeSeries::Parquet(dataset) => &dataset.meta,
            TimeSeries::Placeholder(dataset) => &dataset.meta,
        }
    }

    pub fn is_placeholder(&self) -> bool {
        matches!(self, TimeSeries::Placeholder(_))
    }
}

impl VisitPaths for TimeSeries {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        match &self {
            TimeSeries::Polars(dataset) => dataset.visit_paths(visitor),
            TimeSeries::Pandas(dataset) => dataset.visit_paths(visitor),
            TimeSeries::Python(dataset) => dataset.visit_paths(visitor),
            TimeSeries::Arrow(dataset) => dataset.visit_paths(visitor),
            TimeSeries::Parquet(dataset) => dataset.visit_paths(visitor),
            TimeSeries::Placeholder(dataset) => dataset.visit_paths(visitor),
        }
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        match self {
            TimeSeries::Polars(dataset) => dataset.visit_paths_mut(visitor),
            TimeSeries::Pandas(dataset) => dataset.visit_paths_mut(visitor),
            TimeSeries::Python(dataset) => dataset.visit_paths_mut(visitor),
            TimeSeries::Arrow(dataset) => dataset.visit_paths_mut(visitor),
            TimeSeries::Parquet(dataset) => dataset.visit_paths_mut(visitor),
            TimeSeries::Placeholder(dataset) => dataset.visit_paths_mut(visitor),
        }
    }
}

/// A loaded time series dataset.
///
/// It is expected that one of the columns in the record batch is a time column, which can be used
/// to align the time series with the model timesteps.
#[cfg(feature = "core")]
pub struct LoadedTimeSeries {
    record_batch: RecordBatch,
    time_col: Option<String>,
}

#[cfg(feature = "core")]
impl LoadedTimeSeries {
    pub fn new(record_batch: RecordBatch, time_col: Option<String>) -> Self {
        Self { record_batch, time_col }
    }

    /// Returns the number of columns in the record batch.
    fn num_columns(&self) -> usize {
        self.record_batch.num_columns()
    }

    /// Returns a reference to the column at the given index.
    fn column(&self, index: usize) -> &ArrayRef {
        self.record_batch.column(index)
    }

    /// Returns a reference to the column with the given name, if it exists.
    fn column_by_name(&self, col: &str) -> Option<&ArrayRef> {
        self.record_batch.column_by_name(col)
    }

    /// Returns a reference to a time column.
    ///
    /// If a time column is specified in the time series definition then that column is returned. If no
    /// time column is specified then the first column in the record batch is returned, if its
    /// data type is temporal.
    ///
    fn time_column(&self) -> Option<&ArrayRef> {
        match &self.time_col {
            Some(col) => self.record_batch.column_by_name(col),
            None => {
                let array = self.record_batch.column(0);
                if array.data_type().is_temporal() {
                    Some(array)
                } else {
                    None
                }
            }
        }
    }

    fn time_column_index(&self) -> Option<usize> {
        match &self.time_col {
            Some(col) => self.record_batch.schema().index_of(col).ok(),
            None => {
                let array = self.record_batch.column(0);
                if array.data_type().is_temporal() { Some(0) } else { None }
            }
        }
    }

    /// Returns the name of the time column to look for in the record batch, if it exists.
    fn time_column_name(&self) -> Option<&str> {
        self.time_col.as_deref()
    }

    /// Returns a reference to the first column in the record batch that is not the time column.
    fn first_column_not_time(&self) -> Option<&ArrayRef> {
        if let Some(time_col) = self.time_column_index() {
            for i in 0..self.record_batch.num_columns() {
                if i != time_col {
                    return Some(self.record_batch.column(i));
                }
            }
            None
        } else {
            if self.record_batch.num_columns() > 0 {
                Some(self.record_batch.column(0))
            } else {
                None
            }
        }
    }

    fn not_time_columns(&self) -> Vec<&ArrayRef> {
        let mut cols = Vec::new();
        let time_col_index = self.time_column_index();

        for i in 0..self.record_batch.num_columns() {
            match time_col_index {
                Some(time_index) if i == time_index => continue,
                _ => cols.push(self.record_batch.column(i)),
            }
        }
        cols
    }
}

#[derive(Error, Debug)]
#[cfg(feature = "core")]
pub enum LoadedTimeSeriesCollectionError {
    #[error("Column '{column}' not found in time series input '{name}'")]
    ColumnNotFound { name: String, column: String },
    #[error("Time column '{column}' not found in time series input '{name}'")]
    TimeColumnNotFound { name: String, column: String },
    #[error(
        "No time column explicitly specified for time series input '{name}' and no temporal column could be inferred."
    )]
    TimeColumnCouldNotBeInferred { name: String },
    #[error("Failed to load time series '{name}': {source}")]
    TimeSeriesError { name: String, source: TimeSeriesError },
    #[error("A time series with name '{0}' already exists.")]
    DuplicateTimeSeriesName(String),
    #[error("TimeSeries '{0}' not found in collection.")]
    TimeSeriesNotFound(String),
    #[error(
        "The time series dataset '{0}' has more than one column of data so a column or scenario name must be provided for any reference"
    )]
    TimeSeriesColumnOrScenarioRequired(String),
    #[error("The time series dataset `{0}` is empty and has no columns of data.")]
    TimeSeriesHasNoColumns(String),
}

#[cfg(feature = "core")]
fn make_time_column_not_found_err(name: &str, column: Option<&str>) -> LoadedTimeSeriesCollectionError {
    match column {
        Some(col) => LoadedTimeSeriesCollectionError::TimeColumnNotFound {
            name: name.to_string(),
            column: col.to_string(),
        },
        None => LoadedTimeSeriesCollectionError::TimeColumnCouldNotBeInferred { name: name.to_string() },
    }
}

#[cfg(all(feature = "core", feature = "pyo3"))]
impl TryFrom<LoadedTimeSeriesCollectionError> for PyErr {
    type Error = ();
    fn try_from(err: LoadedTimeSeriesCollectionError) -> Result<Self, Self::Error> {
        match err {
            LoadedTimeSeriesCollectionError::TimeSeriesError { source, .. } => source.try_into(),
            _ => Err(()),
        }
    }
}
#[derive(Default)]
#[cfg(feature = "core")]
pub struct LoadedTimeSeriesCollection {
    time_series: HashMap<String, LoadedTimeSeries>,
}

#[cfg(feature = "core")]
impl LoadedTimeSeriesCollection {
    pub fn from_schema(
        time_series_defs: Option<&[TimeSeries]>,
        data_path: Option<&Path>,
    ) -> Result<Self, LoadedTimeSeriesCollectionError> {
        let mut time_series = HashMap::new();
        if let Some(time_series_defs) = time_series_defs {
            for ts in time_series_defs {
                let df = ts
                    .load(data_path)
                    .map_err(|source| LoadedTimeSeriesCollectionError::TimeSeriesError {
                        name: ts.name().to_string(),
                        source,
                    })?;
                if time_series.contains_key(ts.name()) {
                    return Err(LoadedTimeSeriesCollectionError::DuplicateTimeSeriesName(
                        ts.name().to_string(),
                    ));
                }
                time_series.insert(ts.name().to_string(), df);
            }
        }
        Ok(Self { time_series })
    }

    pub fn load_column_f64(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        name: &str,
        col: &str,
    ) -> Result<ParameterName, LoadedTimeSeriesCollectionError> {
        let df = self
            .time_series
            .get(name)
            .ok_or(LoadedTimeSeriesCollectionError::TimeSeriesNotFound(name.to_string()))?;

        let array = df
            .column_by_name(col)
            .ok_or_else(|| LoadedTimeSeriesCollectionError::ColumnNotFound {
                name: name.to_string(),
                column: col.to_string(),
            })?;

        // TimeSeries is expected to have a time column, which is used to align the time series with the model timesteps.
        let time_array = df
            .time_column()
            .ok_or_else(|| make_time_column_not_found_err(name, df.time_column_name()))?;

        let name = ParameterName::new(col, Some(name));

        if !network.parameters().contains_name(&name) {
            let mut p = Array1ParameterBuilder::from_array_ref(name.clone(), array);
            p.time(time_array.clone());

            network.parameters().f64(Box::new(p));
        }

        Ok(name)
    }

    pub fn load_column_usize(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        name: &str,
        col: &str,
    ) -> Result<ParameterName, LoadedTimeSeriesCollectionError> {
        let df = self
            .time_series
            .get(name)
            .ok_or(LoadedTimeSeriesCollectionError::TimeSeriesNotFound(name.to_string()))?;

        let array = df
            .column_by_name(col)
            .ok_or_else(|| LoadedTimeSeriesCollectionError::ColumnNotFound {
                name: name.to_string(),
                column: col.to_string(),
            })?;

        // Time series is expected to have a time column, which is used to align the time series with the model timesteps.
        let time_array = df
            .time_column()
            .ok_or_else(|| make_time_column_not_found_err(name, df.time_column_name()))?;

        let name = ParameterName::new(col, Some(name));

        if !network.parameters().contains_name(&name) {
            let mut p = Array1ParameterBuilder::from_array_ref(name.clone(), array);
            p.time(time_array.clone());

            network.parameters().u64(Box::new(p));
        }

        Ok(name)
    }

    pub fn load_single_column_f64(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        name: &str,
    ) -> Result<ParameterName, LoadedTimeSeriesCollectionError> {
        let df = self
            .time_series
            .get(name)
            .ok_or(LoadedTimeSeriesCollectionError::TimeSeriesNotFound(name.to_string()))?;

        let (array, time_array) = match df.num_columns() {
            0 => {
                return Err(LoadedTimeSeriesCollectionError::TimeSeriesHasNoColumns(
                    name.to_string(),
                ));
            }
            1 => (df.column(0), None),
            2 => {
                let time_array = df
                    .time_column()
                    .ok_or_else(|| make_time_column_not_found_err(name, df.time_column_name()))?;
                // SAFETY: We know that there are only two columns, and one is the time column,
                // so the other must be the data column.
                let array = df.first_column_not_time().unwrap();
                (array, Some(time_array))
            }
            _ => {
                return Err(LoadedTimeSeriesCollectionError::TimeSeriesColumnOrScenarioRequired(
                    name.to_string(),
                ));
            }
        };

        let name = ParameterName::new("value", Some(name));

        if !network.parameters().contains_name(&name) {
            let mut p = Array1ParameterBuilder::from_array_ref(name.clone(), array);
            if let Some(time_array) = time_array {
                p.time(time_array.clone());
            }
            network.parameters().f64(Box::new(p));
        }

        Ok(name)
    }

    pub fn load_single_column_usize(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        name: &str,
    ) -> Result<ParameterName, LoadedTimeSeriesCollectionError> {
        let df = self
            .time_series
            .get(name)
            .ok_or(LoadedTimeSeriesCollectionError::TimeSeriesNotFound(name.to_string()))?;

        let (array, time_array) = match df.num_columns() {
            0 => {
                return Err(LoadedTimeSeriesCollectionError::TimeSeriesHasNoColumns(
                    name.to_string(),
                ));
            }
            1 => (df.column(0), None),
            2 => {
                let time_array = df
                    .time_column()
                    .ok_or_else(|| make_time_column_not_found_err(name, df.time_column_name()))?;
                // SAFETY: We know that there are only two columns, and one is the time column,
                // so the other must be the data column.
                let array = df.first_column_not_time().unwrap();
                (array, Some(time_array))
            }
            _ => {
                return Err(LoadedTimeSeriesCollectionError::TimeSeriesColumnOrScenarioRequired(
                    name.to_string(),
                ));
            }
        };

        let name = ParameterName::new("value", Some(name));

        if !network.parameters().contains_name(&name) {
            let mut p = Array1ParameterBuilder::from_array_ref(name.clone(), array);
            if let Some(time_array) = time_array {
                p.time(time_array.clone());
            }
            network.parameters().u64(Box::new(p));
        }

        Ok(name)
    }

    /// Load a time series dataframe as a 2D array F64 parameter.
    pub fn load_df_f64(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        name: &str,
        scenario: &str,
    ) -> Result<ParameterName, LoadedTimeSeriesCollectionError> {
        let df = self
            .time_series
            .get(name)
            .ok_or(LoadedTimeSeriesCollectionError::TimeSeriesNotFound(name.to_string()))?;

        // Original array as loaded from the time series
        let array = df.not_time_columns();
        let time_array = df
            .time_column()
            .ok_or_else(|| make_time_column_not_found_err(name, df.time_column_name()))?;

        let name = ParameterName::new(scenario, Some(name));
        if !network.parameters().contains_name(&name) {
            let mut p = Array2ParameterBuilder::from_array_refs(name.clone(), &array, scenario);
            p.time(time_array);
            network.parameters().f64(Box::new(p));
        }

        Ok(name)
    }

    /// Load a time series dataframe as a 2D array Usize parameter.
    pub fn load_df_usize(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        name: &str,

        scenario: &str,
    ) -> Result<ParameterName, LoadedTimeSeriesCollectionError> {
        let df = self
            .time_series
            .get(name)
            .ok_or(LoadedTimeSeriesCollectionError::TimeSeriesNotFound(name.to_string()))?;

        // Original array as loaded from the time series
        let array = df.not_time_columns();
        let time_array = df
            .time_column()
            .ok_or_else(|| make_time_column_not_found_err(name, df.time_column_name()))?;

        let name = ParameterName::new(scenario, Some(name));
        if !network.parameters().contains_name(&name) {
            let mut p = Array2ParameterBuilder::from_array_refs(name.clone(), &array, scenario);
            p.time(time_array);
            network.parameters().u64(Box::new(p));
        }

        Ok(name)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PartialEq, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(TimeSeriesColumnsType))]
pub enum TimeSeriesColumns {
    Scenario { name: String },
    Column { name: String },
}

/// A column name resolves in the time series' own data, so only a scenario group is a reference.
impl VisitReferences for TimeSeriesColumns {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        match self {
            Self::Scenario { name } => visitor(Reference::ScenarioGroup(name)),
            Self::Column { .. } => {}
        }
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        match self {
            Self::Scenario { name } => visitor(ReferenceMut::ScenarioGroup(name)),
            Self::Column { .. } => {}
        }
    }
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(from_py_object))]
pub struct TimeSeriesReference {
    pub name: String,
    pub columns: Option<TimeSeriesColumns>,
}

impl VisitReferences for TimeSeriesReference {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        visitor(Reference::TimeSeries(&self.name));
        self.columns.visit_references(visitor);
    }
    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        visitor(ReferenceMut::TimeSeries(&mut self.name));
        self.columns.visit_references_mut(visitor);
    }
}

impl TimeSeriesReference {
    pub fn new(name: String, columns: Option<TimeSeriesColumns>) -> Self {
        Self { name, columns }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn column(&self) -> Option<&str> {
        match &self.columns {
            Some(TimeSeriesColumns::Column { name }) => Some(name.as_str()),
            _ => None,
        }
    }
}

/// Helper struct to convert references to time series.
///
/// Keeps a reference to the original parameter name and the new time series reference. If the
/// time series refers to a table then the original parameter name is no longer required in the
/// final model, but is needed during conversion to ensure that the table is correctly referenced.
#[derive(Clone)]
pub struct ConvertedTimeSeriesReference {
    pub original_parameter_name: String,
    pub ts_ref: TimeSeriesReference,
}

impl TryFromV1<DataFrameParameterV1> for ConvertedTimeSeriesReference {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: DataFrameParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.try_into_v2(parent_node, conversion_data)?;
        let mut ts_name = meta.name.clone();

        if let Some(url) = v1.url {
            // If there is a URL then this entry must be converted into a time series

            let checksum = match v1.checksum {
                Some(c) => Checksum::try_from_v1(c, parent_node, conversion_data).ok(),
                None => None,
            };

            // This conversion relies on the pandas loading function creating a datetime index
            // which ends up as the first column. This was largely the requirement in v1
            let time_series = PandasTimeSeries {
                meta: meta.clone(),
                time_col: None,
                path: url,
                kwargs: Some(v1.pandas_kwargs),
                checksum,
            };

            // The time series data that is extracted
            let time_series = TimeSeries::Pandas(time_series);

            // Only add if the time series does not already exist
            if !conversion_data.time_series.iter().any(|ts| ts.name() == meta.name) {
                conversion_data.time_series.push(time_series);
            }
        } else if let Some(table) = v1.table {
            // If this is a reference to a table then we need to point to the table by name, and
            // ignore the original parameter's name entirely.
            ts_name = table;
        } else {
            return Err(Box::new(ComponentConversionError::Parameter {
                name: meta.name,
                attr: "url".to_string(),
                error: ConversionError::MissingAttribute {
                    attrs: vec!["url".to_string(), "table".to_string()],
                },
            }));
        };

        // Create the reference to the time series data
        let columns = match (v1.column, v1.scenario) {
            (Some(name), None) => Some(TimeSeriesColumns::Column { name }),
            (None, Some(name)) => Some(TimeSeriesColumns::Scenario { name }),
            (Some(_), Some(_)) => {
                return Err(Box::new(ComponentConversionError::Parameter {
                    name: meta.name.clone(),
                    attr: "column".to_string(),
                    error: ConversionError::AmbiguousAttributes {
                        attrs: vec!["column".to_string(), "scenario".to_string()],
                    },
                }));
            }
            (None, None) => None,
        };
        // The reference that is returned
        let reference = TimeSeriesReference { name: ts_name, columns };
        Ok(ConvertedTimeSeriesReference {
            original_parameter_name: meta.name,
            ts_ref: reference,
        })
    }
}

#[cfg(all(test, feature = "core"))]
mod tests {
    use super::*;
    use arrow::array::{AsArray, Date32Array, Float64Array};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::ipc::writer::FileWriter;
    use parquet::arrow::ArrowWriter;
    use std::fs::File;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn test_record_batch() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("date", DataType::Date32, false),
            Field::new("value", DataType::Float64, false),
        ]));
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Date32Array::from(vec![0, 1, 2])),
                Arc::new(Float64Array::from(vec![1.0, 2.0, 3.0])),
            ],
        )
        .unwrap()
    }

    fn arrow_time_series(path: PathBuf, format: Option<arrow_ts::ArrowFormat>) -> TimeSeries {
        TimeSeries::Arrow(ArrowTimeSeries {
            meta: ParameterMeta {
                name: "test".to_string(),
                comment: None,
                tags: Default::default(),
            },
            time_col: Some("date".to_string()),
            path,
            format,
            checksum: None,
        })
    }

    fn assert_loaded_values(loaded: LoadedTimeSeries) {
        assert_eq!(loaded.record_batch.num_rows(), 3);
        assert_eq!(loaded.record_batch.schema().field(0).data_type(), &DataType::Date32);
        assert_eq!(
            loaded
                .record_batch
                .column(1)
                .as_primitive::<arrow::datatypes::Float64Type>()
                .values(),
            &[1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn arrow_csv_loader_infers_format_from_extension() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("time-series.csv");
        std::fs::write(&path, "date,value\n1970-01-01,1.0\n1970-01-02,2.0\n1970-01-03,3.0\n").unwrap();

        assert_loaded_values(arrow_time_series(path, None).load(None).unwrap());
    }

    #[test]
    fn arrow_ipc_loader_infers_arrow_extension_and_concatenates_batches() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("time-series.arrow");
        let batch = test_record_batch();
        let mut writer = FileWriter::try_new(File::create(&path).unwrap(), &batch.schema()).unwrap();
        writer.write(&batch.slice(0, 1)).unwrap();
        writer.write(&batch.slice(1, 2)).unwrap();
        writer.finish().unwrap();

        assert_loaded_values(arrow_time_series(path, None).load(None).unwrap());
    }

    #[test]
    fn parquet_loader_concatenates_batches() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("time-series.parquet");
        let batch = test_record_batch();
        let mut writer = ArrowWriter::try_new(File::create(&path).unwrap(), batch.schema(), None).unwrap();
        writer.write(&batch.slice(0, 1)).unwrap();
        writer.write(&batch.slice(1, 2)).unwrap();
        writer.close().unwrap();

        let time_series = TimeSeries::Parquet(ParquetTimeSeries {
            meta: ParameterMeta {
                name: "test".to_string(),
                comment: None,
                tags: Default::default(),
            },
            time_col: Some("date".to_string()),
            path,
            checksum: None,
        });
        assert_loaded_values(time_series.load(None).unwrap());
    }

    #[test]
    fn arrow_loader_rejects_unknown_extension_when_format_is_not_specified() {
        let error = match arrow_time_series(PathBuf::from("time-series.unknown"), None).load(None) {
            Ok(_) => panic!("unknown file extension should not be accepted"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            TimeSeriesError::UnsupportedFileFormat { provider, fmt } if provider == "Arrow" && fmt == "unknown"
        ));
    }
}
