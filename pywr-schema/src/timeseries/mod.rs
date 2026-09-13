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
use crate::visit::VisitPaths;
#[cfg(feature = "core")]
use arrow::{
    array::{Array, ArrayRef},
    record_batch::RecordBatch,
};
use arrow_ts::ArrowTimeseries;
pub use pandas::PandasTimeseries;
pub use placeholder::PlaceholderTimeseries;
pub use polars::PolarsTimeseries;
pub use py::PythonTimeseries;
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
pub enum TimeseriesError {
    #[error("Timeseries provider '{provider}' does not support '{fmt}' file types")]
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
    #[error("Placeholder timeseries `{name}` cannot be loaded.")]
    PlaceholderTimeseriesNotAllowed { name: String },
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
    Python(PythonTimeseries),
    Arrow(ArrowTimeseries),
    Parquet(parquet_ts::ParquetTimeseries),
    Placeholder(PlaceholderTimeseries),
}

impl Timeseries {
    #[cfg(feature = "core")]
    pub fn load(&self, data_path: Option<&Path>) -> Result<LoadedTimeseries, TimeseriesError> {
        match &self {
            Timeseries::Polars(dataset) => dataset.load(data_path),
            Timeseries::Pandas(dataset) => dataset.load(data_path),
            Timeseries::Python(dataset) => dataset.load(data_path),
            Timeseries::Arrow(dataset) => dataset.load(data_path),
            Timeseries::Parquet(dataset) => dataset.load(data_path),
            Timeseries::Placeholder(dataset) => dataset.load(),
        }
    }

    pub fn name(&self) -> &str {
        match &self {
            Timeseries::Polars(dataset) => dataset.meta.name.as_str(),
            Timeseries::Pandas(dataset) => dataset.meta.name.as_str(),
            Timeseries::Python(dataset) => dataset.meta.name.as_str(),
            Timeseries::Arrow(dataset) => dataset.meta.name.as_str(),
            Timeseries::Parquet(dataset) => dataset.meta.name.as_str(),
            Timeseries::Placeholder(dataset) => dataset.meta.name.as_str(),
        }
    }

    pub fn meta(&self) -> &ParameterMeta {
        match &self {
            Timeseries::Polars(dataset) => &dataset.meta,
            Timeseries::Pandas(dataset) => &dataset.meta,
            Timeseries::Python(dataset) => &dataset.meta,
            Timeseries::Arrow(dataset) => &dataset.meta,
            Timeseries::Parquet(dataset) => &dataset.meta,
            Timeseries::Placeholder(dataset) => &dataset.meta,
        }
    }

    pub fn is_placeholder(&self) -> bool {
        matches!(self, Timeseries::Placeholder(_))
    }
}

impl VisitPaths for Timeseries {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        match &self {
            Timeseries::Polars(dataset) => dataset.visit_paths(visitor),
            Timeseries::Pandas(dataset) => dataset.visit_paths(visitor),
            Timeseries::Python(dataset) => dataset.visit_paths(visitor),
            Timeseries::Arrow(dataset) => dataset.visit_paths(visitor),
            Timeseries::Parquet(dataset) => dataset.visit_paths(visitor),
            Timeseries::Placeholder(dataset) => dataset.visit_paths(visitor),
        }
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        match self {
            Timeseries::Polars(dataset) => dataset.visit_paths_mut(visitor),
            Timeseries::Pandas(dataset) => dataset.visit_paths_mut(visitor),
            Timeseries::Python(dataset) => dataset.visit_paths_mut(visitor),
            Timeseries::Arrow(dataset) => dataset.visit_paths_mut(visitor),
            Timeseries::Parquet(dataset) => dataset.visit_paths_mut(visitor),
            Timeseries::Placeholder(dataset) => dataset.visit_paths_mut(visitor),
        }
    }
}

/// A loaded timeseries dataset.
///
/// It is expected that one of the columns in the record batch is a time column, which can be used
/// to align the timeseries with the model timesteps.
#[cfg(feature = "core")]
pub struct LoadedTimeseries {
    record_batch: RecordBatch,
    time_col: Option<String>,
}

#[cfg(feature = "core")]
impl LoadedTimeseries {
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
    /// If a time column is specified in the timeseries definition then that column is returned. If no
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
pub enum LoadedTimeseriesCollectionError {
    #[error("Column '{column}' not found in timeseries input '{name}'")]
    ColumnNotFound { name: String, column: String },
    #[error("Time column '{column}' not found in timeseries input '{name}'")]
    TimeColumnNotFound { name: String, column: String },
    #[error(
        "No time column explicitly specified for timeseries input '{name}' and no temporal column could be inferred."
    )]
    TimeColumnCouldNotBeInferred { name: String },
    #[error("Failed to load timeseries dataframe from path '{name}': {source}")]
    TimeseriesError { name: String, source: TimeseriesError },
    #[error("A timeseries with name '{0}' already exists.")]
    DuplicateTimeseriesName(String),
    #[error("Timeseries '{0}' not found in collection.")]
    TimeseriesNotFound(String),
    #[error(
        "The timeseries dataset '{0}' has more than one column of data so a column or scenario name must be provided for any reference"
    )]
    TimeseriesColumnOrScenarioRequired(String),
    #[error("The timeseries dataset is empty and has no columns of data.")]
    TimeseriesHasNoColumns(String),
}

#[cfg(feature = "core")]
fn make_time_column_not_found_err(name: &str, column: Option<&str>) -> LoadedTimeseriesCollectionError {
    match column {
        Some(col) => LoadedTimeseriesCollectionError::TimeColumnNotFound {
            name: name.to_string(),
            column: col.to_string(),
        },
        None => LoadedTimeseriesCollectionError::TimeColumnCouldNotBeInferred { name: name.to_string() },
    }
}

#[cfg(all(feature = "core", feature = "pyo3"))]
impl TryFrom<LoadedTimeseriesCollectionError> for PyErr {
    type Error = ();
    fn try_from(err: LoadedTimeseriesCollectionError) -> Result<Self, Self::Error> {
        match err {
            LoadedTimeseriesCollectionError::TimeseriesError { source, .. } => source.try_into(),
            _ => Err(()),
        }
    }
}
#[derive(Default)]
#[cfg(feature = "core")]
pub struct LoadedTimeseriesCollection {
    timeseries: HashMap<String, LoadedTimeseries>,
}

#[cfg(feature = "core")]
impl LoadedTimeseriesCollection {
    pub fn from_schema(
        timeseries_defs: Option<&[Timeseries]>,
        data_path: Option<&Path>,
    ) -> Result<Self, LoadedTimeseriesCollectionError> {
        let mut timeseries = HashMap::new();
        if let Some(timeseries_defs) = timeseries_defs {
            for ts in timeseries_defs {
                let df = ts
                    .load(data_path)
                    .map_err(|source| LoadedTimeseriesCollectionError::TimeseriesError {
                        name: ts.name().to_string(),
                        source,
                    })?;
                if timeseries.contains_key(ts.name()) {
                    return Err(LoadedTimeseriesCollectionError::DuplicateTimeseriesName(
                        ts.name().to_string(),
                    ));
                }
                timeseries.insert(ts.name().to_string(), df);
            }
        }
        Ok(Self { timeseries })
    }

    pub fn load_column_f64(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        name: &str,
        col: &str,
    ) -> Result<ParameterName, LoadedTimeseriesCollectionError> {
        let df = self
            .timeseries
            .get(name)
            .ok_or(LoadedTimeseriesCollectionError::TimeseriesNotFound(name.to_string()))?;

        let array = df
            .column_by_name(col)
            .ok_or_else(|| LoadedTimeseriesCollectionError::ColumnNotFound {
                name: name.to_string(),
                column: col.to_string(),
            })?;

        // Timeseries is expected to have a time column, which is used to align the timeseries with the model timesteps.
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
    ) -> Result<ParameterName, LoadedTimeseriesCollectionError> {
        let df = self
            .timeseries
            .get(name)
            .ok_or(LoadedTimeseriesCollectionError::TimeseriesNotFound(name.to_string()))?;

        let array = df
            .column_by_name(col)
            .ok_or_else(|| LoadedTimeseriesCollectionError::ColumnNotFound {
                name: name.to_string(),
                column: col.to_string(),
            })?;

        // Timeseries is expected to have a time column, which is used to align the timeseries with the model timesteps.
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
    ) -> Result<ParameterName, LoadedTimeseriesCollectionError> {
        let df = self
            .timeseries
            .get(name)
            .ok_or(LoadedTimeseriesCollectionError::TimeseriesNotFound(name.to_string()))?;

        let (array, time_array) = match df.num_columns() {
            0 => {
                return Err(LoadedTimeseriesCollectionError::TimeseriesHasNoColumns(
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
                return Err(LoadedTimeseriesCollectionError::TimeseriesColumnOrScenarioRequired(
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
    ) -> Result<ParameterName, LoadedTimeseriesCollectionError> {
        let df = self
            .timeseries
            .get(name)
            .ok_or(LoadedTimeseriesCollectionError::TimeseriesNotFound(name.to_string()))?;

        let (array, time_array) = match df.num_columns() {
            0 => {
                return Err(LoadedTimeseriesCollectionError::TimeseriesHasNoColumns(
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
                return Err(LoadedTimeseriesCollectionError::TimeseriesColumnOrScenarioRequired(
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

    /// Load a timeseries dataframe as a 2D array F64 parameter.
    pub fn load_df_f64(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        name: &str,
        scenario: &str,
    ) -> Result<ParameterName, LoadedTimeseriesCollectionError> {
        let df = self
            .timeseries
            .get(name)
            .ok_or(LoadedTimeseriesCollectionError::TimeseriesNotFound(name.to_string()))?;

        // Original array as loaded from the timeseries
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

    /// Load a timeseries dataframe as a 2D array Usize parameter.
    pub fn load_df_usize(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        name: &str,

        scenario: &str,
    ) -> Result<ParameterName, LoadedTimeseriesCollectionError> {
        let df = self
            .timeseries
            .get(name)
            .ok_or(LoadedTimeseriesCollectionError::TimeseriesNotFound(name.to_string()))?;

        // Original array as loaded from the timeseries
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
#[strum_discriminants(name(TimeseriesColumnsType))]
pub enum TimeseriesColumns {
    Scenario { name: String },
    Column { name: String },
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass(from_py_object))]
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
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: DataFrameParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.try_into_v2(parent_node, conversion_data)?;
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
                path: url,
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
            return Err(Box::new(ComponentConversionError::Parameter {
                name: meta.name,
                attr: "url".to_string(),
                error: ConversionError::MissingAttribute {
                    attrs: vec!["url".to_string(), "table".to_string()],
                },
            }));
        };

        // Create the reference to the timeseries data
        let columns = match (v1.column, v1.scenario) {
            (Some(name), None) => Some(TimeseriesColumns::Column { name }),
            (None, Some(name)) => Some(TimeseriesColumns::Scenario { name }),
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
        let reference = TimeseriesReference { name: ts_name, columns };
        Ok(ConvertedTimeseriesReference {
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

    fn arrow_timeseries(path: PathBuf, format: Option<arrow_ts::ArrowFormat>) -> Timeseries {
        Timeseries::Arrow(ArrowTimeseries {
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

    fn assert_loaded_values(loaded: LoadedTimeseries) {
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
        let path = temp_dir.path().join("timeseries.csv");
        std::fs::write(&path, "date,value\n1970-01-01,1.0\n1970-01-02,2.0\n1970-01-03,3.0\n").unwrap();

        assert_loaded_values(arrow_timeseries(path, None).load(None).unwrap());
    }

    #[test]
    fn arrow_ipc_loader_infers_arrow_extension_and_concatenates_batches() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("timeseries.arrow");
        let batch = test_record_batch();
        let mut writer = FileWriter::try_new(File::create(&path).unwrap(), &batch.schema()).unwrap();
        writer.write(&batch.slice(0, 1)).unwrap();
        writer.write(&batch.slice(1, 2)).unwrap();
        writer.finish().unwrap();

        assert_loaded_values(arrow_timeseries(path, None).load(None).unwrap());
    }

    #[test]
    fn parquet_loader_concatenates_batches() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("timeseries.parquet");
        let batch = test_record_batch();
        let mut writer = ArrowWriter::try_new(File::create(&path).unwrap(), batch.schema(), None).unwrap();
        writer.write(&batch.slice(0, 1)).unwrap();
        writer.write(&batch.slice(1, 2)).unwrap();
        writer.close().unwrap();

        let timeseries = Timeseries::Parquet(parquet_ts::ParquetTimeseries {
            meta: ParameterMeta {
                name: "test".to_string(),
                comment: None,
                tags: Default::default(),
            },
            time_col: Some("date".to_string()),
            path,
            checksum: None,
        });
        assert_loaded_values(timeseries.load(None).unwrap());
    }

    #[test]
    fn arrow_loader_rejects_unknown_extension_when_format_is_not_specified() {
        let error = match arrow_timeseries(PathBuf::from("timeseries.unknown"), None).load(None) {
            Ok(_) => panic!("unknown file extension should not be accepted"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            TimeseriesError::UnsupportedFileFormat { provider, fmt } if provider == "Arrow" && fmt == "unknown"
        ));
    }
}
