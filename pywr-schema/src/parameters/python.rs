#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::{IndexMetric, Metric};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{DynamicFloatValueType, ParameterMeta};
use crate::py_utils::PythonSource;
#[cfg(all(feature = "core", feature = "pyo3"))]
use crate::py_utils::{try_load_optional_py_args, try_load_optional_py_kwargs};
use crate::visit::{VisitMetrics, VisitPaths};
#[cfg(all(feature = "core", feature = "pyo3"))]
use pyo3::{
    Bound, Python,
    prelude::{Py, PyAny, PyAnyMethods, PyModule},
};
#[cfg(feature = "core")]
use pywr_core::parameters::ParameterType;
#[cfg(all(feature = "core", feature = "pyo3"))]
use pywr_core::parameters::{ParameterName, PyClassParameter, PyFuncParameter};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use schemars::JsonSchema;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

/// The type of Python object that is expected to be used in the parameter.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, Display, EnumDiscriminants, PywrVisitAll)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(PythonObjectType))]
pub enum PythonObject {
    Class { class: String },
    Function { function: String },
}

#[cfg(all(feature = "core", feature = "pyo3"))]
impl PythonObject {
    fn load_object(&self, module: &Bound<PyModule>) -> Result<PyObj, SchemaError> {
        let obj = match &self {
            PythonObject::Class { class } => {
                let obj = module.getattr(class)?;
                PyObj::Class(obj.unbind())
            }
            PythonObject::Function { function } => {
                let func = module.getattr(function)?;
                PyObj::Function(func.unbind())
            }
        };

        Ok(obj)
    }
}

#[cfg(all(feature = "core", feature = "pyo3"))]
enum PyObj {
    Class(Py<PyAny>),
    Function(Py<PyAny>),
}

/// The expected return type of the Python parameter.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default, JsonSchema, Display, EnumIter)]
pub enum PythonReturnType {
    #[default]
    Float,
    Int,
    Dict,
}

/// A Parameter that uses a Python object for its calculations.
///
/// This struct defines a schema for loading a [`PyClassParameter`] or [`PyFuncParameter`] from external
/// sources. The user provides the name of an object in the given module. Typically, this object will be
/// a class or function the user has written. For more information on the expected format and signature of
/// this object please refer to the [`PyClassParameter`] documentation. If a class is provided
/// then it is initialised with user provided positional and/or keyword arguments that can be provided
/// here. If a function is provided then it is called with the same arguments along with an `info`
/// object (see below).
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
/// // `my_parameter.py` should contain a Python class called `MyParameter`.
/// let data = r#"{
///     "type": "Python",
///     "meta": {
///         "name": "my-custom-calculation"
///     },
///     "source": {
///         "type": "Path",
///         "path": "my_parameter.py"
///     },
///     "object": {
///         "type": "Class",
///         "class": "MyParameter"
///     },
///     "args": [],
///     "kwargs": {},
///     "metrics": {
///         "a_keyword": {
///             "type": "Parameter",
///             "name": "another-parameter"
///         },
///         "volume": {
///             "type": "Node",
///             "name": "a-reservoir",
///             "attribute": "Volume"
///         }
///     }
/// }"#;
///
/// let parameter: Parameter = serde_json::from_str(data).unwrap();
/// ```
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PythonParameter {
    pub meta: ParameterMeta,
    pub source: PythonSource,
    /// The type of Python object from the module to use. This is either a class or a function.
    pub object: PythonObject,
    /// The return type of the Python calculation. This is used to convert the Python return value
    /// to the appropriate type for the Parameter.
    #[serde(default)]
    pub return_type: PythonReturnType,
    /// Position arguments to pass to the object during setup.
    pub args: Option<Vec<Value>>,
    /// Keyword arguments to pass to the object during setup.
    pub kwargs: Option<HashMap<String, Value>>,
    /// Metric values to pass to the calculation method of the initialised object (i.e.
    /// values that the Python calculation is dependent on).
    pub metrics: Option<HashMap<String, Metric>>,
    /// Index values to pass to the calculation method of the initialised object (i.e.
    /// indices that the Python calculation is dependent on).
    pub indices: Option<HashMap<String, IndexMetric>>,
}

impl VisitMetrics for PythonParameter {
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        if let Some(metrics) = &self.metrics {
            for metric in metrics.values() {
                visitor(metric);
            }
        }
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        if let Some(metrics) = &mut self.metrics {
            for metric in metrics.values_mut() {
                visitor(metric);
            }
        }
    }
}

impl VisitPaths for PythonParameter {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        match &self.source {
            PythonSource::Module { .. } => {}
            PythonSource::Path { path } => {
                visitor(path);
            }
        }

        self.visit_metrics(&mut |metric| {
            metric.visit_paths(visitor);
        });
    }

    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        match &mut self.source {
            PythonSource::Module { .. } => {}
            PythonSource::Path { path } => {
                visitor(path);
            }
        }
    }
}

impl PythonParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }
    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType<'_>> {
        HashMap::new()
    }
}

#[cfg(all(feature = "core", not(feature = "pyo3")))]
impl PythonParameter {
    pub fn add_to_model(
        &self,
        _network: &mut pywr_core::network::Network,
        _args: &LoadArgs,
        _parent: Option<&str>,
    ) -> Result<ParameterType, SchemaError> {
        Err(SchemaError::FeatureNotEnabled("pyo3".to_string()))
    }
}
#[cfg(all(feature = "core", feature = "pyo3"))]
impl PythonParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterType, SchemaError> {
        Python::initialize();

        let object = Python::attach(|py| {
            let module = self.source.load_module(py, args.data_path)?;
            let obj = self.object.load_object(&module)?;

            Ok::<_, SchemaError>(obj)
        })?;

        let py_args = Python::attach(|py| try_load_optional_py_args(py, &self.args))?;
        let py_kwargs = Python::attach(|py| try_load_optional_py_kwargs(py, &self.kwargs))?;

        let metrics = match &self.metrics {
            Some(metrics) => metrics
                .iter()
                .map(|(k, v)| Ok((k.to_string(), v.load(network, args, None)?)))
                .collect::<Result<HashMap<_, _>, SchemaError>>()?,
            None => HashMap::new(),
        };

        let indices = match &self.indices {
            Some(indices) => indices
                .iter()
                .map(|(k, v)| Ok((k.to_string(), v.load(network, args, None)?)))
                .collect::<Result<HashMap<_, _>, SchemaError>>()?,
            None => HashMap::new(),
        };

        let pt = match object {
            PyObj::Class(py_class) => {
                let p = PyClassParameter::new(
                    ParameterName::new(&self.meta.name, parent),
                    py_class,
                    py_args,
                    py_kwargs,
                    &metrics,
                    &indices,
                );

                match self.return_type {
                    PythonReturnType::Float => network.add_parameter(Box::new(p))?.into(),
                    PythonReturnType::Int => ParameterType::Index(network.add_index_parameter(Box::new(p))?),
                    PythonReturnType::Dict => ParameterType::Multi(network.add_multi_value_parameter(Box::new(p))?),
                }
            }
            PyObj::Function(py_function) => {
                let p = PyFuncParameter::new(
                    ParameterName::new(&self.meta.name, parent),
                    py_function,
                    py_args,
                    py_kwargs,
                    &metrics,
                    &indices,
                );

                match self.return_type {
                    PythonReturnType::Float => network.add_parameter(Box::new(p))?.into(),
                    PythonReturnType::Int => ParameterType::Index(network.add_index_parameter(Box::new(p))?),
                    PythonReturnType::Dict => ParameterType::Multi(network.add_multi_value_parameter(Box::new(p))?),
                }
            }
        };

        Ok(pt)
    }
}

#[cfg(test)]
#[cfg(all(feature = "core", feature = "pyo3"))]
mod tests {
    use crate::data_tables::LoadedTableCollection;
    use crate::network::{LoadArgs, NetworkSchema};
    use crate::parameters::python::PythonParameter;
    use crate::timeseries::LoadedTimeseriesCollection;
    use pyo3::Python;
    use pywr_core::models::ModelDomain;
    use pywr_core::network::Network;
    use pywr_core::test_utils::default_time_domain;
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn test_python_float_parameter() {
        let mut py_fn = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        py_fn.push("tests/test_parameters.py");

        let data = json!(
            {
                "meta": {
                    "name": "my-float-parameter"
                },
                "source": {
                    "type": "Path",
                    "path": py_fn
                },
                "object": {
                    "type": "Class",
                    "class": "FloatParameter"
                },
                "args": [0, ],
                "kwargs": {},
            }
        )
        .to_string();

        // Init Python
        Python::initialize();
        // Load the schema ...
        let param: PythonParameter = serde_json::from_str(data.as_str()).unwrap();
        // ... add it to an empty network
        // this should trigger loading the module and extracting the class
        let domain: ModelDomain = default_time_domain().into();
        let schema = NetworkSchema::default();
        let mut network = Network::default();
        let tables = LoadedTableCollection::from_schema(None, None).unwrap();
        let ts = LoadedTimeseriesCollection::default();

        let args = LoadArgs {
            schema: &schema,
            data_path: None,
            tables: &tables,
            timeseries: &ts,
            domain: &domain,
            inter_network_transfers: &[],
        };

        param.add_to_model(&mut network, &args, None).unwrap();

        assert!(network.get_parameter_by_name(&"my-float-parameter".into()).is_some());
    }

    #[test]
    fn test_python_int_parameter() {
        let mut py_fn = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        py_fn.push("tests/test_parameters.py");

        let data = json!(
            {
                "meta": {
                    "name": "my-int-parameter"
                },
                "source": {
                    "type": "Path",
                    "path": py_fn
                },
                "return_type": "Int",
                "object": {
                    "type": "Class",
                    "class": "FloatParameter"
                },
                "args": [0, ],
                "kwargs": {},
            }
        )
        .to_string();

        // Init Python
        Python::initialize();
        // Load the schema ...
        let param: PythonParameter = serde_json::from_str(data.as_str()).unwrap();
        // ... add it to an empty network
        // this should trigger loading the module and extracting the class
        let domain: ModelDomain = default_time_domain().into();
        let schema = NetworkSchema::default();
        let mut network = Network::default();
        let tables = LoadedTableCollection::from_schema(None, None).unwrap();
        let ts = LoadedTimeseriesCollection::default();

        let args = LoadArgs {
            schema: &schema,
            data_path: None,
            tables: &tables,
            timeseries: &ts,
            domain: &domain,
            inter_network_transfers: &[],
        };

        param.add_to_model(&mut network, &args, None).unwrap();

        assert!(
            network
                .get_index_parameter_by_name(&"my-int-parameter".into())
                .is_some()
        );
    }
}
