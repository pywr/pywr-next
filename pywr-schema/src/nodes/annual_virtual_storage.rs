use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::core::StorageInitialVolume;
use crate::nodes::{NodeAttribute, NodeMeta};
use crate::parameters::TryIntoV2Parameter;
#[cfg(feature = "core")]
use pywr_core::{
    derived_metric::DerivedMetric,
    metric::MetricF64,
    virtual_storage::{VirtualStorageBuilder, VirtualStorageReset},
};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::nodes::AnnualVirtualStorageNode as AnnualVirtualStorageNodeV1;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub struct AnnualReset {
    pub day: u8,
    pub month: u8,
    pub use_initial_volume: bool,
}

impl Default for AnnualReset {
    fn default() -> Self {
        Self {
            day: 1,
            month: 1,
            use_initial_volume: false,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
pub struct AnnualVirtualStorageNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub nodes: Vec<String>,
    pub factors: Option<Vec<f64>>,
    pub max_volume: Option<Metric>,
    pub min_volume: Option<Metric>,
    pub cost: Option<Metric>,
    pub initial_volume: StorageInitialVolume,
    pub reset: AnnualReset,
}

impl AnnualVirtualStorageNode {
    pub const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Volume;

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
impl AnnualVirtualStorageNode {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        let node_idxs = self
            .nodes
            .iter()
            .map(|name| network.get_node_index_by_name(name.as_str(), None))
            .collect::<Result<Vec<_>, _>>()?;

        let reset_month = self.reset.month.try_into()?;
        let reset = VirtualStorageReset::DayOfYear {
            day: self.reset.day as u32,
            month: reset_month,
        };

        let mut builder = VirtualStorageBuilder::new(self.meta.name.as_str(), &node_idxs)
            .initial_volume(self.initial_volume.into())
            .reset(reset);

        if let Some(factors) = &self.factors {
            builder = builder.factors(factors);
        }

        network.get_or_add_virtual_storage_node(builder)?;

        let min_volume = self.min_volume.as_ref().map(|v| v.load(network, args)).transpose()?;
        let max_volume = self.max_volume.as_ref().map(|v| v.load(network, args)).transpose()?;
        let cost = self.cost.as_ref().map(|v| v.load(network, args)).transpose()?;

        let node = network.get_virtual_storage_node_by_name_mut(self.meta.name.as_str(), None)?;

        if let Some(value) = min_volume {
            node.set_min_volume(value.into());
        }
        if let Some(value) = max_volume {
            node.set_max_volume(value.into());
        }
        if let Some(value) = cost {
            node.set_cost(value.into());
        }

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
                    ty: "AnnualVirtualStorageNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<AnnualVirtualStorageNodeV1> for AnnualVirtualStorageNode {
    type Error = ConversionError;

    fn try_from(v1: AnnualVirtualStorageNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        let max_volume = v1
            .max_volume
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let min_volume = v1
            .min_volume
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let initial_volume = if let Some(v) = v1.initial_volume {
            StorageInitialVolume::Absolute(v)
        } else if let Some(v) = v1.initial_volume_pc {
            StorageInitialVolume::Proportional(v)
        } else {
            return Err(ConversionError::MissingAttribute {
                name: meta.name,
                attrs: vec!["initial_volume".to_string(), "initial_volume_pc".to_string()],
            });
        };

        let n = Self {
            meta,
            nodes: v1.nodes,
            factors: v1.factors,
            max_volume,
            min_volume,
            cost,
            initial_volume,
            reset: AnnualReset {
                day: v1.reset_day as u8,
                month: v1.reset_month as u8,
                use_initial_volume: v1.reset_to_initial_volume,
            },
        };
        Ok(n)
    }
}
