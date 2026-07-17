use crate::error::ComponentConversionError;
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::nodes::abstraction::AbstractionOutputNodeSlot;
#[cfg(feature = "core")]
use crate::nodes::{NodeAttribute, NodeComponent};
use crate::nodes::{NodeMeta, NodeSlot};
use crate::parameters::Parameter;
use crate::v1::{ConversionData, TryFromV1, try_convert_node_attr, try_convert_node_meta};
use crate::{ConversionError, TryIntoV2, mermaid, node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{aggregated_node::ProportionalFactorsBuilder, metric::UnresolvedMetricF64, node::UnresolvedNode};
use pywr_schema_macros::PywrVisitAll;
use pywr_schema_macros::skip_serializing_none;
use pywr_v1_schema::nodes::RiverSplitWithGaugeNode as RiverSplitWithGaugeNodeV1;
use pywr_v1_schema::parameters::ParameterValues;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub struct RiverSplit {
    /// Proportion of flow not going via the mrf route.
    pub factor: Metric,
    /// Name of the slot when connecting to this split. If not provided then the slot
    /// can be accessed by its index.
    pub slot_name: Option<String>,
}
// This macro generates a subset enum for the `RiverSplitWithGaugeNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum RiverSplitWithGaugeNodeAttribute {
        Inflow,
        Outflow,
    }
}

node_component_subset_enum! {
    pub enum RiverSplitWithGaugeNodeComponent {
        Inflow,
        Outflow,
    }
}

pub enum RiverSplitWithGaugeOutputNodeSlot {
    River,
    Split { position: usize },
    User { name: String },
}

impl From<RiverSplitWithGaugeOutputNodeSlot> for NodeSlot {
    fn from(slot: RiverSplitWithGaugeOutputNodeSlot) -> Self {
        match slot {
            RiverSplitWithGaugeOutputNodeSlot::River => NodeSlot::River,
            RiverSplitWithGaugeOutputNodeSlot::Split { position } => NodeSlot::Split { position },
            RiverSplitWithGaugeOutputNodeSlot::User { name } => NodeSlot::User { name },
        }
    }
}

impl TryFrom<NodeSlot> for RiverSplitWithGaugeOutputNodeSlot {
    type Error = SchemaError;

    fn try_from(slot: NodeSlot) -> Result<Self, Self::Error> {
        match slot {
            NodeSlot::River => Ok(RiverSplitWithGaugeOutputNodeSlot::River),
            NodeSlot::Split { position } => Ok(RiverSplitWithGaugeOutputNodeSlot::Split { position }),
            NodeSlot::User { name } => Ok(RiverSplitWithGaugeOutputNodeSlot::User { name }),
            _ => Err(SchemaError::OutputNodeSlotNotSupported { slot }),
        }
    }
}

/// A node used to represent a proportional split above a minimum residual flow (MRF) at a gauging station.
///
/// The maximum flow along each split is controlled by a factor. Internally an aggregated node
/// is created to enforce proportional flows along the splits and bypass.
///
/// **Note**: The behaviour of the factors is different to this in the equivalent Pywr v1.x node.
/// Here the split factors are defined as a proportion of the flow not going via the mrf route.
/// Whereas in Pywr v1.x the factors are defined as ratios.
///
#[doc = mermaid!("doc_diagrams/river-split-with-gauge.mmd")]
///
/// # Available attributes and components
///
/// The enums [`RiverSplitWithGaugeNodeAttribute`] and [`RiverSplitWithGaugeNodeComponent`] define the available
/// attributes and components for this node.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct RiverSplitWithGaugeNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub mrf: Option<Metric>,
    pub mrf_cost: Option<Metric>,
    pub splits: Vec<RiverSplit>,
}

impl RiverSplitWithGaugeNode {
    const DEFAULT_ATTRIBUTE: RiverSplitWithGaugeNodeAttribute = RiverSplitWithGaugeNodeAttribute::Outflow;
    const DEFAULT_COMPONENT: RiverSplitWithGaugeNodeComponent = RiverSplitWithGaugeNodeComponent::Outflow;

    pub fn iter_output_slots(&self) -> impl Iterator<Item = NodeSlot> + '_ {
        [AbstractionOutputNodeSlot::River.into()]
            .into_iter()
            .chain(self.splits.iter().enumerate().map(|(i, split)| match &split.slot_name {
                Some(name) => NodeSlot::User { name: name.clone() },
                None => NodeSlot::Split { position: i },
            }))
    }

    pub fn default_attribute(&self) -> RiverSplitWithGaugeNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }

    pub fn default_component(&self) -> RiverSplitWithGaugeNodeComponent {
        Self::DEFAULT_COMPONENT
    }
}

#[cfg(feature = "core")]
impl RiverSplitWithGaugeNode {
    const DEFAULT_OUTPUT_SLOT: RiverSplitWithGaugeOutputNodeSlot = RiverSplitWithGaugeOutputNodeSlot::River;
    fn mrf_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("mrf"))
    }

    fn bypass_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("bypass"))
    }

    fn split_sub_name(&self, i: usize) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some(&format!("split-{i}")))
    }

    fn split_agg_sub_name(&self, i: usize) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some(&format!("split-agg-{i}")))
    }

    /// These connectors are used for both incoming and Output edges on the default slot.
    fn default_connectors(&self) -> Vec<UnresolvedNode> {
        let mut connectors = vec![self.mrf_sub_name(), self.bypass_sub_name()];

        connectors.extend(self.splits.iter().enumerate().map(|(i, _)| self.split_sub_name(i)));

        connectors
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(self.default_connectors())
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        let slot = match slot {
            Some(s) => s.clone().try_into()?,
            None => Self::DEFAULT_OUTPUT_SLOT,
        };

        let indices = match &slot {
            RiverSplitWithGaugeOutputNodeSlot::River => self.default_connectors(),
            RiverSplitWithGaugeOutputNodeSlot::Split { position } => {
                if *position < self.splits.len() {
                    vec![self.split_sub_name(*position)]
                } else {
                    return Err(SchemaError::NodeConnectionSlotNotFound {
                        node: self.meta.name.clone(),
                        slot: slot.into(),
                    });
                }
            }
            RiverSplitWithGaugeOutputNodeSlot::User { name } => {
                match self
                    .splits
                    .iter()
                    .position(|split| split.slot_name.as_ref().is_some_and(|s| s == name))
                {
                    Some(i) => vec![self.split_sub_name(i)],
                    None => {
                        return Err(SchemaError::NodeConnectionSlotNotFound {
                            node: self.meta.name.clone(),
                            slot: slot.into(),
                        });
                    }
                }
            }
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

        match component {
            // This gets the indices of all the link nodes
            // There's currently no way to isolate the flows to the individual splits
            // Therefore, the only components are gross inflow and outflow
            RiverSplitWithGaugeNodeComponent::Inflow | RiverSplitWithGaugeNodeComponent::Outflow => {
                Ok(self.default_connectors())
            }
        }
    }
    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        let mut mrf_node = pywr_core::node::NodeBuilder::link(self.mrf_sub_name());
        let bypass_node = pywr_core::node::NodeBuilder::link(self.bypass_sub_name());

        // MRF applies as a maximum on the MRF node.
        if let Some(cost) = &self.mrf_cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            mrf_node.cost(value);
        }

        if let Some(mrf) = &self.mrf {
            let value = mrf.load(network, args, Some(&self.meta.name))?;
            mrf_node.max_flow(value);
        }

        for (i, split) in self.splits.iter().enumerate() {
            // Each split has a link node and an aggregated node to enforce the factors
            let split_node = pywr_core::node::NodeBuilder::link(self.split_sub_name(i));

            // Set the factors for each split
            let mut r = ProportionalFactorsBuilder::default();
            r.factor(split.factor.load(network, args, Some(&self.meta.name))?);

            // The factors will be set during the `set_constraints` method
            let mut agg_node = pywr_core::AggregatedNodeBuilder::new(self.split_agg_sub_name(i));
            agg_node
                .nodes(vec![self.bypass_sub_name()])
                .nodes(vec![self.split_sub_name(i)])
                .relationship(Box::new(r));

            network.agg_node(agg_node);
            network.node(split_node);
        }

        network.node(mrf_node);
        network.node(bypass_node);

        Ok(())
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        // This gets the indices of all the link nodes
        // There's currently no way to isolate the flows to the individual splits
        // Therefore, the only metrics are gross inflow and outflow
        let nodes = self.default_connectors();

        let metric = match attr {
            RiverSplitWithGaugeNodeAttribute::Inflow => UnresolvedMetricF64::MultiNodeInFlow {
                nodes,
                name: self.meta.name.to_string(),
            },
            RiverSplitWithGaugeNodeAttribute::Outflow => UnresolvedMetricF64::MultiNodeOutFlow {
                nodes,
                name: self.meta.name.to_string(),
            },
        };

        Ok(metric)
    }
}

impl TryFromV1<RiverSplitWithGaugeNodeV1> for RiverSplitWithGaugeNode {
    type Error = Box<ComponentConversionError>;

    fn try_from_v1(
        v1: RiverSplitWithGaugeNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = try_convert_node_meta(v1.meta)?;

        let mrf = try_convert_node_attr(&meta.name, "mrf", v1.mrf, parent_node, conversion_data)?;
        let mrf_cost = try_convert_node_attr(&meta.name, "mrf_cost", v1.mrf_cost, parent_node, conversion_data)?;

        let factors = convert_factors(v1.factors, parent_node, conversion_data).map_err(|error| {
            ComponentConversionError::Node {
                attr: "factors".to_string(),
                name: meta.name.to_string(),
                error,
            }
        })?;
        let splits = factors
            .into_iter()
            .zip(v1.slot_names.into_iter().skip(1))
            .map(|(factor, slot_name)| {
                Ok(RiverSplit {
                    factor,
                    slot_name: Some(slot_name),
                })
            })
            .collect::<Result<Vec<_>, Self::Error>>()?;

        let n = Self {
            meta,
            parameters: None,
            mrf,
            mrf_cost,
            splits,
        };
        Ok(n)
    }
}

/// Try to convert ratio factors to proprtional factors.
fn convert_factors(
    factors: ParameterValues,
    parent_node: Option<&str>,
    conversion_data: &mut ConversionData,
) -> Result<Vec<Metric>, ConversionError> {
    let mut iter = factors.into_iter();
    if let Some(first_factor) = iter.next() {
        if let Metric::Literal { value } = first_factor.try_into_v2(parent_node, conversion_data)? {
            // First Metric is a constant; we can proceed with the conversion

            let split_factors = iter
                .map(|f| {
                    if let Metric::Literal { value } = f.try_into_v2(parent_node, conversion_data)? {
                        Ok(value)
                    } else {
                        Err(ConversionError::NonConstantValue {})
                    }
                })
                .collect::<Result<Vec<_>, _>>()?;

            // Convert the factors to proportional factors
            let sum: f64 = split_factors.iter().sum::<f64>() + value;
            Ok(split_factors
                .into_iter()
                .map(|f| Metric::Literal { value: f / sum })
                .collect())
        } else {
            // Non-constant metric can not be easily converted to proportional factors
            Err(ConversionError::NonConstantValue {})
        }
    } else {
        // No factors
        Ok(vec![])
    }
}
