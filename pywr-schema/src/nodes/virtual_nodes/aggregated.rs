use crate::error::ComponentConversionError;
use crate::error::SchemaError;
use crate::metric::{Metric, NodeComponentReference};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::node_attribute_subset_enum;
#[cfg(feature = "core")]
use crate::nodes::NodeAttribute;
use crate::nodes::NodeMeta;
use crate::parameters::Parameter;
use crate::v1::{ConversionData, TryFromV1, try_convert_node_attr, try_convert_parameter_attr};
#[cfg(feature = "core")]
use pywr_core::{derived_metric::DerivedMetric, metric::MetricF64};
use pywr_schema_macros::PywrVisitAll;
use pywr_schema_macros::skip_serializing_none;
use pywr_v1_schema::nodes::{AggregatedNode as AggregatedNodeV1, AggregatedStorageNode as AggregatedStorageNodeV1};
use schemars::JsonSchema;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

/// Defines the relationship between nodes in an `AggregatedNode`.
///
/// - `Proportion`: The factors represent the proportion of the total flow that each node after
///   the first should receive. The first node will have the residual flow. Factors should sum to a total
///   less than 1.0, and there should be one less factor than the number of nodes.
/// - `Ratio`: The factors represent the ratio of flow between the nodes. There should be factors
///   equal to the number of nodes, and the factors should be non-negative.
/// - `Coefficients`: The factors represent coefficients in a linear equation, with an optional
///   right-hand side. For example, for three nodes A and B with coefficients 2 and 3, and a
///   right-hand side of 100, the equation would be `2*A + 3*B = 100`. If no right-hand side
///   is provided, it is assumed to be 0, i.e. `2*A + 3*B = 0`. Currently, this is limited to
///   a maximum of 2 nodes.
/// - `Exclusive`: Only a limited number of nodes can be active at any one time. The `min_active`
///   and `max_active` parameters define the minimum and maximum number of nodes that can be active
///   at any one time. If not specified, `min_active` defaults to 0 and `max_active` defaults to 1.
///   This relationship requires binary variables to be added to the model, so may increase
///   solve times.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(RelationshipType))]
pub enum Relationship {
    Proportion {
        factors: Vec<Metric>,
    },
    Ratio {
        factors: Vec<Metric>,
    },
    Coefficients {
        factors: Vec<Metric>,
        rhs: Option<Metric>,
    },
    Exclusive {
        min_active: Option<u64>,
        max_active: Option<u64>,
    },
}

// This macro generates a subset enum for the `AggregatedNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum AggregatedNodeAttribute {
        Inflow,
        Outflow,
    }
}

/// A node that can apply flow constraints across multiple nodes in the model.
///
/// This node can apply constraints to a set of nodes to a maximum and minimum flow. Those
/// constraints can be set via the following optional fields:
///
/// - `max_flow`: The maximum total flow through the set of nodes.
/// - `min_flow`: The minimum total flow through the set of nodes.
/// - `relationship`: The relationship between the nodes, such as a proportion, ratio or exclusive.
///
/// When specifying the set of `nodes` to aggregate, the `component` field can be used to specify
/// which component of the node to use. If not specified, the default component is used.
///
/// # Available attributes and components
///
/// The enum [`AggregatedNodeAttribute`] defines the available attributes. There are no components
/// to choose from.
///
/// # Examples
///
/// An example using a relationship with ratio factors:
/// ```json
#[doc = include_str!("doc_examples/aggregated_rel_ratio.json")]
/// ```
///
/// An example using a relationship with proportion factors:
/// ```json
#[doc = include_str!("doc_examples/aggregated_rel_proportional.json")]
/// ```
///
/// An example using a relationship with coefficient factors:
/// ```json
#[doc = include_str!("doc_examples/aggregated_rel_coef.json")]
/// ```
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct AggregatedNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub nodes: Vec<NodeComponentReference>,
    pub max_flow: Option<Metric>,
    pub min_flow: Option<Metric>,
    pub relationship: Option<Relationship>,
}

impl AggregatedNode {
    const DEFAULT_ATTRIBUTE: AggregatedNodeAttribute = AggregatedNodeAttribute::Outflow;

    pub fn input_connectors(&self) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        // Not connectable
        // TODO this should be a trait? And error if you try to connect to a non-connectable node.
        Ok(vec![])
    }

    pub fn output_connectors(&self) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        // Not connectable
        Ok(vec![])
    }

    pub fn default_attribute(&self) -> AggregatedNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl AggregatedNode {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        let nodes: Vec<Vec<_>> = self
            .nodes
            .iter()
            .map(|node_ref| {
                let node = args
                    .schema
                    .get_node_by_name(&node_ref.name)
                    .ok_or_else(|| SchemaError::NodeNotFound {
                        name: node_ref.name.to_string(),
                    })?;

                node.node_indices_for_flow_constraints(network, node_ref.component)
            })
            .collect::<Result<Vec<_>, _>>()?;

        // We initialise with no factors, but will update them in the `set_constraints` method
        // once all the parameters are loaded.
        network.add_aggregated_node(self.meta.name.as_str(), None, nodes.as_slice(), None)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args, Some(&self.meta.name))?;
            network.set_aggregated_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args, Some(&self.meta.name))?;
            network.set_aggregated_node_min_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(relationship) = &self.relationship {
            let r = match relationship {
                Relationship::Proportion { factors } => {
                    pywr_core::aggregated_node::Relationship::new_proportion_factors(
                        &factors
                            .iter()
                            .map(|f| f.load(network, args, Some(&self.meta.name)))
                            .collect::<Result<Vec<_>, _>>()?,
                    )
                }
                Relationship::Ratio { factors } => pywr_core::aggregated_node::Relationship::new_ratio_factors(
                    &factors
                        .iter()
                        .map(|f| f.load(network, args, Some(&self.meta.name)))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                Relationship::Coefficients { factors, rhs } => {
                    let rhs_value = match rhs {
                        Some(r) => Some(r.load(network, args, Some(&self.meta.name))?),
                        None => None,
                    };
                    pywr_core::aggregated_node::Relationship::new_coefficient_factors(
                        &factors
                            .iter()
                            .map(|f| f.load(network, args, Some(&self.meta.name)))
                            .collect::<Result<Vec<_>, _>>()?,
                        rhs_value,
                    )
                }
                Relationship::Exclusive { min_active, max_active } => {
                    pywr_core::aggregated_node::Relationship::new_exclusive(
                        min_active.unwrap_or(0),
                        max_active.unwrap_or(1),
                    )
                }
            };

            network.set_aggregated_node_relationship(self.meta.name.as_str(), None, Some(r))?;
        }

        Ok(())
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let idx = network
            .get_aggregated_node_index_by_name(self.meta.name.as_str(), None)
            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                name: self.meta.name.clone(),
                sub_name: None,
            })?;

        let metric = match attr {
            AggregatedNodeAttribute::Outflow => MetricF64::AggregatedNodeOutFlow(idx),
            AggregatedNodeAttribute::Inflow => MetricF64::AggregatedNodeInFlow(idx),
        };

        Ok(metric)
    }
}

impl TryFromV1<AggregatedNodeV1> for AggregatedNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: AggregatedNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        let relationship = match v1.factors {
            Some(f) => Some(Relationship::Ratio {
                factors: f
                    .into_iter()
                    .map(|v| {
                        try_convert_parameter_attr(
                            &meta.name,
                            "factors",
                            v,
                            parent_node.or(Some(&meta.name)),
                            conversion_data,
                        )
                    })
                    .collect::<Result<_, _>>()?,
            }),
            None => None,
        };

        let max_flow = try_convert_node_attr(&meta.name, "max_flow", v1.max_flow, parent_node, conversion_data)?;
        let min_flow = try_convert_node_attr(&meta.name, "min_flow", v1.min_flow, parent_node, conversion_data)?;

        let nodes = v1.nodes.into_iter().map(|n| n.into()).collect();

        let n = Self {
            meta,
            parameters: None,
            nodes,
            max_flow,
            min_flow,
            relationship,
        };
        Ok(n)
    }
}

// This macro generates a subset enum for the `AggregatedStorageNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum AggregatedStorageNodeAttribute {
        Volume,
        ProportionalVolume,
    }
}

/// A node that aggregates multiple storage nodes into a single node.
///
/// This node is used to represent a collection of storage nodes, such as reservoirs or aquifers,
/// that related to one another. It allows for the aggregation of the storage volumes to create
/// metrics and parameters that are based on the total storage of the aggregated nodes. For example,
/// a drought curves that are based on the total storage of a set of reservoirs.
///
/// This node will always use the storage component of the aggregated nodes. Currently, the
/// `component` field of the `NodeComponentReference` is ignored by this node. It is invalid
/// to specify a node that is not a storage node in the `storage_nodes` field.
///
/// # Available attributes and components
/// The enum [`AggregatedStorageNodeAttribute`] defines the available attributes. There are no components
/// to choose from.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct AggregatedStorageNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub storage_nodes: Vec<NodeComponentReference>,
}

impl AggregatedStorageNode {
    const DEFAULT_ATTRIBUTE: AggregatedStorageNodeAttribute = AggregatedStorageNodeAttribute::Volume;

    pub fn input_connectors(&self) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        // Not connectable
        // TODO this should be a trait? And error if you try to connect to a non-connectable node.
        Ok(vec![])
    }

    pub fn output_connectors(&self) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        // Not connectable
        Ok(vec![])
    }

    pub fn default_attribute(&self) -> AggregatedStorageNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl AggregatedStorageNode {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        let nodes = self
            .storage_nodes
            .iter()
            .map(|node_ref| {
                let node = args
                    .schema
                    .get_node_by_name(&node_ref.name)
                    .ok_or_else(|| SchemaError::NodeNotFound {
                        name: node_ref.name.to_string(),
                    })?;

                node.node_indices_for_storage_constraints(network)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();

        network.add_aggregated_storage_node(self.meta.name.as_str(), None, nodes)?;
        Ok(())
    }

    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let idx = network
            .get_aggregated_storage_node_index_by_name(self.meta.name.as_str(), None)
            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                name: self.meta.name.clone(),
                sub_name: None,
            })?;

        let metric = match attr {
            AggregatedStorageNodeAttribute::Volume => MetricF64::AggregatedNodeVolume(idx),
            AggregatedStorageNodeAttribute::ProportionalVolume => {
                let dm = DerivedMetric::AggregatedNodeProportionalVolume(idx);
                let derived_metric_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(derived_metric_idx)
            }
        };

        Ok(metric)
    }
}

impl From<AggregatedStorageNodeV1> for AggregatedStorageNode {
    fn from(v1: AggregatedStorageNodeV1) -> Self {
        let storage_nodes = v1.storage_nodes.into_iter().map(|n| n.into()).collect();

        Self {
            meta: v1.meta.into(),
            parameters: None,
            storage_nodes,
        }
    }
}
