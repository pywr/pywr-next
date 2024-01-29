use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::model::PywrMultiNetworkTransfer;
use crate::nodes::core::StorageInitialVolume;
use crate::nodes::{NodeAttribute, NodeMeta};
use crate::parameters::{DynamicFloatValue, TryIntoV2Parameter};
use pywr_core::derived_metric::DerivedMetric;
use pywr_core::metric::Metric;
use pywr_core::models::ModelDomain;
use pywr_core::node::ConstraintValue;
use pywr_core::virtual_storage::VirtualStorageReset;
use pywr_v1_schema::nodes::MonthlyVirtualStorageNode as MonthlyVirtualStorageNodeV1;
use std::path::Path;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct NumberOfMonthsReset {
    pub months: u8,
}

impl Default for NumberOfMonthsReset {
    fn default() -> Self {
        Self { months: 1 }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct MonthlyVirtualStorageNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub nodes: Vec<String>,
    pub factors: Option<Vec<f64>>,
    pub max_volume: Option<DynamicFloatValue>,
    pub min_volume: Option<DynamicFloatValue>,
    pub cost: Option<DynamicFloatValue>,
    pub initial_volume: StorageInitialVolume,
    pub reset: NumberOfMonthsReset,
}

impl MonthlyVirtualStorageNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Volume;

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<(), SchemaError> {
        let cost = match &self.cost {
            Some(v) => v
                .load(network, schema, domain, tables, data_path, inter_network_transfers)?
                .into(),
            None => ConstraintValue::Scalar(0.0),
        };

        let min_volume = match &self.min_volume {
            Some(v) => v
                .load(network, schema, domain, tables, data_path, inter_network_transfers)?
                .into(),
            None => ConstraintValue::Scalar(0.0),
        };

        let max_volume = match &self.max_volume {
            Some(v) => v
                .load(network, schema, domain, tables, data_path, inter_network_transfers)?
                .into(),
            None => ConstraintValue::None,
        };

        let node_idxs = self
            .nodes
            .iter()
            .map(|name| network.get_node_index_by_name(name.as_str(), None))
            .collect::<Result<Vec<_>, _>>()?;

        let reset = VirtualStorageReset::NumberOfMonths {
            months: self.reset.months as i32,
        };

        // TODO this should be an annual virtual storage!
        network.add_virtual_storage_node(
            self.meta.name.as_str(),
            None,
            node_idxs.as_ref(),
            self.factors.as_deref(),
            self.initial_volume.into(),
            min_volume,
            max_volume,
            reset,
            cost,
        )?;
        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![]
    }

    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<Metric, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_virtual_storage_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Volume => Metric::VirtualStorageVolume(idx),
            NodeAttribute::ProportionalVolume => {
                let dm = DerivedMetric::VirtualStorageProportionalVolume(idx);
                let derived_metric_idx = network.add_derived_metric(dm);
                Metric::DerivedMetric(derived_metric_idx)
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
            StorageInitialVolume::default()
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
