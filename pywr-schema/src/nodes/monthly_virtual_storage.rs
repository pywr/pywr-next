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
use pywr_v1_schema::nodes::MonthlyVirtualStorageNode as MonthlyVirtualStorageNodeV1;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub struct NumberOfMonthsReset {
    pub months: u8,
}

impl Default for NumberOfMonthsReset {
    fn default() -> Self {
        Self { months: 1 }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
pub struct MonthlyVirtualStorageNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub nodes: Vec<String>,
    pub factors: Option<Vec<f64>>,
    pub max_volume: Option<Metric>,
    pub min_volume: Option<Metric>,
    pub cost: Option<Metric>,
    pub initial_volume: StorageInitialVolume,
    pub reset: NumberOfMonthsReset,
}

impl MonthlyVirtualStorageNode {
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
impl MonthlyVirtualStorageNode {
    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let indices = self
            .nodes
            .iter()
            .map(|name| {
                args.schema
                    .get_node_by_name(name)
                    .ok_or_else(|| SchemaError::NodeNotFound(name.to_string()))?
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

        let node_idxs = self.node_indices_for_constraints(network, args)?;

        let reset = VirtualStorageReset::NumberOfMonths {
            months: self.reset.months as i32,
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
                    ty: "MonthlyVirtualStorageNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<MonthlyVirtualStorageNodeV1> for MonthlyVirtualStorageNode {
    type Error = ConversionError;

    fn try_from(v1: MonthlyVirtualStorageNodeV1) -> Result<Self, Self::Error> {
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
            reset: NumberOfMonthsReset { months: v1.months },
        };
        Ok(n)
    }
}
