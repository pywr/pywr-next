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
use crate::v1::{ConversionData, TryFromV1, try_convert_node_attr};
use crate::{ConversionError, TryIntoV2, node_attribute_subset_enum, node_component_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{aggregated_node::Relationship, metric::MetricF64, node::NodeIndex};
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

#[doc = svgbobdoc::transform!(
/// A node used to represent a proportional split above a minimum residual flow (MRF) at a gauging station.
///
/// The maximum flow along each split is controlled by a factor. Internally an aggregated node
/// is created to enforce proportional flows along the splits and bypass.
///
/// **Note**: The behaviour of the factors is different to this in the equivalent Pywr v1.x node.
/// Here the split factors are defined as a proportion of the flow not going via the mrf route.
/// Whereas in Pywr v1.x the factors are defined as ratios.
///
/// ```svgbob
///           <node>.mrf
///          .------>L -----.
///      U  | <node>.bypass  |     D[<default>]
///     -*--|------->L ------|--->*- - -
///         | <node>.split-0 |
///          '------>L -----'
///                  |             D[slot_name_0]
///                   '---------->*- - -
///
///         |                |
///         | <node>.split-i |
///          '------>L -----'
///                  |             D[slot_name_i]
///                   '---------->*- - -
/// ```
///
/// # Available attributes and components
///
/// The enums [`RiverSplitWithGaugeNodeAttribute`] and [`RiverSplitWithGaugeNodeComponent`] define the available
/// attributes and components for this node.
///
)]
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
    const DEFAULT_OUTPUT_SLOT: RiverSplitWithGaugeOutputNodeSlot = RiverSplitWithGaugeOutputNodeSlot::River;

    fn mrf_sub_name() -> Option<&'static str> {
        Some("mrf")
    }

    fn bypass_sub_name() -> Option<&'static str> {
        Some("bypass")
    }

    fn split_sub_name(i: usize) -> Option<String> {
        Some(format!("split-{i}"))
    }

    /// These connectors are used for both incoming and Output edges on the default slot.
    fn default_connectors(&self) -> Vec<(&str, Option<String>)> {
        let mut connectors = vec![
            (self.meta.name.as_str(), Self::mrf_sub_name().map(|s| s.to_string())),
            (self.meta.name.as_str(), Self::bypass_sub_name().map(|s| s.to_string())),
        ];

        connectors.extend(
            self.splits
                .iter()
                .enumerate()
                .map(|(i, _)| (self.meta.name.as_str(), Self::split_sub_name(i))),
        );

        connectors
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(self.default_connectors())
        }
    }

    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        let slot = match slot {
            Some(s) => s.clone().try_into()?,
            None => Self::DEFAULT_OUTPUT_SLOT,
        };

        let indices = match &slot {
            RiverSplitWithGaugeOutputNodeSlot::River => self.default_connectors(),
            RiverSplitWithGaugeOutputNodeSlot::Split { position } => {
                if *position < self.splits.len() {
                    vec![(self.meta.name.as_str(), Self::split_sub_name(*position))]
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
                    Some(i) => vec![(self.meta.name.as_str(), Self::split_sub_name(i))],
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
    fn split_agg_sub_name(i: usize) -> Option<String> {
        Some(format!("split-agg-{i}"))
    }

    pub fn node_indices_for_flow_constraints(
        &self,
        network: &pywr_core::network::Network,
        component: Option<NodeComponent>,
    ) -> Result<Vec<NodeIndex>, SchemaError> {
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
                let mut indices = vec![
                    network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::mrf_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::mrf_sub_name().map(String::from),
                        })?,
                    network
                        .get_node_index_by_name(self.meta.name.as_str(), Self::bypass_sub_name())
                        .ok_or_else(|| SchemaError::CoreNodeNotFound {
                            name: self.meta.name.clone(),
                            sub_name: Self::bypass_sub_name().map(String::from),
                        })?,
                ];

                let split_idx: Vec<NodeIndex> = self
                    .splits
                    .iter()
                    .enumerate()
                    .map(|(i, _)| {
                        network
                            .get_node_index_by_name(self.meta.name.as_str(), Self::split_sub_name(i).as_deref())
                            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                                name: self.meta.name.clone(),
                                sub_name: Self::split_sub_name(i),
                            })
                    })
                    .collect::<Result<_, _>>()?;

                indices.extend(split_idx);
                Ok(indices)
            }
        }
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        // TODO do this properly
        network.add_link_node(self.meta.name.as_str(), Self::mrf_sub_name())?;
        let bypass_idx = network.add_link_node(self.meta.name.as_str(), Self::bypass_sub_name())?;

        for (i, _) in self.splits.iter().enumerate() {
            // Each split has a link node and an aggregated node to enforce the factors
            let split_idx = network.add_link_node(self.meta.name.as_str(), Self::split_sub_name(i).as_deref())?;

            // The factors will be set during the `set_constraints` method
            network.add_aggregated_node(
                self.meta.name.as_str(),
                Self::split_agg_sub_name(i).as_deref(),
                &[vec![bypass_idx], vec![split_idx]],
                None,
            )?;
        }

        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        // MRF applies as a maximum on the MRF node.
        if let Some(cost) = &self.mrf_cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            network.set_node_cost(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        if let Some(mrf) = &self.mrf {
            let value = mrf.load(network, args, Some(&self.meta.name))?;
            network.set_node_max_flow(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        for (i, split) in self.splits.iter().enumerate() {
            // Set the factors for each split
            let r = Relationship::new_proportion_factors(&[split.factor.load(network, args, Some(&self.meta.name))?]);
            network.set_aggregated_node_relationship(
                self.meta.name.as_str(),
                Self::split_agg_sub_name(i).as_deref(),
                Some(r),
            )?;
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

        // This gets the indices of all the link nodes
        // There's currently no way to isolate the flows to the individual splits
        // Therefore, the only metrics are gross inflow and outflow
        let mut indices = vec![
            network
                .get_node_index_by_name(self.meta.name.as_str(), Self::mrf_sub_name())
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta.name.clone(),
                    sub_name: Self::mrf_sub_name().map(String::from),
                })?,
            network
                .get_node_index_by_name(self.meta.name.as_str(), Self::bypass_sub_name())
                .ok_or_else(|| SchemaError::CoreNodeNotFound {
                    name: self.meta.name.clone(),
                    sub_name: Self::bypass_sub_name().map(String::from),
                })?,
        ];

        let split_idx: Vec<NodeIndex> = self
            .splits
            .iter()
            .enumerate()
            .map(|(i, _)| {
                network
                    .get_node_index_by_name(self.meta.name.as_str(), Self::split_sub_name(i).as_deref())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: self.meta.name.clone(),
                        sub_name: Self::split_sub_name(i),
                    })
            })
            .collect::<Result<_, _>>()?;

        indices.extend(split_idx);

        let metric = match attr {
            RiverSplitWithGaugeNodeAttribute::Inflow => MetricF64::MultiNodeInFlow {
                indices,
                name: self.meta.name.to_string(),
            },
            RiverSplitWithGaugeNodeAttribute::Outflow => MetricF64::MultiNodeOutFlow {
                indices,
                name: self.meta.name.to_string(),
            },
        };

        Ok(metric)
    }
}

impl TryFromV1<RiverSplitWithGaugeNodeV1> for RiverSplitWithGaugeNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: RiverSplitWithGaugeNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

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
