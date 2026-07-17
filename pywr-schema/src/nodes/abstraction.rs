use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
#[cfg(feature = "core")]
use crate::nodes::{NodeAttribute, NodeComponent};
use crate::nodes::{NodeMeta, NodeSlot};
use crate::parameters::Parameter;
use crate::{mermaid, node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{metric::UnresolvedMetricF64, node::UnresolvedNode};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use schemars::JsonSchema;

// This macro generates a subset enum for the `AbstractionNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum AbstractionNodeAttribute {
        Inflow,
        Outflow,
        Abstraction,
    }
}

node_component_subset_enum! {
    pub enum AbstractionNodeComponent {
        Inflow,
        Outflow,
        Abstraction,
    }
}

pub enum AbstractionOutputNodeSlot {
    River,
    Abstraction,
}

impl From<AbstractionOutputNodeSlot> for NodeSlot {
    fn from(slot: AbstractionOutputNodeSlot) -> Self {
        match slot {
            AbstractionOutputNodeSlot::River => NodeSlot::River,
            AbstractionOutputNodeSlot::Abstraction => NodeSlot::Abstraction,
        }
    }
}

impl TryFrom<NodeSlot> for AbstractionOutputNodeSlot {
    type Error = SchemaError;

    fn try_from(slot: NodeSlot) -> Result<Self, Self::Error> {
        match slot {
            NodeSlot::River => Ok(AbstractionOutputNodeSlot::River),
            NodeSlot::Abstraction => Ok(AbstractionOutputNodeSlot::Abstraction),
            _ => Err(SchemaError::OutputNodeSlotNotSupported { slot }),
        }
    }
}

/// This node represents a river abstraction.
///
/// The abstraction can optionally be constrained by a minimum residual flow (MRF) requirement. If
/// this is defined an internal MRF node is created.
///
/// The node defines two output slots. The 'downstream' slot represents a continuation of the
/// river and the 'abstraction' slot is where the abstracted flow is directed.
///
///
#[doc = mermaid!("doc_diagrams/abstraction.mmd")]
///
/// # Available attributes and components
///
/// The enums [`AbstractionNodeAttribute`] and [`AbstractionNodeComponent`] define the available
/// attributes and components for this node.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct AbstractionNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    /// The MRF flow constraint.
    pub mrf: Option<Metric>,
    /// The MRF cost.
    pub mrf_cost: Option<Metric>,
    /// The abstraction cost.
    pub abs_cost: Option<Metric>,
    /// The maximum abstraction flow.
    pub abs_max_flow: Option<Metric>,
    /// The minimum abstraction flow.
    pub abs_min_flow: Option<Metric>,
}

impl AbstractionNode {
    const DEFAULT_ATTRIBUTE: AbstractionNodeAttribute = AbstractionNodeAttribute::Abstraction;
    const DEFAULT_COMPONENT: AbstractionNodeComponent = AbstractionNodeComponent::Abstraction;

    pub fn iter_output_slots(&self) -> impl Iterator<Item = NodeSlot> + '_ {
        [
            AbstractionOutputNodeSlot::River.into(),
            AbstractionOutputNodeSlot::Abstraction.into(),
        ]
        .into_iter()
    }

    pub fn default_attribute(&self) -> AbstractionNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> AbstractionNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl AbstractionNode {
    const DEFAULT_OUTPUT_SLOT: AbstractionOutputNodeSlot = AbstractionOutputNodeSlot::River;
    fn mrf_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("mrf"))
    }

    fn bypass_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("bypass"))
    }

    fn abstraction_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("abstraction"))
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            let mut connectors = vec![self.bypass_sub_name(), self.abstraction_sub_name()];
            if self.mrf.is_some() {
                connectors.push(self.mrf_sub_name());
            }
            Ok(connectors)
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        let slot = match slot {
            Some(s) => s.clone().try_into()?,
            None => Self::DEFAULT_OUTPUT_SLOT,
        };

        let indices = match slot {
            AbstractionOutputNodeSlot::River => {
                if self.mrf.is_some() {
                    vec![self.mrf_sub_name(), self.bypass_sub_name()]
                } else {
                    vec![self.bypass_sub_name()]
                }
            }
            AbstractionOutputNodeSlot::Abstraction => vec![self.abstraction_sub_name()],
        };

        Ok(indices)
    }
    pub fn nodes_for_flow_constraints(
        &self,
        component: Option<NodeComponent>,
    ) -> Result<Vec<UnresolvedNode>, SchemaError> {
        // Use the default component if none is specified
        let component = match component {
            Some(c) => c.try_into()?,
            None => Self::DEFAULT_COMPONENT,
        };

        let indices = match component {
            AbstractionNodeComponent::Inflow => {
                let mut indices = Vec::new();

                if self.mrf.is_some() {
                    indices.push(self.mrf_sub_name());
                }
                indices.push(self.bypass_sub_name());
                indices.push(self.abstraction_sub_name());
                indices
            }
            AbstractionNodeComponent::Outflow => {
                let mut indices = Vec::new();

                if self.mrf.is_some() {
                    indices.push(self.mrf_sub_name());
                }

                indices.push(self.bypass_sub_name());
                indices
            }
            AbstractionNodeComponent::Abstraction => {
                vec![self.abstraction_sub_name()]
            }
        };
        Ok(indices)
    }

    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(mrf) = &self.mrf {
            let mut mrf_node = pywr_core::node::NodeBuilder::link(self.mrf_sub_name());

            let value = mrf.load(network, args, Some(&self.meta.name))?;
            mrf_node.max_flow(value);

            if let Some(cost) = &self.mrf_cost {
                let value = cost.load(network, args, Some(&self.meta.name))?;
                mrf_node.cost(value);
            }

            network.node(mrf_node);
        } else if self.mrf_cost.is_some() {
            return Err(SchemaError::InvalidNodeAttributes {
                msg: format!(
                    "MRF cost defined but no MRF constraint provided for node '{}'",
                    self.meta.name
                ),
            });
        }

        let bypass_node = pywr_core::node::NodeBuilder::link(self.bypass_sub_name());
        let mut abstraction_node = pywr_core::node::NodeBuilder::link(self.abstraction_sub_name());

        if let Some(abs_max_flow) = &self.abs_max_flow {
            let value = abs_max_flow.load(network, args, Some(&self.meta.name))?;
            abstraction_node.max_flow(value);
        }

        if let Some(abs_min_flow) = &self.abs_min_flow {
            let value = abs_min_flow.load(network, args, Some(&self.meta.name))?;
            abstraction_node.min_flow(value);
        }

        if let Some(cost) = &self.abs_cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            abstraction_node.cost(value);
        }

        network.node(bypass_node);
        network.node(abstraction_node);

        Ok(())
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        match attr {
            AbstractionNodeAttribute::Inflow => {
                let nodes = self.nodes_for_flow_constraints(Some(NodeComponent::Inflow))?;
                Ok(UnresolvedMetricF64::MultiNodeInFlow {
                    nodes,
                    name: self.meta.name.to_string(),
                })
            }
            AbstractionNodeAttribute::Outflow => {
                let nodes = self.nodes_for_flow_constraints(Some(NodeComponent::Outflow))?;
                Ok(UnresolvedMetricF64::MultiNodeInFlow {
                    nodes,
                    name: self.meta.name.to_string(),
                })
            }
            AbstractionNodeAttribute::Abstraction => {
                let node = self.abstraction_sub_name();
                Ok(UnresolvedMetricF64::NodeOutFlow(node))
            }
        }
    }
}
