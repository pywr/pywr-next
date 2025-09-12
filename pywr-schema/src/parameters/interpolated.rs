use crate::ConversionError;
use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::{Metric, NodeAttrReference};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::parameters::{ConversionData, ParameterMeta};
use crate::v1::{IntoV2, TryFromV1, try_convert_parameter_attr};

#[cfg(feature = "core")]
use pywr_core::parameters::{ParameterIndex, ParameterName};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use pywr_v1_schema::parameters::{
    InterpolatedFlowParameter as InterpolatedFlowParameterV1,
    InterpolatedVolumeParameter as InterpolatedVolumeParameterV1,
};
use schemars::JsonSchema;

/// A parameter that interpolates a value to a function with given discrete data points.
///
/// Internally this is implemented as a piecewise linear interpolation via
/// [`pywr_core::parameters::InterpolatedParameter`].
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct InterpolatedParameter {
    pub meta: ParameterMeta,
    pub x: Metric,
    pub xp: Vec<Metric>,
    pub fp: Vec<Metric>,
    /// If not given or true, raise an error if the x value is outside the range of the data points.
    pub error_on_bounds: Option<bool>,
}

#[cfg(feature = "core")]
impl InterpolatedParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let x = self.x.load(network, args, None)?;

        // Sense check the points
        if self.xp.len() != self.fp.len() {
            return Err(SchemaError::DataLengthMismatch {
                expected: self.xp.len(),
                found: self.fp.len(),
            });
        }

        let xp = self
            .xp
            .iter()
            .map(|p| p.load(network, args, None))
            .collect::<Result<Vec<_>, _>>()?;
        let fp = self
            .fp
            .iter()
            .map(|p| p.load(network, args, None))
            .collect::<Result<Vec<_>, _>>()?;

        let points = xp.into_iter().zip(fp).collect::<Vec<_>>();

        let p = pywr_core::parameters::InterpolatedParameter::new(
            ParameterName::new(&self.meta.name, parent),
            x,
            points,
            self.error_on_bounds.unwrap_or(true),
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1<InterpolatedFlowParameterV1> for InterpolatedParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: InterpolatedFlowParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        // Convert the node reference to a metric
        let node_ref = NodeAttrReference {
            name: v1.node,
            attribute: None,
        };
        // This defaults to v2's default metric
        let x = Metric::Node(node_ref);

        let xp = v1
            .flows
            .into_iter()
            .map(|p| try_convert_parameter_attr(&meta.name, "flows", p, parent_node, conversion_data))
            .collect::<Result<Vec<_>, _>>()?;

        let fp = v1
            .values
            .into_iter()
            .map(|p| try_convert_parameter_attr(&meta.name, "values", p, parent_node, conversion_data))
            .collect::<Result<Vec<_>, _>>()?;

        // Default values
        let mut error_on_bounds = None;
        if let Some(interp_kwargs) = v1.interp_kwargs {
            if let Some(error_on_bounds_value) = interp_kwargs.get("bounds_error") {
                // Try to get the value as a boolean;
                if let Some(eob) = error_on_bounds_value.as_bool() {
                    error_on_bounds = Some(eob);
                }
            }

            // Check if non-linear interpolation is requested; this is not supported at the moment.
            if let Some(kind) = interp_kwargs.get("kind") {
                if let Some(kind_str) = kind.as_str() {
                    if kind_str != "linear" {
                        return Err(ComponentConversionError::Parameter {
                            name: meta.name.clone(),
                            attr: "interp_kwargs".to_string(),
                            error: ConversionError::UnsupportedFeature {
                                feature: "Interpolation with `kind` other than `linear` is not supported.".to_string(),
                            },
                        });
                    }
                }
            }
        }

        Ok(Self {
            meta,
            x,
            xp,
            fp,
            error_on_bounds,
        })
    }
}

impl TryFromV1<InterpolatedVolumeParameterV1> for InterpolatedParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: InterpolatedVolumeParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        // Convert the node reference to a metric
        let node_ref = NodeAttrReference {
            name: v1.node,
            attribute: None,
        };
        // This defaults to the node's inflow; not sure if we can do better than that.
        let x = Metric::Node(node_ref);

        let xp = v1
            .volumes
            .into_iter()
            .map(|p| try_convert_parameter_attr(&meta.name, "volumes", p, parent_node, conversion_data))
            .collect::<Result<Vec<_>, _>>()?;

        let fp = v1
            .values
            .into_iter()
            .map(|p| try_convert_parameter_attr(&meta.name, "values", p, parent_node, conversion_data))
            .collect::<Result<Vec<_>, _>>()?;

        // Default values
        let mut error_on_bounds = None;
        if let Some(interp_kwargs) = v1.interp_kwargs {
            if let Some(error_on_bounds_value) = interp_kwargs.get("bounds_error") {
                // Try to get the value as a boolean;
                if let Some(eob) = error_on_bounds_value.as_bool() {
                    error_on_bounds = Some(eob);
                }
            }

            // Check if non-linear interpolation is requested; this is not supported at the moment.
            if let Some(kind) = interp_kwargs.get("kind") {
                if let Some(kind_str) = kind.as_str() {
                    if kind_str != "linear" {
                        return Err(ComponentConversionError::Parameter {
                            name: meta.name.clone(),
                            attr: "interp_kwargs".to_string(),
                            error: ConversionError::UnsupportedFeature {
                                feature: "Interpolation with `kind` other than `linear` is not supported.".to_string(),
                            },
                        });
                    }
                }
            }
        }

        Ok(Self {
            meta,
            x,
            xp,
            fp,
            error_on_bounds,
        })
    }
}
