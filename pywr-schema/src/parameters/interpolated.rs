use crate::data_tables::LoadedTableCollection;
use crate::error::SchemaError;
use crate::model::PywrMultiNetworkTransfer;
use crate::parameters::{
    DynamicFloatValue, DynamicFloatValueType, IntoV2Parameter, MetricFloatReference, MetricFloatValue, NodeReference,
    ParameterMeta, TryFromV1Parameter, TryIntoV2Parameter,
};
use crate::timeseries::LoadedTimeseriesCollection;
use crate::ConversionError;
use pywr_core::models::ModelDomain;
use pywr_core::parameters::ParameterIndex;
use pywr_v1_schema::parameters::{
    InterpolatedFlowParameter as InterpolatedFlowParameterV1,
    InterpolatedVolumeParameter as InterpolatedVolumeParameterV1,
};
use std::collections::HashMap;
use std::path::Path;

/// A parameter that interpolates a value to a function with given discrete data points.
///
/// Internally this is implemented as a piecewise linear interpolation via
/// [`pywr_core::parameters::InterpolatedParameter`].
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct InterpolatedParameter {
    #[serde(flatten)]
    pub meta: ParameterMeta,
    pub x: DynamicFloatValue,
    pub xp: Vec<DynamicFloatValue>,
    pub fp: Vec<DynamicFloatValue>,
    /// If not given or true, raise an error if the x value is outside the range of the data points.
    pub error_on_bounds: Option<bool>,
}

impl InterpolatedParameter {
    pub fn node_references(&self) -> HashMap<&str, &str> {
        HashMap::new()
    }

    pub fn parameters(&self) -> HashMap<&str, DynamicFloatValueType> {
        let mut attributes = HashMap::new();

        let x = &self.x;
        attributes.insert("x", x.into());

        let xp = &self.xp;
        attributes.insert("xp", xp.into());

        let fp = &self.fp;
        attributes.insert("fp", fp.into());

        attributes
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
        timeseries: &LoadedTimeseriesCollection,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let x = self.x.load(
            network,
            schema,
            domain,
            tables,
            data_path,
            inter_network_transfers,
            timeseries,
        )?;

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
            .map(|p| {
                p.load(
                    network,
                    schema,
                    domain,
                    tables,
                    data_path,
                    inter_network_transfers,
                    timeseries,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let fp = self
            .fp
            .iter()
            .map(|p| {
                p.load(
                    network,
                    schema,
                    domain,
                    tables,
                    data_path,
                    inter_network_transfers,
                    timeseries,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;

        let points = xp.into_iter().zip(fp).collect::<Vec<_>>();

        let p = pywr_core::parameters::InterpolatedParameter::new(
            &self.meta.name,
            x,
            points,
            self.error_on_bounds.unwrap_or(true),
        );
        Ok(network.add_parameter(Box::new(p))?)
    }
}

impl TryFromV1Parameter<InterpolatedFlowParameterV1> for InterpolatedParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: InterpolatedFlowParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        // Convert the node reference to a metric
        let node_ref = NodeReference {
            name: v1.node,
            attribute: None,
        };
        // This defaults to v2's default metric
        let x = DynamicFloatValue::Dynamic(MetricFloatValue::Reference(MetricFloatReference::Node(node_ref)));

        let xp = v1
            .flows
            .into_iter()
            .map(|p| p.try_into_v2_parameter(Some(&meta.name), unnamed_count))
            .collect::<Result<Vec<_>, _>>()?;

        let fp = v1
            .values
            .into_iter()
            .map(|p| p.try_into_v2_parameter(Some(&meta.name), unnamed_count))
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
                        return Err(ConversionError::UnsupportedFeature {
                            feature: "Interpolation with `kind` other than `linear` is not supported.".to_string(),
                            name: meta.name.clone(),
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

impl TryFromV1Parameter<InterpolatedVolumeParameterV1> for InterpolatedParameter {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: InterpolatedVolumeParameterV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2_parameter(parent_node, unnamed_count);

        // Convert the node reference to a metric
        let node_ref = NodeReference {
            name: v1.node,
            attribute: None,
        };
        // This defaults to the node's inflow; not sure if we can do better than that.
        let x = DynamicFloatValue::Dynamic(MetricFloatValue::Reference(MetricFloatReference::Node(node_ref)));

        let xp = v1
            .volumes
            .into_iter()
            .map(|p| p.try_into_v2_parameter(Some(&meta.name), unnamed_count))
            .collect::<Result<Vec<_>, _>>()?;

        let fp = v1
            .values
            .into_iter()
            .map(|p| p.try_into_v2_parameter(Some(&meta.name), unnamed_count))
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
                        return Err(ConversionError::UnsupportedFeature {
                            feature: "Interpolation with `kind` other than `linear` is not supported.".to_string(),
                            name: meta.name.clone(),
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
