use crate::data_tables::{make_path, LoadedTableCollection};
use crate::error::SchemaError;
use crate::model::PywrMultiNetworkTransfer;
use crate::parameters::{DynamicFloatValue, DynamicFloatValueType, DynamicIndexValue, ParameterMeta};
use pyo3::prelude::PyModule;
use pyo3::types::{PyDict, PyTuple};
use pyo3::{IntoPy, PyErr, PyObject, Python, ToPyObject};
use pywr_core::models::ModelDomain;
use pywr_core::parameters::{ParameterType, PyParameter};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum PythonModule {
    Module(String),
    Path(PathBuf),
}

/// A Parameter that uses a Python object for its calculations.
///
/// This struct defines a schema for loading a [`crate::parameters::PyParameter`] from external
/// sources. The user provides the name of an object in the given module. Typically, this object will be
/// a class the user has written. For more information on the expected format and signature of
/// this object please refer to the [`crate::parameters::PyParameter`] documentation. The object
/// is initialised with user provided positional and/or keyword arguments that can be provided
/// here.
///
/// In additions `metrics` and `indices` can be specified. These dependent values from the network
/// are provided to the calculation method of the Python object. This allows a custom Python
/// parameter to use information from the current network simulation (e.g. current storage volume,
/// other parameter value or index).
///
/// ```
/// use pywr_schema::parameters::Parameter;
///
/// // Parameter JSON definition
/// // `my_parameter.py` should contain a Python class.
/// let data = r#"{
///     "type": "Python",
///     "name": "my-custom-calculation",
///     "path": "my_parameter.py",
///     "object": "MyParameter",
///     "args": [],
///     "kwargs": {},
///     "metrics": {
///         "a_keyword": {
///             "type": "Parameter",
///             "name": "another-parameter"
///         },
///         "volume": {
///             "type": "NodeVolume",
///             "name": "a-reservoir"
///         }
///     }
/// }"#;
///
/// let parameter: Parameter = serde_json::from_str(data).unwrap();
/// ```
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct PythonParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    #[serde(flatten)]
    pub module: PythonModule,
    /// The name of Python object from the module to use.
    pub object: String,
    /// Is this a multi-valued parameter or not. If true then the calculation method should
    /// return a dictionary with string keys and either floats or ints as values.
    #[serde(default)]
    pub multi: bool,
    /// Position arguments to pass to the object during setup.
    pub args: Vec<serde_json::Value>,
    /// Keyword arguments to pass to the object during setup.
    pub kwargs: HashMap<String, serde_json::Value>,
    /// Metric values to pass to the calculation method of the initialised object (i.e.
    /// values that the Python calculation is dependent on).
    pub metrics: Option<HashMap<String, DynamicFloatValue>>,
    /// Index values to pass to the calculation method of the initialised object (i.e.
    /// indices that the Python calculation is dependent on).
    pub indices: Option<HashMap<String, DynamicIndexValue>>,
}

pub fn try_json_value_into_py(py: Python, value: &serde_json::Value) -> Result<Option<PyObject>, SchemaError> {
    let py_value = match value {
        Value::Null => None,
        Value::Bool(v) => Some(v.into_py(py)),
        Value::Number(v) => {
            if let Some(i) = v.as_i64() {
                Some(i.into_py(py))
            } else if let Some(f) = v.as_f64() {
                Some(f.into_py(py))
            } else {
                panic!("Could not convert JSON number to Python type.");
            }
        }
        Value::String(v) => Some(v.into_py(py)),
        Value::Array(array) => Some(
            array
                .iter()
                .map(|v| try_json_value_into_py(py, v).unwrap())
                .collect::<Vec<_>>()
                .into_py(py),
        ),
        Value::Object(map) => Some(
            map.iter()
                .map(|(k, v)| (k, try_json_value_into_py(py, v).unwrap()))
                .collect::<HashMap<_, _>>()
                .into_py(py),
        ),
    };

    Ok(py_value)
}

impl PythonParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        HashMap::new()
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<ParameterType, SchemaError> {
        pyo3::prepare_freethreaded_python();

        let object = Python::with_gil(|py| {
            let module = match &self.module {
                PythonModule::Module(module) => PyModule::import(py, module.as_str()),
                PythonModule::Path(original_path) => {
                    let path = &make_path(original_path, data_path);
                    let code = std::fs::read_to_string(path).expect("Could not read Python code from path.");
                    let file_name = path.file_name().unwrap().to_str().unwrap();
                    let module_name = path.file_stem().unwrap().to_str().unwrap();

                    PyModule::from_code(py, &code, file_name, module_name)
                }
            }?;

            Ok(module.getattr(self.object.as_str())?.into())
        })
        .map_err(|e: PyErr| SchemaError::PythonError(e.to_string()))?;

        let args = Python::with_gil(|py| {
            PyTuple::new(py, self.args.iter().map(|arg| try_json_value_into_py(py, arg).unwrap())).into_py(py)
        });

        let kwargs = Python::with_gil(|py| {
            let seq = PyTuple::new(
                py,
                self.kwargs
                    .iter()
                    .map(|(k, v)| (k.into_py(py), try_json_value_into_py(py, v).unwrap())),
            );

            PyDict::from_sequence(py, seq.to_object(py)).unwrap().into_py(py)
        });

        let metrics = match &self.metrics {
            Some(metrics) => metrics
                .iter()
                .map(|(k, v)| {
                    Ok((
                        k.to_string(),
                        v.load(network, schema, domain, tables, data_path, inter_network_transfers)?,
                    ))
                })
                .collect::<Result<HashMap<_, _>, SchemaError>>()?,
            None => HashMap::new(),
        };

        let indices = match &self.indices {
            Some(indices) => indices
                .iter()
                .map(|(k, v)| {
                    Ok((
                        k.to_string(),
                        v.load(network, schema, domain, tables, data_path, inter_network_transfers)?,
                    ))
                })
                .collect::<Result<HashMap<_, _>, SchemaError>>()?,
            None => HashMap::new(),
        };

        let p = PyParameter::new(&self.meta.name, object, args, kwargs, &metrics, &indices);
        let pt = if self.multi {
            ParameterType::Multi(network.add_multi_value_parameter(Box::new(p))?)
        } else {
            ParameterType::Parameter(network.add_parameter(Box::new(p))?)
        };

        Ok(pt)
    }
}

#[cfg(test)]
mod tests {
    use crate::data_tables::LoadedTableCollection;
    use crate::model::PywrNetwork;
    use crate::parameters::python::PythonParameter;
    use pywr_core::models::ModelDomain;
    use pywr_core::network::Network;
    use pywr_core::test_utils::default_time_domain;
    use serde_json::json;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_python_parameter() {
        let dir = tempdir().unwrap();

        let file_path = dir.path().join("my_parameter.py");

        let data = json!(
            {
                "name": "my-custom-calculation",
                "type": "Python",
                "path": file_path,
                "object": "MyParameter",
                "args": [0, ],
                "kwargs": {},
            }
        )
        .to_string();

        let mut file = File::create(file_path).unwrap();
        write!(
            file,
            r#"
class MyParameter:
    def __init__(self, count, *args, **kwargs):
        self.count = 0

    def calc(self, ts, si, p_values):
        self.count += si
        return float(self.count + ts.day)
"#
        )
        .unwrap();

        // Init Python
        pyo3::prepare_freethreaded_python();
        // Load the schema ...
        let param: PythonParameter = serde_json::from_str(data.as_str()).unwrap();
        // ... add it to an empty network
        // this should trigger loading the module and extracting the class
        let domain: ModelDomain = default_time_domain().into();
        let schema = PywrNetwork::default();
        let mut network = Network::default();
        let tables = LoadedTableCollection::from_schema(None, None).unwrap();
        param
            .add_to_model(&mut network, &schema, &domain, &tables, None, &[])
            .unwrap();
    }
}
