use crate::parameters::{Array1Parameter, Array2Parameter};
use crate::schema::parameters::python::try_json_value_into_py;
use crate::schema::parameters::{DynamicFloatValueType, ParameterMeta};
use crate::{ParameterIndex, PywrError};
use ndarray::Array2;
use polars::prelude::DataType::Float64;
use polars::prelude::{DataFrame, Float64Type};
use pyo3::prelude::PyModule;
use pyo3::types::{PyDict, PyTuple};
use pyo3::{IntoPy, PyErr, PyObject, Python, ToPyObject};
use pyo3_polars::PyDataFrame;
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
        model: &mut crate::model::Model,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, PywrError> {
        // Handle the case of an optional data path with a relative url.
        let pth = if let Some(dp) = data_path {
            if self.url.is_relative() {
                dp.join(&self.url)
            } else {
                self.url.clone()
            }
        } else {
            self.url.clone()
        };

        let format = FileFormat::from_path(&pth).ok_or(PywrError::UnsupportedFileFormat)?;

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
        .map_err(|e: PyErr| PywrError::PythonError(e.to_string()))?;

        // 2. TODO Validate the shape of the data array. I.e. check number of columns matches scenario
        //    and number of rows matches time-steps.

        // 3. Create an ArrayParameter using the loaded array.
        match &self.columns {
            DataFrameColumns::Scenario(scenario) => {
                let scenario_group = model.get_scenario_group_index_by_name(scenario)?;
                let array: Array2<f64> = df.to_ndarray::<Float64Type>().unwrap();
                let p = Array2Parameter::new(&self.meta.name, array, scenario_group);
                model.add_parameter(Box::new(p))
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

                let p = Array1Parameter::new(&self.meta.name, array);
                model.add_parameter(Box::new(p))
            }
        }
    }
}
