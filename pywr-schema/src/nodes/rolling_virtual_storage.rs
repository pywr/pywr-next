#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::error::{ComponentConversionError, ConversionError};
use crate::metric::{Metric, SimpleNodeReference};
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::{NodeAttribute, NodeMeta, StorageInitialVolume};
use crate::parameters::Parameter;
use crate::v1::{ConversionData, TryFromV1, try_convert_initial_storage, try_convert_node_attr};
#[cfg(feature = "core")]
use pywr_core::{
    derived_metric::DerivedMetric,
    metric::MetricF64,
    timestep::TimeDomain,
    virtual_storage::{VirtualStorageBuilder, VirtualStorageReset},
};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::nodes::RollingVirtualStorageNode as RollingVirtualStorageNodeV1;
use schemars::JsonSchema;
use std::num::NonZeroUsize;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

/// The length of the rolling window.
///
/// This can be specified in either days or time-steps.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(MetricType))]
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

/// A virtual storage node that constrains node(s) utilisation over a fixed window.
///
/// A virtual storage node represents a "virtual" volume that can be used to constrain the utilisation of one or more
/// nodes. This rolling virtual storage node constraints the utilisation of the nodes using a fixed window of the
/// last `N` days or time-steps. Each time-step the available volume in the virtual storage is based on the maximum
/// volume less the sum of the utilisation of the nodes over the window. The window is rolled forward each time-step,
/// with the oldest time-step being removed from the history and the newest utilisation added.
///
/// The rolling virtual storage node is useful for representing rolling licences. For example, a 30-day or 90-day
/// licence on a water abstraction.
///
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct RollingVirtualStorageNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub nodes: Vec<SimpleNodeReference>,
    pub factors: Option<Vec<f64>>,
    pub max_volume: Option<Metric>,
    pub min_volume: Option<Metric>,
    pub cost: Option<Metric>,
    pub initial_volume: StorageInitialVolume,
    pub window: RollingWindow,
}

impl RollingVirtualStorageNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Volume;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl RollingVirtualStorageNode {
    pub fn node_indices_for_constraints(
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
                    .ok_or_else(|| SchemaError::NodeNotFound(node_ref.name.to_string()))?
                    .node_indices_for_constraints(network, args)
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect();
        Ok(indices)
    }
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        let cost = match &self.cost {
            Some(v) => v.load(network, args, Some(&self.meta.name))?.into(),
            None => None,
        };

        let min_volume = match &self.min_volume {
            Some(v) => Some(v.load(network, args, Some(&self.meta.name))?.try_into()?),
            None => None,
        };

        let max_volume = match &self.max_volume {
            Some(v) => Some(v.load(network, args, Some(&self.meta.name))?.try_into()?),
            None => None,
        };

        let node_idxs = self.node_indices_for_constraints(network, args)?;
        // The rolling licence never resets
        let reset = VirtualStorageReset::Never;
        let timesteps =
            self.window
                .as_timesteps(args.domain.time())
                .ok_or_else(|| SchemaError::InvalidRollingWindow {
                    name: self.meta.name.clone(),
                })?;

        let mut builder = VirtualStorageBuilder::new(self.meta.name.as_str(), &node_idxs)
            .initial_volume(self.initial_volume.into())
            .min_volume(min_volume)
            .max_volume(max_volume)
            .reset(reset)
            .rolling_window(timesteps)
            .cost(cost);

        if let Some(factors) = &self.factors {
            builder = builder.factors(factors);
        }

        network.add_virtual_storage_node(builder)?;
        Ok(())
    }
    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_virtual_storage_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Volume => MetricF64::VirtualStorageVolume(idx),
            NodeAttribute::ProportionalVolume => {
                let dm = DerivedMetric::VirtualStorageProportionalVolume(idx);
                let derived_metric_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(derived_metric_idx)
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "RollingVirtualStorageNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                });
            }
        };

        Ok(metric)
    }
}

impl TryFromV1<RollingVirtualStorageNodeV1> for RollingVirtualStorageNode {
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
            window,
        };
        Ok(n)
    }
}
