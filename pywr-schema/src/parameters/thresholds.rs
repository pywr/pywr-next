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
use pywr_core::parameters::{ParameterName, ParameterType};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use pywr_v1_schema::parameters::{
    MultipleThresholdIndexParameter as MultiThresholdIndexParameterV1,
    MultipleThresholdParameterIndexParameter as MultipleThresholdParameterIndexParameterV1,
    NodeThresholdParameter as NodeThresholdParameterV1, ParameterThresholdParameter as ParameterThresholdParameterV1,
    Predicate as PredicateV1, StorageThresholdParameter as StorageThresholdParameterV1,
};
use schemars::JsonSchema;
use strum_macros::{Display, EnumIter};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, JsonSchema, PywrVisitAll, Display, EnumIter)]
pub enum Predicate {
    #[serde(alias = "<")]
    LT,
    #[serde(alias = ">")]
    GT,
    #[serde(alias = "==")]
    EQ,
    #[serde(alias = "<=")]
    LE,
    #[serde(alias = ">=")]
    GE,
}

impl From<PredicateV1> for Predicate {
    fn from(v1: PredicateV1) -> Self {
        match v1 {
            PredicateV1::LT => Predicate::LT,
            PredicateV1::GT => Predicate::GT,
            PredicateV1::EQ => Predicate::EQ,
            PredicateV1::LE => Predicate::LE,
            PredicateV1::GE => Predicate::GE,
        }
    }
}

#[cfg(feature = "core")]
impl From<Predicate> for pywr_core::parameters::Predicate {
    fn from(p: Predicate) -> Self {
        match p {
            Predicate::LT => pywr_core::parameters::Predicate::LessThan,
            Predicate::GT => pywr_core::parameters::Predicate::GreaterThan,
            Predicate::EQ => pywr_core::parameters::Predicate::EqualTo,
            Predicate::LE => pywr_core::parameters::Predicate::LessThanOrEqualTo,
            Predicate::GE => pywr_core::parameters::Predicate::GreaterThanOrEqualTo,
        }
    }
}

/// A parameter that compares a metric against a threshold metric
///
/// The metric is compared using the given predicate and the result is returned as an index. If the comparison
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
#[skip_serializing_none]
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
        parent: Option<&str>,
    ) -> Result<ParameterType, SchemaError> {
        let metric = self.metric.load(network, args, None)?;
        let threshold = self.threshold.load(network, args, None)?;

        // If the parameter has returned metrics, we need to create a sub-name for the threshold parameter
        // so that it can be distinguished from the indexed array parameter.
        // If it does not have returned metrics, we can just use the parameter name as the
        // name for the threshold parameter.
        let name = if self.returned_metrics.is_some() {
            ParameterName::new_with_subname(&self.meta.name, Some("threshold"), Some(&self.meta.name))
        } else {
            ParameterName::new(&self.meta.name, parent)
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
                    ParameterName::new(&self.meta.name, parent),
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
                        });
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

        let metric = Metric::Node(NodeAttrReference::new(v1.node, None));

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
                        });
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

        let metric = Metric::Node(NodeAttrReference::new(v1.storage_node, None));

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
                        });
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

/// A parameter that compares a metric against a multiple threshold metrics.
///
/// The metric is compared to the thresholds in order using the given predicate and the result is
/// returned as an index of the first threshold that passed the comparison. I.e. if the first
/// metric passes the comparison against the first threshold, the index is 0, if it passes the
/// comparison against the second threshold, the index is 1, and so on.
///
/// When values are provided for the `returned_metrics` attribute, these values are returned instead
/// of the index.
///
/// The parameter has different representations in core depending on the `returned_metrics` attribute. If values are
/// set for `returned_metrics` then two parameters are added to the model. The first a
/// [`pywr_core::parameters::MultiThresholdParameter`], which is set as the index parameter of a
/// [`pywr_core::parameters::IndexedArrayParameter`] containing the `returned_metrics`
/// values.
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct MultiThresholdParameter {
    pub meta: ParameterMeta,
    /// The metric to compare against the threshold.
    pub metric: Metric,
    /// The thresholds to compare against.
    pub thresholds: Vec<Metric>,
    /// The comparison predicate. Should be one of `LT`, `GT`, `EQ`, `LE`, or `GE` or their equivalents `<`, `>`, `==`,
    /// `<=` or `>=`.
    pub predicate: Predicate,
    /// Optional metrics returned by the parameter.
    pub returned_metrics: Option<Vec<Metric>>,
    /// If true, the threshold comparison remains at the highest position ever reached.
    #[serde(default)]
    pub ratchet: bool,
}

#[cfg(feature = "core")]
impl MultiThresholdParameter {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<ParameterType, SchemaError> {
        let metric = self.metric.load(network, args, None)?;
        let thresholds = self
            .thresholds
            .iter()
            .map(|v| v.load(network, args, None))
            .collect::<Result<Vec<_>, _>>()?;

        let name = if self.returned_metrics.is_some() {
            ParameterName::new_with_subname(&self.meta.name, Some("threshold"), Some(&self.meta.name))
        } else {
            self.meta.name.as_str().into()
        };

        let p = pywr_core::parameters::MultiThresholdParameter::new(
            name,
            metric,
            &thresholds,
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
                    ParameterName::new(&self.meta.name, parent),
                    p_idx.into(),
                    &metrics,
                );
                Ok(network.add_parameter(Box::new(values_param))?.into())
            }
            None => Ok(p_idx.into()),
        }
    }
}

impl TryFromV1<MultiThresholdIndexParameterV1> for MultiThresholdParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: MultiThresholdIndexParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let metric = Metric::Node(NodeAttrReference::new(v1.node, None));

        let thresholds = v1
            .thresholds
            .into_iter()
            .map(|v1_t| try_convert_parameter_attr(&meta.name, "thresholds", v1_t, parent_node, conversion_data))
            .collect::<Result<Vec<_>, _>>()?;

        let p = Self {
            meta,
            metric,
            returned_metrics: None,
            thresholds,
            predicate: Predicate::GE, // v1 assumed GE and this was not configurable
            ratchet: false,
        };
        Ok(p)
    }
}

impl TryFromV1<MultipleThresholdParameterIndexParameterV1> for MultiThresholdParameter {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: MultipleThresholdParameterIndexParameterV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: ParameterMeta = v1.meta.into_v2(parent_node, conversion_data);

        let metric = try_convert_parameter_attr(&meta.name, "parameter", v1.parameter, parent_node, conversion_data)?;

        let thresholds = v1
            .thresholds
            .into_iter()
            .map(|v1_t| try_convert_parameter_attr(&meta.name, "thresholds", v1_t, parent_node, conversion_data))
            .collect::<Result<Vec<_>, _>>()?;

        let p = Self {
            meta,
            metric,
            returned_metrics: None,
            thresholds,
            predicate: Predicate::GE, // v1 assumed GE and this was not configurable
            ratchet: false,
        };
        Ok(p)
    }
}
