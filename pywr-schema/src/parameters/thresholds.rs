use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::{Metric, NodeReference};
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::parameters::{ConversionData, ParameterMeta};
use crate::predicate::Predicate;
use crate::v1::{try_convert_parameter_attr, IntoV2, TryFromV1};
use crate::ConversionError;
#[cfg(feature = "core")]
use pywr_core::parameters::{ParameterName, ParameterType};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::{
    NodeThresholdParameter as NodeThresholdParameterV1, ParameterThresholdParameter as ParameterThresholdParameterV1,
    StorageThresholdParameter as StorageThresholdParameterV1,
};
use schemars::JsonSchema;

/// A parameter that compares a metric against a threshold metric
///
/// The metrics are compared using the given predicate and the result is returned as an index. If the comparison
/// evaluates to true the index is 1, otherwise it is 0. When values are provided for the `returned_metrics` attribute,
/// these values are returned instead of the index. If the predicate comparison evaluates to false the first value is
/// returned, if it is true the second value is returned.
///
/// The parameter has different representations in core depending on the `returned_metrics` attribute. If values are
/// set for `returned_metrics` then two parameters are added to the model. The first a
/// [`pywr_core::parameters::ThresholdParameter`], which is set as the index parameter of a
/// [`pywr_core::parameters::IndexedArrayParameter`] containing the `returned_metrics`
/// values.
///
/// # Examples
///
/// ```JSON
#[doc = include_str!("doc_examples/threshold_returned_values1.json")]
/// ```
/// Note that the name specified in the model JSON for the parameter in this example is assigned to the core
/// `IndexedArrayParameter`. The core `ThresholdParameter` is given an additional sub-name of `threshold`.
///
/// An equivalent representation could be achieved by defining the two parameters separately in the model JSON:
/// ```JSON
#[doc = include_str!("doc_examples/threshold_returned_values2.json")]
/// ```
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct ThresholdParameter {
    pub meta: ParameterMeta,
    /// The metric to compare against the threshold.
    pub metric: Metric,
    /// The threshold to compare against.
    pub threshold: Metric,
    /// The comparison predicate. Should be one of `LT`, `GT`, `EQ`, `LE`, or `GE` or their equivalents `<`, `>`, `==`,
    /// `<=` or `>=`.
    pub predicate: Predicate,
    /// Optional metrics returned by the parameter. If the metric comparison evaluates to false the parameter returns
    /// the first metric, if it is true the second metric is returned.
    pub returned_metrics: Option<[Metric; 2]>,
    /// If true, the threshold comparison remains true once it has evaluated to true once.
    #[serde(default)]
    pub ratchet: bool,
}

#[cfg(feature = "core")]
impl ThresholdParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<ParameterType, SchemaError> {
        let metric = self.metric.load(network, args, None)?;
        let threshold = self.threshold.load(network, args, None)?;

        let name = if self.returned_metrics.is_some() {
            ParameterName::new("threshold", Some(&self.meta.name))
        } else {
            self.meta.name.as_str().into()
        };

        let p = pywr_core::parameters::ThresholdParameter::new(
            name,
            metric,
            threshold,
            self.predicate.into(),
            self.ratchet,
        );

        let p_idx = network.add_index_parameter(Box::new(p))?;

        match self.returned_metrics {
            Some(ref values) => {
                let metrics = values
                    .iter()
                    .map(|v| v.load(network, args, None))
                    .collect::<Result<Vec<_>, _>>()?;
                let values_param = pywr_core::parameters::IndexedArrayParameter::new(
                    self.meta.name.as_str().into(),
                    p_idx.into(),
                    &metrics,
                );
                Ok(network.add_parameter(Box::new(values_param))?.into())
            }
            None => Ok(p_idx.into()),
        }
    }
}

impl TryFromV1<ParameterThresholdParameterV1> for ThresholdParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: ParameterThresholdParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let metric = try_convert_parameter_attr(&meta.name, "parameter", v1.parameter, parent_node, conversion_data)?;
        let threshold =
            try_convert_parameter_attr(&meta.name, "threshold", v1.threshold, parent_node, conversion_data)?;

        let returned_metrics: Option<[Metric; 2]> = match v1.values {
            Some(v) => {
                let values: Vec<Metric> = v.into_iter().map(Metric::from).collect();
                match values.try_into() {
                    Ok(array) => Some(array),
                    Err(v) => {
                        return Err(ComponentConversionError::Parameter {
                            name: meta.name.to_string(),
                            attr: "values".to_string(),
                            error: ConversionError::IncorrectNumberOfValues {
                                expected: 2,
                                found: v.len(),
                            },
                        })
                    }
                }
            }
            None => None,
        };

        let p = Self {
            meta,
            metric,
            returned_metrics,
            threshold,
            predicate: v1.predicate.into(),
            ratchet: false,
        };
        Ok(p)
    }
}

impl TryFromV1<NodeThresholdParameterV1> for ThresholdParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: NodeThresholdParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let metric = Metric::Node(NodeReference::new(v1.node, None));

        let threshold =
            try_convert_parameter_attr(&meta.name, "threshold", v1.threshold, parent_node, conversion_data)?;

        let returned_metrics: Option<[Metric; 2]> = match v1.values {
            Some(v) => {
                let values: Vec<Metric> = v.into_iter().map(Metric::from).collect();
                match values.try_into() {
                    Ok(array) => Some(array),
                    Err(v) => {
                        return Err(ComponentConversionError::Parameter {
                            name: meta.name.to_string(),
                            attr: "values".to_string(),
                            error: ConversionError::IncorrectNumberOfValues {
                                expected: 2,
                                found: v.len(),
                            },
                        })
                    }
                }
            }
            None => None,
        };

        let p = Self {
            meta,
            metric,
            returned_metrics,
            threshold,
            predicate: v1.predicate.into(),
            ratchet: false,
        };
        Ok(p)
    }
}

impl TryFromV1<StorageThresholdParameterV1> for ThresholdParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: StorageThresholdParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let metric = Metric::Node(NodeReference::new(v1.storage_node, None));

        let returned_metrics: Option<[Metric; 2]> = match v1.values {
            Some(v) => {
                let values: Vec<Metric> = v.into_iter().map(Metric::from).collect();
                match values.try_into() {
                    Ok(array) => Some(array),
                    Err(v) => {
                        return Err(ComponentConversionError::Parameter {
                            name: meta.name.to_string(),
                            attr: "values".to_string(),
                            error: ConversionError::IncorrectNumberOfValues {
                                expected: 2,
                                found: v.len(),
                            },
                        })
                    }
                }
            }
            None => None,
        };

        let threshold =
            try_convert_parameter_attr(&meta.name, "threshold", v1.threshold, parent_node, conversion_data)?;

        let p = Self {
            meta,
            metric,
            returned_metrics,
            threshold,
            predicate: v1.predicate.into(),
            ratchet: false,
        };
        Ok(p)
    }
}
