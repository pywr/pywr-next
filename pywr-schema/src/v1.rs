//! Conversion traits for converting from Pywr v1 to v2 schema.
//!
//! This module contains traits for converting from Pywr v1 schema to Pywr v2 schema.
//! The traits are implemented for all types that can be converted. Due to differences in
//! the schemas some conversions may extract [`Parameter`]s and [`Timeseries`] from the
//! original data. This is primarily due to the fact that the v2 schema does not support
//! inline (and unnamed) parameters, and includes a separate timeseries section.
//!
//! The struct [`ConversionData`] is used to store these extracted parameters and timeseries.
//! It also tracks a count of unnamed parameters and timeseries. This is used during conversion
//! of meta-data to provide a unique name for unnamed parameters and timeseries.

use std::collections::HashMap;

use crate::ConversionError;
use crate::error::ComponentConversionError;
use crate::metric::Metric;
use crate::nodes::{NodeMeta, StorageInitialVolume};
use crate::parameters::{ConstantFloatVec, Parameter, ParameterMeta};
use crate::timeseries::Timeseries;
use pywr_v1_schema::nodes::NodeMeta as NodeMetaV1;
use pywr_v1_schema::parameters::{
    ExternalDataRef, ParameterMeta as ParameterMetaV1, ParameterValue, ParameterValues, TableDataRef,
};

/// Counters for unnamed parameters and timeseries.
#[derive(Default)]
pub struct ConversionData {
    unnamed_count: usize,
    pub virtual_nodes: Vec<String>,
    pub parameters: Vec<Parameter>,
    pub timeseries: Vec<Timeseries>,
}

impl ConversionData {
    pub fn reset_count(&mut self) {
        self.unnamed_count = 0;
    }
}

pub trait TryFromV1<T>: Sized {
    type Error;
    fn try_from_v1(v1: T, parent_node: Option<&str>, conversion_data: &mut ConversionData)
    -> Result<Self, Self::Error>;
}
impl<T, U> TryFromV1<Option<U>> for Option<T>
where
    T: TryFromV1<U>,
{
    type Error = T::Error;
    fn try_from_v1(
        v1: Option<U>,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        v1.map(|v| v.try_into_v2(parent_node, conversion_data)).transpose()
    }
}

pub trait TryIntoV2<T> {
    type Error;
    fn try_into_v2(self, parent_node: Option<&str>, conversion_data: &mut ConversionData) -> Result<T, Self::Error>;
}

// TryFromV1Parameter implies TryIntoV2Parameter
impl<T, U> TryIntoV2<U> for T
where
    U: TryFromV1<T>,
{
    type Error = U::Error;

    fn try_into_v2(self, parent_node: Option<&str>, conversion_data: &mut ConversionData) -> Result<U, Self::Error> {
        U::try_from_v1(self, parent_node, conversion_data)
    }
}

/// Convert v1 metadata tags (arbitrary JSON values) to v2 tags (string values).
///
/// Returns an error if any tag value is not a string.
pub(crate) fn convert_tags(
    tags: Option<HashMap<String, serde_json::Value>>,
) -> Result<HashMap<String, String>, ConversionError> {
    tags.unwrap_or_default()
        .into_iter()
        .map(|(k, v)| match v {
            serde_json::Value::String(s) => Ok((k, s)),
            other => Err(ConversionError::UnexpectedType {
                expected: "String".to_string(),
                actual: other.to_string(),
            }),
        })
        .collect()
}

impl TryFromV1<ParameterMetaV1> for ParameterMeta {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: ParameterMetaV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let name = v1.name.unwrap_or_else(|| {
            let pname = match parent_node {
                Some(pn) => format!("{pn}-p{}", conversion_data.unnamed_count),
                None => format!("unnamed-{}", conversion_data.unnamed_count),
            };
            conversion_data.unnamed_count += 1;
            pname
        });

        let tags = convert_tags(v1.tags).map_err(|error| {
            Box::new(ComponentConversionError::Parameter {
                attr: "tags".to_string(),
                name: name.clone(),
                error,
            })
        })?;

        Ok(Self {
            name,
            comment: v1.comment,
            tags,
        })
    }
}

impl TryFromV1<Option<ParameterMetaV1>> for ParameterMeta {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: Option<ParameterMetaV1>,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        match v1 {
            Some(meta) => meta.try_into_v2(parent_node, conversion_data),
            None => {
                let meta = Self {
                    name: format!("unnamed-{}", conversion_data.unnamed_count),
                    comment: None,
                    tags: HashMap::new(),
                };
                conversion_data.unnamed_count += 1;
                Ok(meta)
            }
        }
    }
}

/// Helper function to convert a node attribute from v1 to v2.
pub fn try_convert_node_attr<V1, V2>(
    name: &str,
    attr: &str,
    v1_value: V1,
    parent_node: Option<&str>,
    conversion_data: &mut ConversionData,
) -> Result<V2, Box<ComponentConversionError>>
where
    V1: TryIntoV2<V2, Error = ConversionError>,
{
    v1_value
        .try_into_v2(parent_node.or(Some(name)), conversion_data)
        .map_err(|error| {
            Box::new(ComponentConversionError::Node {
                attr: attr.to_string(),
                name: name.to_string(),
                error,
            })
        })
}

/// Helper function to convert a parameter attribute from v1 to v2.
pub fn try_convert_parameter_attr<V1, V2>(
    name: &str,
    attr: &str,
    v1_value: V1,
    parent_node: Option<&str>,
    conversion_data: &mut ConversionData,
) -> Result<V2, Box<ComponentConversionError>>
where
    V1: TryIntoV2<V2, Error = ConversionError>,
{
    v1_value
        .try_into_v2(parent_node.or(Some(name)), conversion_data)
        .map_err(|error| {
            Box::new(ComponentConversionError::Parameter {
                attr: attr.to_string(),
                name: name.to_string(),
                error,
            })
        })
}

/// Helper function to convert node meta from v1 to v2.
///
/// Returns an error if any tag value is not a string.
pub fn try_convert_node_meta(v1: NodeMetaV1) -> Result<NodeMeta, Box<ComponentConversionError>> {
    let name = v1.name.clone();
    NodeMeta::try_from(v1).map_err(|error| {
        Box::new(ComponentConversionError::Node {
            attr: "tags".to_string(),
            name,
            error,
        })
    })
}

/// Helper function to convert initial storage from v1 to v2.
pub fn try_convert_initial_storage(
    name: &str,
    attr: &str,
    v1_initial_volume: Option<f64>,
    v1_initial_volume_pc: Option<f64>,
) -> Result<StorageInitialVolume, Box<ComponentConversionError>> {
    let initial_volume = if let Some(volume) = v1_initial_volume {
        StorageInitialVolume::Absolute { volume }
    } else if let Some(proportion) = v1_initial_volume_pc {
        StorageInitialVolume::Proportional { proportion }
    } else {
        return Err(Box::new(ComponentConversionError::Node {
            attr: attr.to_string(),
            name: name.to_string(),
            error: ConversionError::MissingAttribute {
                attrs: vec!["initial_volume".to_string(), "initial_volume_pc".to_string()],
            },
        }));
    };

    Ok(initial_volume)
}

pub fn try_convert_values(
    name: &str,
    v1_values: Option<Vec<f64>>,
    v1_external: Option<ExternalDataRef>,
    v1_table_ref: Option<TableDataRef>,
) -> Result<ConstantFloatVec, Box<ComponentConversionError>> {
    let values: ConstantFloatVec = if let Some(values) = v1_values {
        ConstantFloatVec::Literal { values }
    } else if let Some(_external) = v1_external {
        return Err(Box::new(ComponentConversionError::Parameter {
            name: name.to_string(),
            attr: "url".to_string(),
            error: ConversionError::UnsupportedFeature {
                feature: "External data references are not supported in Pywr v2. Please use a table instead."
                    .to_string(),
            },
        }));
    } else if let Some(table_ref) = v1_table_ref {
        ConstantFloatVec::Table(
            table_ref
                .try_into()
                .map_err(|error| ComponentConversionError::Parameter {
                    name: name.to_string(),
                    attr: "table".to_string(),
                    error,
                })?,
        )
    } else {
        return Err(Box::new(ComponentConversionError::Parameter {
            name: name.to_string(),
            attr: "table".to_string(),
            error: ConversionError::MissingAttribute {
                attrs: vec!["values".to_string(), "table".to_string(), "url".to_string()],
            },
        }));
    };

    Ok(values)
}

pub fn try_convert_control_curves(
    name: &str,
    v1_control_curves: Option<ParameterValues>,
    v1_control_curve: Option<ParameterValue>,
    parent_node: Option<&str>,
    conversion_data: &mut ConversionData,
) -> Result<Vec<Metric>, Box<ComponentConversionError>> {
    let control_curves = if let Some(control_curves) = v1_control_curves {
        control_curves
            .into_iter()
            .map(|p| try_convert_parameter_attr(name, "control_curves", p, parent_node, conversion_data))
            .collect::<Result<Vec<_>, _>>()?
    } else if let Some(control_curve) = v1_control_curve {
        vec![try_convert_parameter_attr(
            name,
            "control_curve",
            control_curve,
            parent_node,
            conversion_data,
        )?]
    } else {
        return Err(Box::new(ComponentConversionError::Parameter {
            name: name.to_string(),
            attr: "control_curves".to_string(),
            error: ConversionError::MissingAttribute {
                attrs: vec!["control_curves".to_string(), "control_curve".to_string()],
            },
        }));
    };

    Ok(control_curves)
}

#[cfg(test)]
mod tests {
    use super::convert_tags;
    use crate::ConversionError;
    use std::collections::HashMap;

    #[test]
    fn convert_tags_none_is_empty() {
        assert!(convert_tags(None).unwrap().is_empty());
    }

    #[test]
    fn convert_tags_string_values_ok() {
        let tags = HashMap::from([("group".to_string(), serde_json::json!("supply"))]);
        let converted = convert_tags(Some(tags)).unwrap();
        assert_eq!(converted.get("group").map(String::as_str), Some("supply"));
    }

    #[test]
    fn convert_tags_non_string_value_errors() {
        let tags = HashMap::from([("priority".to_string(), serde_json::json!(5))]);
        let err = convert_tags(Some(tags)).unwrap_err();
        assert!(matches!(err, ConversionError::UnexpectedType { .. }));
    }
}
