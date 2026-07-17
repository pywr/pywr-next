use crate::error::ComponentConversionError;
use crate::metric::{Metric, NodeComponentReference};
use crate::node_attribute_subset_enum;
use crate::nodes::NodeMeta;
use crate::parameters::Parameter;
use crate::v1::{ConversionData, TryFromV1, try_convert_node_attr, try_convert_node_meta, try_convert_parameter_attr};
#[cfg(feature = "core")]
use crate::{error::SchemaError, network::LoadArgs, nodes::NodeAttribute};
#[cfg(feature = "core")]
use pywr_core::{
    aggregated_node::{ExclusivityBuilder, RelationshipBuilder},
    metric::UnresolvedMetricF64,
    node::UnresolvedNode,
};
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

    pub fn default_attribute(&self) -> AggregatedNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl AggregatedNode {
    fn name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, None)
    }

    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        // Create the aggregated node builder
        let mut agg_node = pywr_core::AggregatedNodeBuilder::new(self.name());

        // Add the nodes
        for node_ref in &self.nodes {
            let node = args
                .schema
                .get_node_by_name(&node_ref.name)
                .ok_or_else(|| SchemaError::NodeNotFound {
                    name: node_ref.name.to_string(),
                })?;

            // Add each node's nodes to the aggregated node.
            agg_node.nodes(node.nodes_for_flow_constraints(node_ref.component)?);
        }

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args, Some(&self.meta.name))?;
            agg_node.max_flow(value);
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args, Some(&self.meta.name))?;
            agg_node.min_flow(value);
        }

        if let Some(relationship) = &self.relationship {
            let r: Box<dyn RelationshipBuilder> = match relationship {
                Relationship::Proportion { factors } => {
                    let mut r = pywr_core::aggregated_node::ProportionalFactorsBuilder::default();

                    for factor in factors {
                        r.factor(factor.load(network, args, Some(&self.meta.name))?);
                    }

                    Box::new(r)
                }
                Relationship::Ratio { factors } => {
                    let mut r = pywr_core::aggregated_node::RatioFactorsBuilder::default();

                    for factor in factors {
                        r.factor(factor.load(network, args, Some(&self.meta.name))?);
                    }

                    Box::new(r)
                }
                Relationship::Coefficients { factors, rhs } => {
                    let mut r = pywr_core::aggregated_node::CoefficientFactorsBuilder::default();

                    for factor in factors {
                        r.factor(factor.load(network, args, Some(&self.meta.name))?);
                    }

                    if let Some(rhs_value) = rhs {
                        let rhs = rhs_value.load(network, args, Some(&self.meta.name))?;
                        r.rhs(rhs);
                    }

                    Box::new(r)
                }
                Relationship::Exclusive { min_active, max_active } => {
                    let mut r = ExclusivityBuilder::default();

                    if let Some(min_active) = min_active {
                        r.min_active(*min_active);
                    }

                    if let Some(max_active) = max_active {
                        r.max_active(*max_active);
                    }
                    Box::new(r)
                }
            };

            agg_node.relationship(r);
        }

        network.agg_node(agg_node);
        Ok(())
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let name = self.name();

        let metric = match attr {
            AggregatedNodeAttribute::Outflow => UnresolvedMetricF64::AggregatedNodeOutFlow(name),
            AggregatedNodeAttribute::Inflow => UnresolvedMetricF64::AggregatedNodeInFlow(name),
        };

        Ok(metric)
    }
}

impl TryFromV1<AggregatedNodeV1> for AggregatedNode {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: AggregatedNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = try_convert_node_meta(v1.meta)?;

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

    pub fn default_attribute(&self) -> AggregatedStorageNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl AggregatedStorageNode {
    fn name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, None)
    }

    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
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

                node.nodes_for_storage_constraints()
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        let mut agg_storage_node = pywr_core::AggregatedStorageNodeBuilder::new(self.name());

        for node in nodes {
            agg_storage_node.node(node);
        }

        network.agg_storage_node(agg_storage_node);
        Ok(())
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let name = self.name();

        let metric = match attr {
            AggregatedStorageNodeAttribute::Volume => UnresolvedMetricF64::AggregatedStorageNodeVolume(name),
            AggregatedStorageNodeAttribute::ProportionalVolume => {
                UnresolvedMetricF64::AggregatedStorageNodeProportionalVolume(name)
            }
        };

        Ok(metric)
    }
}

impl TryFrom<AggregatedStorageNodeV1> for AggregatedStorageNode {
    type Error = Box<ComponentConversionError>;

    fn try_from(v1: AggregatedStorageNodeV1) -> Result<Self, Self::Error> {
        let storage_nodes = v1.storage_nodes.into_iter().map(|n| n.into()).collect();

        Ok(Self {
            meta: try_convert_node_meta(v1.meta)?,
            parameters: None,
            storage_nodes,
        })
    }
}
