use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::{Metric, SimpleNodeReference};
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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct AnnualVirtualStorageNode {
    pub meta: NodeMeta,
    pub nodes: Vec<SimpleNodeReference>,
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

impl AnnualVirtualStorageNode {
    #[cfg(feature = "core")]
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        let cost = match &self.cost {
            Some(v) => v.load(network, args)?.into(),
            None => None,
        };

        let min_volume = match &self.min_volume {
            Some(v) => Some(v.load(network, args)?.try_into()?),
            None => None,
        };

        let max_volume = match &self.max_volume {
            Some(v) => Some(v.load(network, args)?.try_into()?),
            None => None,
        };

        let node_idxs = self
            .nodes
            .iter()
            .map(|node_ref| network.get_node_index_by_name(&node_ref.name, None))
            .collect::<Result<Vec<_>, _>>()?;

        let reset_month = self.reset.month.try_into()?;
        let reset = VirtualStorageReset::DayOfYear {
            day: self.reset.day as u32,
            month: reset_month,
        };

        let mut builder = VirtualStorageBuilder::new(self.meta.name.as_str(), &node_idxs)
            .initial_volume(self.initial_volume.into())
            .min_volume(min_volume)
            .max_volume(max_volume)
            .reset(reset)
            .cost(cost);

        if let Some(factors) = &self.factors {
            builder = builder.factors(factors);
        }

        network.add_virtual_storage_node(builder)?;
        Ok(())
    }

    #[cfg(feature = "core")]
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

        let nodes = v1.nodes.into_iter().map(|n| n.into()).collect();

        let n = Self {
            meta,
            nodes,
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
