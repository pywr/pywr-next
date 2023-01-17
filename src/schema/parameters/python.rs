use crate::parameters::py::PyParameter;
use crate::schema::data_tables::LoadedTableCollection;
use crate::schema::parameters::{DynamicFloatValue, DynamicFloatValueType, ParameterMeta};
use crate::{ParameterIndex, PywrError};
use pyo3::prelude::PyModule;
use pyo3::{PyErr, Python};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum PythonModule {
    Module(String),
    Path(PathBuf),
}

/// A Parameter that uses a Python class for its calculations.
///
///
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct PythonParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    #[serde(flatten)]
    pub module: PythonModule,
    pub object: String,
    pub parameters: Option<Vec<DynamicFloatValue>>,
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
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<ParameterIndex, PywrError> {
        let object = Python::with_gil(|py| {
            let module = match &self.module {
                PythonModule::Module(module) => PyModule::import(py, module.as_str()),
                PythonModule::Path(path) => {
                    let code = std::fs::read_to_string(path).expect("Could not read Python code from path.");
                    let file_name = path.file_name().unwrap().to_str().unwrap();
                    let module_name = path.file_stem().unwrap().to_str().unwrap();

                    PyModule::from_code(py, &code, file_name, module_name)
                }
            }?;

            Ok(module.getattr(self.object.as_str())?.into())
        })
        .map_err(|e: PyErr| PywrError::PythonError(e.to_string()))?;

        let parameters = match &self.parameters {
            Some(parameters) => parameters
                .iter()
                .map(|v| v.load(model, tables, data_path))
                .collect::<Result<Vec<_>, _>>()?,
            None => Vec::new(),
        };

        let p = PyParameter::new(&self.meta.name, object, &parameters);
        model.add_parameter(Box::new(p))
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Model;
    use crate::schema::data_tables::LoadedTableCollection;
    use crate::schema::parameters::python::PythonParameter;
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
                "name": "my-custom-class",
                "type": "Python",
                "path": file_path,
                "object": "MyParameter"
            }
        )
        .to_string();

        let mut file = File::create(file_path).unwrap();
        write!(
            file,
            r#"
class MyParameter:
    def __init__(self):
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
        // ... add it to an empty model
        // this should trigger loading the module and extracting the class
        let mut model = Model::default();
        let tables = LoadedTableCollection::from_schema(None, None).unwrap();
        param.add_to_model(&mut model, &tables, None).unwrap();
    }
}
