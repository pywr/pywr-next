use crate::error::ComponentConversionError;
use crate::error::SchemaError;
use crate::metric::{Metric, NodeComponentReference};
#[cfg(feature = "core")]
use crate::network::LoadArgs;
#[cfg(feature = "core")]
use crate::nodes::NodeAttribute;
use crate::nodes::NodeMeta;
use crate::nodes::core::StorageInitialVolume;
use crate::parameters::Parameter;
use crate::v1::{ConversionData, TryFromV1, try_convert_initial_storage, try_convert_node_attr};
use crate::{ConversionError, node_attribute_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{
    derived_metric::DerivedMetric, metric::MetricF64, timestep::TimeDomain, virtual_storage::VirtualStorageBuilder,
};
use pywr_schema_macros::PywrVisitAll;
use pywr_schema_macros::skip_serializing_none;
use pywr_v1_schema::nodes::{
    AnnualVirtualStorageNode as AnnualVirtualStorageNodeV1, MonthlyVirtualStorageNode as MonthlyVirtualStorageNodeV1,
    RollingVirtualStorageNode as RollingVirtualStorageNodeV1,
    SeasonalVirtualStorageNode as SeasonalVirtualStorageNodeV1, VirtualStorageNode as VirtualStorageNodeV1,
};
use schemars::JsonSchema;
use std::num::NonZeroUsize;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

// This macro generates a subset enum for the `VirtualStorageNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum VirtualStorageNodeAttribute {
        Volume,
        ProportionalVolume,
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct AnnualReset {
    pub day: u8,
    pub month: u8,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct SeasonalReset {
    pub start_day: u8,
    pub start_month: u8,
    pub end_day: u8,
    pub end_month: u8,
}

/// The reset behaviour for a virtual storage node.
///
/// If provided this determines when the virtual storage node's volume is reset.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(VirtualStorageResetType))]
pub enum VirtualStorageReset {
    Never,
    /// Reset annually on a specific day and month.
    Annual(AnnualReset),
    /// Reset every N months.
    Monthly {
        months: u8,
    },
    Seasonal(SeasonalReset),
}

#[cfg(feature = "core")]
impl TryInto<pywr_core::virtual_storage::VirtualStorageReset> for VirtualStorageReset {
    type Error = SchemaError;
    fn try_into(self) -> Result<pywr_core::virtual_storage::VirtualStorageReset, Self::Error> {
        let r = match self {
            VirtualStorageReset::Never => pywr_core::virtual_storage::VirtualStorageReset::Never,
            VirtualStorageReset::Annual(annual) => {
                let reset_month = annual.month.try_into()?;
                pywr_core::virtual_storage::VirtualStorageReset::DayOfYear {
                    day: annual.day as u32,
                    month: reset_month,
                }
            }
            VirtualStorageReset::Monthly { months } => {
                pywr_core::virtual_storage::VirtualStorageReset::NumberOfMonths { months: months.into() }
            }
            VirtualStorageReset::Seasonal(seasonal) => {
                let reset_month = seasonal.start_month.try_into()?;
                pywr_core::virtual_storage::VirtualStorageReset::DayOfYear {
                    day: seasonal.start_day as u32,
                    month: reset_month,
                }
            }
        };

        Ok(r)
    }
}

/// The length of the rolling window.
///
/// This can be specified in either days or time-steps.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(RollingWindowType))]
pub enum RollingWindow {
    Days { days: NonZeroUsize },
    Timesteps { timesteps: NonZeroUsize },
}

impl Default for RollingWindow {
    fn default() -> Self {
        Self::Timesteps {
            timesteps: NonZeroUsize::new(30).expect("30 is not zero"),
        }
    }
}

#[cfg(feature = "core")]
impl RollingWindow {
    /// Convert the rolling window to a number of time-steps.
    ///
    /// If the conversion fails (e.g. the number of days is less than the time-step duration) then `None` is returned.
    pub fn as_timesteps(&self, time: &TimeDomain) -> Option<NonZeroUsize> {
        match self {
            Self::Days { days } => {
                let ts_days = match time.step_duration().whole_days() {
                    Some(d) => d as usize,
                    // If the timestep duration is not a whole number of days then the rolling window cannot be specified in days.
                    None => return None,
                };

                let timesteps = days.get() / ts_days;

                NonZeroUsize::new(timesteps)
            }
            Self::Timesteps { timesteps } => Some(*timesteps),
        }
    }
}

/// The volume to reset to when a reset occurs.
#[derive(
    serde::Deserialize, serde::Serialize, Copy, Clone, Debug, JsonSchema, PywrVisitAll, Display, EnumDiscriminants,
)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(VirtualStorageResetVolumeType))]
pub enum VirtualStorageResetVolume {
    Initial,
    Max,
}

#[cfg(feature = "core")]
impl From<VirtualStorageResetVolume> for pywr_core::virtual_storage::VirtualStorageResetVolume {
    fn from(val: VirtualStorageResetVolume) -> pywr_core::virtual_storage::VirtualStorageResetVolume {
        match val {
            VirtualStorageResetVolume::Initial => pywr_core::virtual_storage::VirtualStorageResetVolume::Initial,
            VirtualStorageResetVolume::Max => pywr_core::virtual_storage::VirtualStorageResetVolume::Max,
        }
    }
}

/// A virtual storage node that can be used to represent non-physical storage constraints.
///
/// This is typically used to represent storage limits that are associated with licences or
/// other artificial constraints. The storage is drawdown by the nodes specified in the
/// `nodes` field. The `component` of the node reference is used to determine the flow that is
/// used by storage. The rate of drawdown is determined by the `factors` field, which
/// multiplies the flow by the factor to determine the rate of drawdown. If not specified
/// the factor is assumed to be 1.0 for each node.
///
/// The `max_volume` and `min_volume` fields are used to determine the maximum and minimum
/// volume of the storage. If `max_volume` is not specified then the storage is
/// unlimited. If `min_volume` is not specified then it is assumed to be zero.
///
/// The `reset` field can be used to specify when the storage is reset to a specific volume.
/// By default, the storage is never reset. The choices in [`VirtualStorageReset`] are:
/// - `Never`: The storage is never reset.
/// - `Annual`: The storage is reset annually on a specific day and month.
/// - `Monthly`: The storage is reset every N months.
/// - `Seasonal`: The storage is reset seasonally between a start and end date.
///
/// If the `reset` field is specified, the `reset_volume` field determines the volume to reset to.
/// The choices in [`VirtualStorageResetVolume`] are:
/// - `Initial`: The storage is reset to the initial volume specified in the `initial_volume` field.
///   This is the default if `reset_volume` is not specified.
/// - `Max`: The storage is reset to the maximum volume specified in the `max_volume` field.
///
// TODO write the cost documentation when linking a node to this cost is supported in the schema.
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct VirtualStorageNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub nodes: Vec<NodeComponentReference>,
    pub factors: Option<Vec<f64>>,
    pub max_volume: Option<Metric>,
    pub min_volume: Option<Metric>,
    pub cost: Option<Metric>,
    pub initial_volume: StorageInitialVolume,
    pub reset: Option<VirtualStorageReset>,
    pub reset_volume: Option<VirtualStorageResetVolume>,
    pub window: Option<RollingWindow>,
}

impl VirtualStorageNode {
    const DEFAULT_ATTRIBUTE: VirtualStorageNodeAttribute = VirtualStorageNodeAttribute::Volume;

    pub fn input_connectors(&self) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        Ok(vec![])
    }

    pub fn output_connectors(&self) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        Ok(vec![])
    }

    pub fn default_attribute(&self) -> VirtualStorageNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl VirtualStorageNode {
    /// This returns the node indices for flow constraints based on the nodes referenced in this virtual storage node.
    ///
    /// Note that this is a private function, as it is not supported using this node itself
    /// inside a flow constraint.
    fn node_indices_for_flow_constraints(
        &self,
        network: &pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let indices = self
            .nodes
            .iter()
            .map(|node_ref| {
                args.schema
                    .get_node_by_name(&node_ref.name)
                    .ok_or_else(|| SchemaError::NodeNotFound {
                        name: node_ref.name.to_string(),
                    })?
                    .node_indices_for_flow_constraints(network, node_ref.component)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(indices)
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        let node_idxs = self.node_indices_for_flow_constraints(network, args)?;

        let mut builder =
            VirtualStorageBuilder::new(self.meta.name.as_str(), &node_idxs).initial_volume(self.initial_volume.into());

        if let Some(r) = self.reset.clone() {
            let reset = r.try_into()?;
            builder = builder.reset(reset);
        }

        if let Some(rv) = &self.reset_volume {
            builder = builder.reset_volume((*rv).into());
        }

        // Set the active period if this is a seasonal reset
        if let Some(VirtualStorageReset::Seasonal(seasonal)) = &self.reset {
            let start_month = seasonal.start_month.try_into()?;
            let end_month = seasonal.end_month.try_into()?;
            let period = pywr_core::virtual_storage::VirtualStorageActivePeriod::Period {
                start_day: seasonal.start_day as u32,
                start_month,
                end_day: seasonal.end_day as u32,
                end_month,
            };
            builder = builder.active_period(period);
        }

        if let Some(window) = &self.window {
            let rolling_window =
                window
                    .as_timesteps(args.domain.time())
                    .ok_or_else(|| SchemaError::InvalidRollingWindow {
                        name: self.meta.name.clone(),
                    })?;
            builder = builder.rolling_window(rolling_window);
        }

        if let Some(factors) = &self.factors {
            builder = builder.factors(factors);
        }

        network.add_virtual_storage_node(builder)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            network.set_virtual_storage_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_volume) = &self.min_volume {
            let value = min_volume.load(network, args, Some(&self.meta.name))?;
            network.set_virtual_storage_min_volume(self.meta.name.as_str(), None, Some(value.try_into()?))?;
        }

        if let Some(max_volume) = &self.max_volume {
            let value = max_volume.load(network, args, Some(&self.meta.name))?;
            network.set_virtual_storage_max_volume(self.meta.name.as_str(), None, Some(value.try_into()?))?;
        }

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
            .get_virtual_storage_node_index_by_name(self.meta.name.as_str(), None)
            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                name: self.meta.name.clone(),
                sub_name: None,
            })?;

        let metric = match attr {
            VirtualStorageNodeAttribute::Volume => MetricF64::VirtualStorageVolume(idx),
            VirtualStorageNodeAttribute::ProportionalVolume => {
                let dm = DerivedMetric::VirtualStorageProportionalVolume(idx);
                let derived_metric_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(derived_metric_idx)
            }
        };

        Ok(metric)
    }
}

impl TryFromV1<VirtualStorageNodeV1> for VirtualStorageNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: VirtualStorageNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        let cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;
        let max_volume = try_convert_node_attr(&meta.name, "max_volume", v1.max_volume, parent_node, conversion_data)?;
        let min_volume = try_convert_node_attr(&meta.name, "min_volume", v1.min_volume, parent_node, conversion_data)?;

        let initial_volume =
            try_convert_initial_storage(&meta.name, "initial_volume", v1.initial_volume, v1.initial_volume_pc)?;

        let nodes = v1.nodes.into_iter().map(|v| v.into()).collect();

        let n = Self {
            meta,
            parameters: None,
            nodes,
            factors: v1.factors,
            max_volume,
            min_volume,
            cost,
            initial_volume,
            reset: None,
            reset_volume: None,
            window: None,
        };
        Ok(n)
    }
}

impl TryFromV1<AnnualVirtualStorageNodeV1> for VirtualStorageNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: AnnualVirtualStorageNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        let cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;
        let max_volume = try_convert_node_attr(&meta.name, "max_volume", v1.max_volume, parent_node, conversion_data)?;
        let min_volume = try_convert_node_attr(&meta.name, "min_volume", v1.min_volume, parent_node, conversion_data)?;

        let initial_volume =
            try_convert_initial_storage(&meta.name, "initial_volume", v1.initial_volume, v1.initial_volume_pc)?;

        let nodes = v1.nodes.into_iter().map(|n| n.into()).collect();

        let reset_volume = if v1.reset_to_initial_volume {
            Some(VirtualStorageResetVolume::Initial)
        } else {
            Some(VirtualStorageResetVolume::Max)
        };

        let n = Self {
            meta,
            parameters: None,
            nodes,
            factors: v1.factors,
            max_volume,
            min_volume,
            cost,
            initial_volume,
            reset: Some(VirtualStorageReset::Annual(AnnualReset {
                day: v1.reset_day as u8,
                month: v1.reset_month as u8,
            })),
            reset_volume,
            window: None,
        };
        Ok(n)
    }
}

impl TryFromV1<MonthlyVirtualStorageNodeV1> for VirtualStorageNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: MonthlyVirtualStorageNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        let cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;
        let max_volume = try_convert_node_attr(&meta.name, "max_volume", v1.max_volume, parent_node, conversion_data)?;
        let min_volume = try_convert_node_attr(&meta.name, "min_volume", v1.min_volume, parent_node, conversion_data)?;

        let initial_volume =
            try_convert_initial_storage(&meta.name, "initial_volume", v1.initial_volume, v1.initial_volume_pc)?;

        let nodes = v1.nodes.into_iter().map(|n| n.into()).collect();

        let reset_volume = if v1.reset_to_initial_volume {
            Some(VirtualStorageResetVolume::Initial)
        } else {
            Some(VirtualStorageResetVolume::Max)
        };

        let n = Self {
            meta,
            parameters: None,
            nodes,
            factors: v1.factors,
            max_volume,
            min_volume,
            cost,
            initial_volume,
            reset: Some(VirtualStorageReset::Monthly { months: v1.months }),
            reset_volume,
            window: None,
        };
        Ok(n)
    }
}

impl TryFromV1<RollingVirtualStorageNodeV1> for VirtualStorageNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: RollingVirtualStorageNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        let cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;
        let max_volume = try_convert_node_attr(&meta.name, "max_volume", v1.max_volume, parent_node, conversion_data)?;
        let min_volume = try_convert_node_attr(&meta.name, "min_volume", v1.min_volume, parent_node, conversion_data)?;

        let initial_volume =
            try_convert_initial_storage(&meta.name, "initial_volume", v1.initial_volume, v1.initial_volume_pc)?;

        let window = if let Some(days) = v1.days {
            if let Some(days) = NonZeroUsize::new(days as usize) {
                RollingWindow::Days { days }
            } else {
                return Err(ComponentConversionError::Node {
                    name: meta.name.clone(),
                    attr: "window".to_string(),
                    error: ConversionError::UnsupportedFeature {
                        feature: "Rolling window with zero `days` is not supported".to_string(),
                    },
                });
            }
        } else if let Some(timesteps) = v1.timesteps {
            if let Some(timesteps) = NonZeroUsize::new(timesteps as usize) {
                RollingWindow::Timesteps { timesteps }
            } else {
                return Err(ComponentConversionError::Node {
                    name: meta.name.clone(),
                    attr: "window".to_string(),
                    error: ConversionError::UnsupportedFeature {
                        feature: "Rolling window with zero `timesteps` is not supported".to_string(),
                    },
                });
            }
        } else {
            return Err(ComponentConversionError::Node {
                name: meta.name.clone(),
                attr: "window".to_string(),
                error: ConversionError::MissingAttribute {
                    attrs: vec!["days".to_string(), "timesteps".to_string()],
                },
            });
        };

        let nodes = v1.nodes.into_iter().map(|n| n.into()).collect();

        let n = Self {
            meta,
            parameters: None,
            nodes,
            factors: v1.factors,
            max_volume,
            min_volume,
            cost,
            initial_volume,
            reset: None,
            reset_volume: None,
            window: Some(window),
        };
        Ok(n)
    }
}

impl TryFromV1<SeasonalVirtualStorageNodeV1> for VirtualStorageNode {
    type Error = ComponentConversionError;

    fn try_from_v1(
        v1: SeasonalVirtualStorageNodeV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();

        let cost = try_convert_node_attr(&meta.name, "cost", v1.cost, parent_node, conversion_data)?;
        let max_volume = try_convert_node_attr(&meta.name, "max_volume", v1.max_volume, parent_node, conversion_data)?;
        let min_volume = try_convert_node_attr(&meta.name, "min_volume", v1.min_volume, parent_node, conversion_data)?;

        let initial_volume =
            try_convert_initial_storage(&meta.name, "initial_volume", v1.initial_volume, v1.initial_volume_pc)?;

        let nodes = v1.nodes.into_iter().map(|n| n.into()).collect();

        let reset_volume = if v1.reset_to_initial_volume {
            Some(VirtualStorageResetVolume::Initial)
        } else {
            Some(VirtualStorageResetVolume::Max)
        };

        let reset = VirtualStorageReset::Seasonal(SeasonalReset {
            start_day: v1.reset_day as u8,
            start_month: v1.reset_month as u8,
            end_day: v1.end_day as u8,
            end_month: v1.end_month as u8,
        });

        let n = Self {
            meta,
            parameters: None,
            nodes,
            factors: v1.factors,
            max_volume,
            min_volume,
            cost,
            initial_volume,
            reset: Some(reset),
            reset_volume,
            window: None,
        };
        Ok(n)
    }
}
