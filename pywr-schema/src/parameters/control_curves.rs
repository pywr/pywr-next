#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::error::{ComponentConversionError, ConversionError};
use crate::metric::{Metric, NodeAttrReference, VirtualNodeAttrReference};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::nodes::NodeAttribute;
use crate::parameters::{ConversionData, ParameterMeta};
use crate::v1::{IntoV2, TryFromV1, try_convert_control_curves, try_convert_parameter_attr};

#[cfg(feature = "core")]
use pywr_core::parameters::{ParameterIndex, ParameterName};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use pywr_v1_schema::parameters::{
    ControlCurveIndexParameter as ControlCurveIndexParameterV1,
    ControlCurveInterpolatedParameter as ControlCurveInterpolatedParameterV1,
    ControlCurveParameter as ControlCurveParameterV1,
    ControlCurvePiecewiseInterpolatedParameter as ControlCurvePiecewiseInterpolatedParameterV1,
};
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct ControlCurveInterpolatedParameter {
    pub meta: ParameterMeta,
    pub control_curves: Vec<Metric>,
    pub storage_metric: Metric,
    pub values: Vec<Metric>,
}

#[cfg(feature = "core")]
impl ControlCurveInterpolatedParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let metric = self.storage_metric.load(network, args, None)?;

        let control_curves = self
            .control_curves
            .iter()
            .map(|cc| cc.load(network, args, None))
            .collect::<Result<_, _>>()?;

        let values = self
            .values
            .iter()
            .map(|val| val.load(network, args, None))
            .collect::<Result<_, _>>()?;

        let p = pywr_core::parameters::ControlCurveInterpolatedParameter::new(
            ParameterName::new(&self.meta.name, parent),
            metric,
            control_curves,
            values,
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1<ControlCurveInterpolatedParameterV1> for ControlCurveInterpolatedParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: ControlCurveInterpolatedParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let control_curves = try_convert_control_curves(
            &meta.name,
            v1.control_curves,
            v1.control_curve,
            parent_node,
            conversion_data,
        )?;

        // Handle the case where neither or both "values" and "parameters" are defined.
        let values = match (v1.values, v1.parameters) {
            (None, None) => {
                return Err(ComponentConversionError::Parameter {
                    name: meta.name,
                    attr: "control_curves".to_string(),
                    error: ConversionError::MissingAttribute {
                        attrs: vec!["values".to_string(), "parameters".to_string()],
                    },
                });
            }
            (Some(_), Some(_)) => {
                return Err(ComponentConversionError::Parameter {
                    name: meta.name,
                    attr: "control_curves".to_string(),
                    error: ConversionError::UnexpectedAttribute {
                        attrs: vec!["values".to_string(), "parameters".to_string()],
                    },
                });
            }
            (Some(values), None) => values.into_iter().map(Metric::from).collect(),
            (None, Some(parameters)) => parameters
                .into_iter()
                .map(|p| try_convert_parameter_attr(&meta.name, "parameters", p, parent_node, conversion_data))
                .collect::<Result<Vec<_>, _>>()?,
        };

        // v1 uses proportional volume for control curves
        let storage_metric = if conversion_data.virtual_nodes.contains(&v1.storage_node) {
            VirtualNodeAttrReference {
                name: v1.storage_node,
                attribute: Some(NodeAttribute::ProportionalVolume),
            }
            .into()
        } else {
            NodeAttrReference {
                name: v1.storage_node,
                attribute: Some(NodeAttribute::ProportionalVolume),
            }
            .into()
        };

        let p = Self {
            meta,
            control_curves,
            storage_metric,
            values,
        };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct ControlCurveIndexParameter {
    pub meta: ParameterMeta,
    pub control_curves: Vec<Metric>,
    pub storage_metric: Metric,
}

#[cfg(feature = "core")]
impl ControlCurveIndexParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<u64>, SchemaError> {
        let metric = self.storage_metric.load(network, args, None)?;

        let control_curves = self
            .control_curves
            .iter()
            .map(|cc| cc.load(network, args, None))
            .collect::<Result<_, _>>()?;

        let p = pywr_core::parameters::ControlCurveIndexParameter::new(
            ParameterName::new(&self.meta.name, parent),
            metric,
            control_curves,
        );
        Ok(network.add_index_parameter(Box::new(p))?)
    }
}

impl TryFromV1<ControlCurveIndexParameterV1> for ControlCurveIndexParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: ControlCurveIndexParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let control_curves = v1
            .control_curves
            .into_iter()
            .map(|p| try_convert_parameter_attr(&meta.name, "control_curves", p, parent_node, conversion_data))
            .collect::<Result<Vec<_>, _>>()?;

        // v1 uses proportional volume for control curves
        let storage_metric = if conversion_data.virtual_nodes.contains(&v1.storage_node) {
            VirtualNodeAttrReference {
                name: v1.storage_node,
                attribute: Some(NodeAttribute::ProportionalVolume),
            }
            .into()
        } else {
            NodeAttrReference {
                name: v1.storage_node,
                attribute: Some(NodeAttribute::ProportionalVolume),
            }
            .into()
        };

        let p = Self {
            meta,
            control_curves,
            storage_metric,
        };
        Ok(p)
    }
}

/// Pywr v1.x ControlCurveParameter can be an index parameter if it is not given "values"
/// or "parameters" keys.
impl TryFromV1<ControlCurveParameterV1> for ControlCurveIndexParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: ControlCurveParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let control_curves = try_convert_control_curves(
            &meta.name,
            v1.control_curves,
            v1.control_curve,
            parent_node,
            conversion_data,
        )?;

        if v1.values.is_some() || v1.parameters.is_some() {
            return Err(ComponentConversionError::Parameter {
                name: meta.name,
                attr: "values".to_string(),
                error: ConversionError::UnexpectedAttribute {
                    attrs: vec!["values".to_string(), "parameters".to_string()],
                },
            });
        };

        // v1 uses proportional volume for control curves
        let storage_node = NodeAttrReference {
            name: v1.storage_node,
            attribute: Some(NodeAttribute::ProportionalVolume),
        };

        let p = Self {
            meta,
            control_curves,
            storage_metric: storage_node.into(),
        };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct ControlCurveParameter {
    pub meta: ParameterMeta,
    pub control_curves: Vec<Metric>,
    pub storage_metric: Metric,
    pub values: Vec<Metric>,
}

#[cfg(feature = "core")]
impl ControlCurveParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let metric = self.storage_metric.load(network, args, None)?;

        let control_curves = self
            .control_curves
            .iter()
            .map(|cc| cc.load(network, args, None))
            .collect::<Result<_, _>>()?;

        let values = self
            .values
            .iter()
            .map(|val| val.load(network, args, None))
            .collect::<Result<_, _>>()?;

        let p = pywr_core::parameters::ControlCurveParameter::new(
            ParameterName::new(&self.meta.name, parent),
            metric,
            control_curves,
            values,
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1<ControlCurveParameterV1> for ControlCurveParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: ControlCurveParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let control_curves = try_convert_control_curves(
            &meta.name,
            v1.control_curves,
            v1.control_curve,
            parent_node,
            conversion_data,
        )?;

        let values = if let Some(values) = v1.values {
            values.into_iter().map(Metric::from).collect()
        } else if let Some(parameters) = v1.parameters {
            parameters
                .into_iter()
                .map(|p| try_convert_parameter_attr(&meta.name, "parameters", p, parent_node, conversion_data))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            return Err(ComponentConversionError::Parameter {
                name: meta.name,
                attr: "values".to_string(),
                error: ConversionError::MissingAttribute {
                    attrs: vec!["values".to_string(), "parameters".to_string()],
                },
            });
        };

        // v1 uses proportional volume for control curves
        let storage_metric = if conversion_data.virtual_nodes.contains(&v1.storage_node) {
            VirtualNodeAttrReference {
                name: v1.storage_node,
                attribute: Some(NodeAttribute::ProportionalVolume),
            }
            .into()
        } else {
            NodeAttrReference {
                name: v1.storage_node,
                attribute: Some(NodeAttribute::ProportionalVolume),
            }
            .into()
        };

        let p = Self {
            meta,
            control_curves,
            storage_metric,
            values,
        };
        Ok(p)
    }
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct ControlCurvePiecewiseInterpolatedParameter {
    pub meta: ParameterMeta,
    pub control_curves: Vec<Metric>,
    pub storage_metric: Metric,
    pub values: Option<Vec<[f64; 2]>>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
}

#[cfg(feature = "core")]
impl ControlCurvePiecewiseInterpolatedParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let metric = self.storage_metric.load(network, args, None)?;

        let control_curves = self
            .control_curves
            .iter()
            .map(|cc| cc.load(network, args, None))
            .collect::<Result<_, _>>()?;

        let values = match &self.values {
            None => Vec::new(),
            Some(values) => values.clone(),
        };

        let p = pywr_core::parameters::PiecewiseInterpolatedParameter::new(
            ParameterName::new(&self.meta.name, parent),
            metric,
            control_curves,
            values,
            self.maximum.unwrap_or(1.0),
            self.minimum.unwrap_or(0.0),
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1<ControlCurvePiecewiseInterpolatedParameterV1> for ControlCurvePiecewiseInterpolatedParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: ControlCurvePiecewiseInterpolatedParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let control_curves = try_convert_control_curves(
            &meta.name,
            v1.control_curves,
            v1.control_curve,
            parent_node,
            conversion_data,
        )?;

        // v1 uses proportional volume for control curves
        let storage_node = if conversion_data.virtual_nodes.contains(&v1.storage_node) {
            VirtualNodeAttrReference {
                name: v1.storage_node,
                attribute: Some(NodeAttribute::ProportionalVolume),
            }
            .into()
        } else {
            NodeAttrReference {
                name: v1.storage_node,
                attribute: Some(NodeAttribute::ProportionalVolume),
            }
            .into()
        };

        let p = Self {
            meta,
            control_curves,
            storage_metric: storage_node,
            values: v1.values,
            minimum: v1.minimum,
            maximum: None,
        };
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        metric::{Metric, NodeAttrReference},
        parameters::control_curves::ControlCurvePiecewiseInterpolatedParameter,
    };

    #[test]
    fn test_control_curve_piecewise_interpolated() {
        let data = r#"
            {
                "meta": {
                    "name": "My control curve",
                    "comment": "A witty comment"
                },
                "storage_metric": {
                    "type": "Node",
                    "name": "storage1",
                    "attribute": "ProportionalVolume"
                },
                "control_curves": [
                    {"type": "Parameter", "name": "reservoir_cc"},
                    {"type": "Literal", "value": 0.2}
                ],
                "values": [
                    [-0.1, -1.0],
                    [-100, -200],
                    [-300, -400]
                ],
                "minimum": 0.05
            }
            "#;

        let param: ControlCurvePiecewiseInterpolatedParameter = serde_json::from_str(data).unwrap();

        assert_eq!(
            param.storage_metric,
            Metric::Node(NodeAttrReference {
                name: "storage1".to_string(),
                attribute: Some(crate::nodes::NodeAttribute::ProportionalVolume),
            })
        );
    }
}
