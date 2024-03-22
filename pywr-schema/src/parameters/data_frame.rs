use crate::error::SchemaError;
use crate::model::LoadArgs;
use crate::parameters::python::try_json_value_into_py;
use crate::parameters::{DynamicFloatValueType, IntoV2Parameter, ParameterMeta, TryFromV1Parameter};
use crate::ConversionError;
use ndarray::Array2;
use polars::prelude::DataType::Float64;
use polars::prelude::{DataFrame, Float64Type, IndexOrder};
use pyo3::prelude::PyModule;
use pyo3::types::{PyDict, PyTuple};
use pyo3::{IntoPy, PyErr, PyObject, Python, ToPyObject};
use pyo3_polars::PyDataFrame;
use pywr_core::parameters::{Array1Parameter, Array2Parameter, ParameterIndex};
use pywr_v1_schema::parameters::DataFrameParameter as DataFrameParameterV1;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(tag = "type", content = "name")]
pub enum DataFrameColumns {
    Scenario(String),
    Column(String),
}

enum FileFormat {
    Csv,
    Hdf,
    Excel,
}

impl FileFormat {
    /// Determine file format from a path's extension.
    fn from_path(path: &Path) -> Option<FileFormat> {
        match path.extension() {
            None => None, // No extension; unknown format
            Some(ext) => match ext.to_str() {
                None => None,
                Some(ext) => match ext.to_lowercase().as_str() {
                    "h5" | "hdf5" | "hdf" => Some(FileFormat::Hdf),
                    "csv" => Some(FileFormat::Csv),
                    "xlsx" | "xlsm" => Some(FileFormat::Excel),
                    "gz" => FileFormat::from_path(&path.with_extension("")),
                    _ => None,
                },
            },
        }
    }
}

/// A parameter that reads its data into a Pandas DataFrame object.
///
/// Upon loading this parameter will attempt to read its data using the Python library
/// `pandas`. It expects to load a timeseries DataFrame which is then sliced and aligned
/// to the
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct DataFrameParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub url: PathBuf,
    pub columns: DataFrameColumns,
    pub timestep_offset: Option<i32>,
    pub pandas_kwargs: HashMap<String, serde_json::Value>,
}

impl DataFrameParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        // Handle the case of an optional data path with a relative url.
        let pth = if let Some(dp) = args.data_path {
            if self.url.is_relative() {
                dp.join(&self.url)
            } else {
                self.url.clone()
            }
        } else {
            self.url.clone()
        };

        let format = FileFormat::from_path(&pth).ok_or(SchemaError::UnsupportedFileFormat)?;

        // 1. Call Python & Pandas to read the data and return an array
        let df: DataFrame = Python::with_gil(|py| {
            // Import pandas and appropriate read function depending on file extension
            let pandas = PyModule::import(py, "pandas")?;
            // Determine pandas read function from file format.
            let read_func = match format {
                FileFormat::Csv => pandas.getattr("read_csv"),
                FileFormat::Hdf => pandas.getattr("read_hdf"),
                FileFormat::Excel => pandas.getattr("read_excel"),
            }?;

            // Import polars and get a reference to the DataFrame initialisation method
            let polars = PyModule::import(py, "polars")?;
            let polars_data_frame_init = polars.getattr("DataFrame")?;

            // Create arguments for pandas
            let args = (pth.into_py(py),);
            let seq = PyTuple::new(
                py,
                self.pandas_kwargs
                    .iter()
                    .map(|(k, v)| (k.into_py(py), try_json_value_into_py(py, v).unwrap())),
            );
            let kwargs = PyDict::from_sequence(py, seq.to_object(py))?;
            // Read pandas DataFrame from relevant function
            let py_pandas_df: PyObject = read_func.call(args, Some(kwargs))?.extract()?;
            // Convert to polars DataFrame using the Python library
            let py_polars_df: PyDataFrame = polars_data_frame_init.call1((py_pandas_df,))?.extract()?;

            Ok(py_polars_df.into())
        })
        .map_err(|e: PyErr| SchemaError::PythonError(e.to_string()))?;

        // 2. TODO Validate the shape of the data array. I.e. check number of columns matches scenario
        //    and number of rows matches time-steps.

        // 3. Create an ArrayParameter using the loaded array.
        match &self.columns {
            DataFrameColumns::Scenario(scenario) => {
                let scenario_group_index = args
                    .domain
                    .scenarios()
                    .group_index(scenario)
                    .ok_or(SchemaError::ScenarioGroupNotFound(scenario.to_string()))?;

                let array: Array2<f64> = df.to_ndarray::<Float64Type>(IndexOrder::default()).unwrap();
                let p = Array2Parameter::new(&self.meta.name, array, scenario_group_index, self.timestep_offset);
                Ok(network.add_parameter(Box::new(p))?)
            }
            DataFrameColumns::Column(column) => {
                let series = df.column(column).unwrap();
                let array = series
                    .cast(&Float64)
                    .unwrap()
                    .f64()
                    .unwrap()
                    .to_ndarray()
                    .unwrap()
                    .to_owned();

                let p = Array1Parameter::new(&self.meta.name, array, self.timestep_offset);
                Ok(network.add_parameter(Box::new(p))?)
            }
        }
    }
}

impl TryFromV1Parameter<DataFrameParameterV1> for DataFrameParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: DataFrameParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);
        let url = v1.url.ok_or(ConversionError::MissingAttribute {
            attrs: vec!["url".to_string()],
            name: meta.name.clone(),
        })?;

        // Here we can only handle a specific column or assume the columns map to a scenario group.
        let columns = match (v1.column, v1.scenario) {
            (None, None) => {
                return Err(ConversionError::MissingAttribute {
                    attrs: vec!["column".to_string(), "scenario".to_string()],
                    name: meta.name.clone(),
                })
            }
            (Some(_), Some(_)) => {
                return Err(ConversionError::UnexpectedAttribute {
                    attrs: vec!["column".to_string(), "scenario".to_string()],
                    name: meta.name.clone(),
                })
            }
            (Some(c), None) => DataFrameColumns::Column(c),
            (None, Some(s)) => DataFrameColumns::Scenario(s),
        };

        if v1.index.is_some() || v1.indexes.is_some() || v1.table.is_some() {
            return Err(ConversionError::UnsupportedAttribute {
                attrs: vec!["index".to_string(), "indexes".to_string(), "table".to_string()],
                name: meta.name.clone(),
            });
        }

        Ok(Self {
            meta,
            url,
            columns,
            timestep_offset: v1.timestep_offset,
            pandas_kwargs: v1.pandas_kwargs,
        })
    }
}
