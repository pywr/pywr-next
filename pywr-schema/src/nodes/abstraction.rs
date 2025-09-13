use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::nodes::NodeMeta;
#[cfg(feature = "core")]
use crate::nodes::{NodeAttribute, NodeComponent};
use crate::parameters::Parameter;
use crate::{node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::metric::MetricF64;
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

#[doc = svgbobdoc::transform!(
/// This node represents a river abstraction.
/// 
/// The abstraction can optionally be constrained by a minimum residual flow (MRF) requirement. If
/// this is defined an internal MRF node is created.
///
/// The node defines two output slots. The 'downstream' slot represents a continuation of the
/// river and the 'abstraction' slot is where the abstracted flow is directed.
///
///
/// ```svgbob
///            <node>.mrf
///          .------>L -----.
///      U  |                |     D[downstream]
///     -*--|                |--->*- - ->
///         |                |
///         |'------>L -----'
///         |   <node>.bypass
///         |
///         |
///         |                     D[abstraction]
///         +------>L ---------->*- - ->
///            <node>.abstraction
///
/// ```
///
/// # Available attributes and components
///
/// The enums [`AbstractionNodeAttribute`] and [`AbstractionNodeComponent`] define the available
/// attributes and components for this node.
///
)]
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

    fn mrf_sub_name() -> Option<&'static str> {
        Some("mrf")
    }

    fn bypass_sub_name() -> Option<&'static str> {
        Some("bypass")
    }

    fn abstraction_sub_name() -> Option<&'static str> {
        Some("abstraction")
    }

    pub fn input_connectors(&self) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        let mut connectors = vec![
            (self.meta.name.as_str(), Self::bypass_sub_name().map(|s| s.to_string())),
            (
                self.meta.name.as_str(),
                Self::abstraction_sub_name().map(|s| s.to_string()),
            ),
        ];
        if self.mrf.is_some() {
            connectors.push((self.meta.name.as_str(), Self::mrf_sub_name().map(|s| s.to_string())));
        }
        Ok(connectors)
    }

    pub fn output_connectors(&self, slot: Option<&str>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        match slot {
            Some("downstream") => {
                if self.mrf.is_some() {
                    Ok(vec![
                        (self.meta.name.as_str(), Self::mrf_sub_name().map(|s| s.to_string())),
                        (self.meta.name.as_str(), Self::bypass_sub_name().map(|s| s.to_string())),
                    ])
                } else {
                    Ok(vec![(
                        self.meta.name.as_str(),
                        Self::bypass_sub_name().map(|s| s.to_string()),
                    )])
                }
            }
            Some("abstraction") => Ok(vec![(
                self.meta.name.as_str(),
                Self::abstraction_sub_name().map(|s| s.to_string()),
            )]),
            Some(s) => Err(SchemaError::NodeConnectionSlotNotFound {
                node: self.meta.name.clone(),
                slot: s.to_string(),
            }),
            None => Err(SchemaError::NodeConnectionSlotRequired {
                msg: format!(
                    "Either the 'downstream' or 'abstraction' slot must be specified when connecting to '{}'",
                    self.meta.name
                ),
            }),
        }
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
    pub fn node_indices_for_flow_constraints(
        &self,
        network: &pywr_core::network::Network,
        component: Option<NodeComponent>,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        // Use the default component if none is specified
        let component = match component {
            Some(c) => c.try_into()?,
            None => Self::DEFAULT_COMPONENT,
        };

        let indices = match component {
            AbstractionNodeComponent::Inflow => {
                let mut indices = Vec::new();

                if self.mrf.is_some() {
                    indices.push(
                        network
                            .get_node_index_by_name(self.meta.name.as_str(), Self::mrf_sub_name())
                            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                name: self.meta.name.clone(),
                                sub_name: Self::mrf_sub_name().map(String::from),
                            })?,
                    );
                }
                indices.push(
                    network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::bypass_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::bypass_sub_name().map(String::from),
                        })?,
                );
                indices.push(
                    network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::abstraction_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::abstraction_sub_name().map(String::from),
                        })?,
                );
                indices
            }
            AbstractionNodeComponent::Outflow => {
                let mut indices = Vec::new();

                if self.mrf.is_some() {
                    indices.push(
                        network
                            .get_node_index_by_name(self.meta.name.as_str(), Self::mrf_sub_name())
                            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                name: self.meta.name.clone(),
                                sub_name: Self::mrf_sub_name().map(String::from),
                            })?,
                    );
                }

                indices.push(
                    network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::bypass_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::bypass_sub_name().map(String::from),
                        })?,
                );
                indices
            }
            AbstractionNodeComponent::Abstraction => {
                vec![
                    network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::abstraction_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::abstraction_sub_name().map(String::from),
                        })?,
                ]
            }
        };
        Ok(indices)
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        if self.mrf.is_some() {
            network.add_link_node(self.meta.name.as_str(), Self::mrf_sub_name())?;
        }

        network.add_link_node(self.meta.name.as_str(), Self::bypass_sub_name())?;
        network.add_link_node(self.meta.name.as_str(), Self::abstraction_sub_name())?;

        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(mrf) = &self.mrf {
            let value = mrf.load(network, args, Some(&self.meta.name))?;
            network.set_node_max_flow(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        if let Some(cost) = &self.mrf_cost {
            if self.mrf.is_some() {
                let value = cost.load(network, args, Some(&self.meta.name))?;
                network.set_node_cost(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
            } else {
                return Err(SchemaError::InvalidNodeAttributes {
                    msg: format!(
                        "MRF cost defined but no MRF constraint provided for node '{}'",
                        self.meta.name
                    ),
                });
            }
        }

        if let Some(abs_max_flow) = &self.abs_max_flow {
            let value = abs_max_flow.load(network, args, Some(&self.meta.name))?;
            network.set_node_max_flow(self.meta.name.as_str(), Self::abstraction_sub_name(), value.into())?;
        }

        if let Some(abs_min_flow) = &self.abs_min_flow {
            let value = abs_min_flow.load(network, args, Some(&self.meta.name))?;
            network.set_node_min_flow(self.meta.name.as_str(), Self::abstraction_sub_name(), value.into())?;
        }

        if let Some(cost) = &self.abs_cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            network.set_node_cost(self.meta.name.as_str(), Self::abstraction_sub_name(), value.into())?;
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

        match attr {
            AbstractionNodeAttribute::Inflow => {
                let indices = self.node_indices_for_flow_constraints(network, Some(NodeComponent::Inflow))?;
                Ok(MetricF64::MultiNodeInFlow {
                    indices,
                    name: self.meta.name.to_string(),
                })
            }
            AbstractionNodeAttribute::Outflow => {
                let indices = self.node_indices_for_flow_constraints(network, Some(NodeComponent::Outflow))?;
                Ok(MetricF64::MultiNodeInFlow {
                    indices,
                    name: self.meta.name.to_string(),
                })
            }
            AbstractionNodeAttribute::Abstraction => {
                let idx = network
                    .get_node_index_by_name(self.meta.name.as_str(), Self::abstraction_sub_name())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: self.meta.name.clone(),
                        sub_name: Self::abstraction_sub_name().map(String::from),
                    })?;
                Ok(MetricF64::NodeOutFlow(idx))
            }
        }
    }
}
