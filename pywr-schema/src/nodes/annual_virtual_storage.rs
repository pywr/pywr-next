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
    derived_metric::DerivedMetric, metric::MetricF64, node::ConstraintValue, virtual_storage::VirtualStorageReset,
};
use pywr_schema_macros::PywrNode;
use pywr_v1_schema::nodes::AnnualVirtualStorageNode as AnnualVirtualStorageNodeV1;
use std::collections::HashMap;

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct AnnualReset {
    pub day: u8,
    pub month: chrono::Month,
    pub use_initial_volume: bool,
}

impl Default for AnnualReset {
    fn default() -> Self {
        Self {
            day: 1,
            month: chrono::Month::January,
            use_initial_volume: false,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, PywrNode)]
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

impl AnnualVirtualStorageNode {
    #[cfg(feature = "core")]
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        let cost = match &self.cost {
            Some(v) => v.load(network, args)?.into(),
            None => ConstraintValue::Scalar(0.0),
        };

        let min_volume = match &self.min_volume {
            Some(v) => v.load(network, args)?.into(),
            None => ConstraintValue::Scalar(0.0),
        };

        let max_volume = match &self.max_volume {
            Some(v) => v.load(network, args)?.into(),
            None => ConstraintValue::None,
        };

        let node_idxs = self
            .nodes
            .iter()
            .map(|name| network.get_node_index_by_name(name.as_str(), None))
            .collect::<Result<Vec<_>, _>>()?;

        let reset = VirtualStorageReset::DayOfYear {
            day: self.reset.day as u32,
            month: self.reset.month,
        };

        network.add_virtual_storage_node(
            self.meta.name.as_str(),
            None,
            node_idxs.as_ref(),
            self.factors.as_deref(),
            self.initial_volume.into(),
            min_volume,
            max_volume,
            reset,
            None,
            cost,
        )?;
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

        let month = chrono::Month::try_from(v1.reset_month as u8)?;

        let n = Self {
            meta,
            nodes: v1.nodes,
            factors: v1.factors,
            max_volume,
            min_volume,
            cost,
            initial_volume,
            reset: AnnualReset {
                day: v1.reset_day,
                month,
                use_initial_volume: v1.reset_to_initial_volume,
            },
        };
        Ok(n)
    }
}
