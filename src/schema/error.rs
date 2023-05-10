use thiserror::Error;

#[derive(Error, Debug, PartialEq, Eq)]
pub enum SchemaError {}

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
