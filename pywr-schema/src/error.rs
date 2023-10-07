use crate::data_tables::TableError;
use pyo3::exceptions::PyRuntimeError;
use pyo3::PyErr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SchemaError {
    #[error("IO error: {0}")]
    IO(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("node with name {0} not found")]
    NodeNotFound(String),
    #[error("parameter {0} not found")]
    ParameterNotFound(String),
    #[error("missing initial volume for node: {0}")]
    MissingInitialVolume(String),
    #[error("Pywr core error: {0}")]
    PywrCore(#[from] pywr_core::PywrError),
    #[error("data table error: {0}")]
    DataTable(#[from] TableError),
    #[error("Circular node reference(s) found.")]
    CircularNodeReference,
    #[error("Circular parameters reference(s) found.")]
    CircularParameterReference,
    #[error("unsupported file format")]
    UnsupportedFileFormat,
    #[error("Python error: {0}")]
    PythonError(String),
    #[error("invalid date format description")]
    InvalidDateFormatDescription(#[from] time::error::InvalidFormatDescription),
    #[error("failed to parse date")]
    DateParse(#[from] time::error::Parse),
    #[error("invalid date component range")]
    InvalidDateComponentRange(#[from] time::error::ComponentRange),
    #[error("hdf5 error: {0}")]
    HDF5Error(String),
    #[error("csv error: {0}")]
    CSVError(String),
    #[error("unexpected parameter type: {0}")]
    UnexpectedParameterType(String),
}

impl From<SchemaError> for PyErr {
    fn from(err: SchemaError) -> PyErr {
        PyRuntimeError::new_err(err.to_string())
    }
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum ConversionError {
    #[error("Error converting {attr:?} on node {name:?}")]
    NodeAttribute {
        attr: String,
        name: String,
        source: Box<ConversionError>,
    },
    #[error("Constant float value cannot be a parameter reference.")]
    ConstantFloatReferencesParameter,
    #[error("Constant float value cannot be an inline parameter.")]
    ConstantFloatInlineParameter,
    #[error("Missing one of the following attributes {attrs:?} on parameter {name:?}.")]
    MissingAttribute { attrs: Vec<String>, name: String },
    #[error("Unexpected the following attributes {attrs:?} on parameter {name:?}.")]
    UnexpectedAttribute { attrs: Vec<String>, name: String },
    #[error("Can not convert a float constant to an index constant.")]
    FloatToIndex,
    #[error("Attribute {attr:?} is not allowed on node {name:?}.")]
    ExtraNodeAttribute { attr: String, name: String },
}
