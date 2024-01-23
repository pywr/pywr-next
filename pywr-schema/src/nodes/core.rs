use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::model::PywrMultiNetworkTransfer;
use crate::nodes::{NodeAttribute, NodeMeta};
use crate::parameters::{DynamicFloatValue, TryIntoV2Parameter};
use pywr_core::derived_metric::DerivedMetric;
use pywr_core::metric::Metric;
use pywr_core::models::ModelDomain;
use pywr_core::node::{ConstraintValue, StorageInitialVolume};
use pywr_v1_schema::nodes::{
    AggregatedNode as AggregatedNodeV1, AggregatedStorageNode as AggregatedStorageNodeV1,
    CatchmentNode as CatchmentNodeV1, InputNode as InputNodeV1, LinkNode as LinkNodeV1, OutputNode as OutputNodeV1,
    ReservoirNode as ReservoirNodeV1, StorageNode as StorageNodeV1,
};
use std::collections::HashMap;
use std::path::Path;

#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct InputNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_flow: Option<DynamicFloatValue>,
    pub min_flow: Option<DynamicFloatValue>,
    pub cost: Option<DynamicFloatValue>,
}

impl InputNode {
    pub const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn parameters(&self) -> HashMap<&str, &DynamicFloatValue> {
        let mut attributes = HashMap::new();
        if let Some(p) = &self.max_flow {
            attributes.insert("max_flow", p);
        }
        if let Some(p) = &self.min_flow {
            attributes.insert("min_flow", p);
        }
        if let Some(p) = &self.cost {
            attributes.insert("cost", p);
        }

        attributes
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_input_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_min_flow(self.meta.name.as_str(), None, value.into())?;
        }

        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }
    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<Metric, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Outflow => Metric::NodeOutFlow(idx),
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<InputNodeV1> for InputNode {
    type Error = ConversionError;

    fn try_from(v1: InputNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let max_flow = v1
            .max_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let min_flow = v1
            .min_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        let cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let n = Self {
            meta,
            max_flow,
            min_flow,
            cost,
        };
        Ok(n)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct LinkNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_flow: Option<DynamicFloatValue>,
    pub min_flow: Option<DynamicFloatValue>,
    pub cost: Option<DynamicFloatValue>,
}

impl LinkNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn parameters(&self) -> HashMap<&str, &DynamicFloatValue> {
        let mut attributes = HashMap::new();
        if let Some(p) = &self.max_flow {
            attributes.insert("max_flow", p);
        }
        if let Some(p) = &self.min_flow {
            attributes.insert("min_flow", p);
        }
        if let Some(p) = &self.cost {
            attributes.insert("cost", p);
        }

        attributes
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_link_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_min_flow(self.meta.name.as_str(), None, value.into())?;
        }

        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }
    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<Metric, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Outflow => Metric::NodeOutFlow(idx),
            NodeAttribute::Inflow => Metric::NodeInFlow(idx),
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<LinkNodeV1> for LinkNode {
    type Error = ConversionError;

    fn try_from(v1: LinkNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let max_flow = v1
            .max_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        let min_flow = v1
            .min_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        let cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let n = Self {
            meta,
            max_flow,
            min_flow,
            cost,
        };
        Ok(n)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct OutputNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_flow: Option<DynamicFloatValue>,
    pub min_flow: Option<DynamicFloatValue>,
    pub cost: Option<DynamicFloatValue>,
}

impl OutputNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Inflow;

    pub fn parameters(&self) -> HashMap<&str, &DynamicFloatValue> {
        let mut attributes = HashMap::new();
        if let Some(p) = &self.max_flow {
            attributes.insert("max_flow", p);
        }
        if let Some(p) = &self.min_flow {
            attributes.insert("min_flow", p);
        }
        if let Some(p) = &self.cost {
            attributes.insert("cost", p);
        }

        attributes
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_output_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_min_flow(self.meta.name.as_str(), None, value.into())?;
        }

        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<Metric, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Inflow => Metric::NodeInFlow(idx),
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<OutputNodeV1> for OutputNode {
    type Error = ConversionError;

    fn try_from(v1: OutputNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let max_flow = v1
            .max_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        let min_flow = v1
            .min_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        let cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let n = Self {
            meta,
            max_flow,
            min_flow,
            cost,
        };
        Ok(n)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct StorageNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_volume: Option<DynamicFloatValue>,
    pub min_volume: Option<DynamicFloatValue>,
    pub cost: Option<DynamicFloatValue>,
    pub initial_volume: Option<f64>,
    pub initial_volume_pc: Option<f64>,
}

impl StorageNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Volume;

    pub fn parameters(&self) -> HashMap<&str, &DynamicFloatValue> {
        let mut attributes = HashMap::new();
        // if let Some(p) = &self.max_volume {
        //     attributes.insert("max_volume", p);
        // }
        // if let Some(p) = &self.min_volume {
        //     attributes.insert("min_volume", p);
        // }
        if let Some(p) = &self.cost {
            attributes.insert("cost", p);
        }

        attributes
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<(), SchemaError> {
        let initial_volume = if let Some(iv) = self.initial_volume {
            StorageInitialVolume::Absolute(iv)
        } else if let Some(pc) = self.initial_volume_pc {
            StorageInitialVolume::Proportional(pc)
        } else {
            return Err(SchemaError::MissingInitialVolume(self.meta.name.to_string()));
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

        network.add_storage_node(self.meta.name.as_str(), None, initial_volume, min_volume, max_volume)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<Metric, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Volume => Metric::NodeVolume(idx),
            NodeAttribute::ProportionalVolume => {
                let dm = DerivedMetric::NodeProportionalVolume(idx);
                let derived_metric_idx = network.add_derived_metric(dm);
                Metric::DerivedMetric(derived_metric_idx)
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<StorageNodeV1> for StorageNode {
    type Error = ConversionError;

    fn try_from(v1: StorageNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()
            .map_err(|source| ConversionError::NodeAttribute {
                attr: "cost".to_string(),
                name: meta.name.clone(),
                source: Box::new(source),
            })?;

        let max_volume = v1
            .max_volume
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()
            .map_err(|source| ConversionError::NodeAttribute {
                attr: "max_volume".to_string(),
                name: meta.name.clone(),
                source: Box::new(source),
            })?;

        let min_volume = v1
            .min_volume
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()
            .map_err(|source| ConversionError::NodeAttribute {
                attr: "min_volume".to_string(),
                name: meta.name.clone(),
                source: Box::new(source),
            })?;

        let n = Self {
            meta,
            max_volume,
            min_volume,
            cost,
            initial_volume: v1.initial_volume,
            initial_volume_pc: v1.initial_volume_pc,
        };
        Ok(n)
    }
}

impl TryFrom<ReservoirNodeV1> for StorageNode {
    type Error = ConversionError;

    fn try_from(v1: ReservoirNodeV1) -> Result<Self, Self::Error> {
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

        let n = Self {
            meta,
            max_volume,
            min_volume,
            cost,
            initial_volume: v1.initial_volume,
            initial_volume_pc: v1.initial_volume_pc,
        };
        Ok(n)
    }
}

#[doc = svgbobdoc::transform!(
/// This is used to represent a catchment inflow.
///
/// Catchment nodes create a single [`crate::node::InputNode`] node in the network, but
/// ensure that the maximum and minimum flow are equal to [`Self::flow`].
///
/// ```svgbob
///  <node>     D
///     *----->*- - -
/// ```
///
)]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct CatchmentNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub flow: Option<DynamicFloatValue>,
    pub cost: Option<DynamicFloatValue>,
}

impl CatchmentNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_input_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(flow) = &self.flow {
            let value = flow.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_node_min_flow(self.meta.name.as_str(), None, value.clone().into())?;
            network.set_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<Metric, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Outflow => Metric::NodeOutFlow(idx),
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<CatchmentNodeV1> for CatchmentNode {
    type Error = ConversionError;

    fn try_from(v1: CatchmentNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let flow = v1
            .flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;
        let cost = v1
            .cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let n = Self { meta, flow, cost };
        Ok(n)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
#[serde(tag = "type")]
pub enum Factors {
    Proportion { factors: Vec<DynamicFloatValue> },
    Ratio { factors: Vec<DynamicFloatValue> },
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct AggregatedNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub nodes: Vec<String>,
    pub max_flow: Option<DynamicFloatValue>,
    pub min_flow: Option<DynamicFloatValue>,
    pub factors: Option<Factors>,
}

impl AggregatedNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        let nodes = self
            .nodes
            .iter()
            .map(|name| network.get_node_index_by_name(name, None))
            .collect::<Result<Vec<_>, _>>()?;

        // We initialise with no factors, but will update them in the `set_constraints` method
        // once all the parameters are loaded.
        network.add_aggregated_node(self.meta.name.as_str(), None, nodes.as_slice(), None)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        schema: &crate::model::PywrNetwork,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<(), SchemaError> {
        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_aggregated_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, schema, domain, tables, data_path, inter_network_transfers)?;
            network.set_aggregated_node_min_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(factors) = &self.factors {
            let f = match factors {
                Factors::Proportion { factors } => pywr_core::aggregated_node::Factors::Proportion(
                    factors
                        .iter()
                        .map(|f| f.load(network, schema, domain, tables, data_path, inter_network_transfers))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                Factors::Ratio { factors } => pywr_core::aggregated_node::Factors::Ratio(
                    factors
                        .iter()
                        .map(|f| f.load(network, schema, domain, tables, data_path, inter_network_transfers))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            };

            network.set_aggregated_node_factors(self.meta.name.as_str(), None, Some(f))?;
        }

        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Not connectable
        // TODO this should be a trait? And error if you try to connect to a non-connectable node.
        vec![]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Not connectable
        vec![]
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<Metric, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_aggregated_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Outflow => Metric::AggregatedNodeOutFlow(idx),
            NodeAttribute::Inflow => Metric::AggregatedNodeInFlow(idx),
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<AggregatedNodeV1> for AggregatedNode {
    type Error = ConversionError;

    fn try_from(v1: AggregatedNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let factors = match v1.factors {
            Some(f) => Some(Factors::Ratio {
                factors: f
                    .into_iter()
                    .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
                    .collect::<Result<_, _>>()?,
            }),
            None => None,
        };

        let max_flow = v1
            .max_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let min_flow = v1
            .min_flow
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let n = Self {
            meta,
            nodes: v1.nodes,
            max_flow,
            min_flow,
            factors,
        };
        Ok(n)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct AggregatedStorageNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub storage_nodes: Vec<String>,
}

impl AggregatedStorageNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        let nodes = self
            .storage_nodes
            .iter()
            .map(|name| network.get_node_index_by_name(name, None))
            .collect::<Result<_, _>>()?;

        network.add_aggregated_storage_node(self.meta.name.as_str(), None, nodes)?;
        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Not connectable
        // TODO this should be a trait? And error if you try to connect to a non-connectable node.
        vec![]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Not connectable
        vec![]
    }

    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<Metric, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_aggregated_storage_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Volume => Metric::AggregatedNodeVolume(idx),
            NodeAttribute::ProportionalVolume => {
                let dm = DerivedMetric::AggregatedNodeProportionalVolume(idx);
                let derived_metric_idx = network.add_derived_metric(dm);
                Metric::DerivedMetric(derived_metric_idx)
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}

impl TryFrom<AggregatedStorageNodeV1> for AggregatedStorageNode {
    type Error = ConversionError;

    fn try_from(v1: AggregatedStorageNodeV1) -> Result<Self, Self::Error> {
        let n = Self {
            meta: v1.meta.into(),
            storage_nodes: v1.storage_nodes,
        };
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use crate::nodes::InputNode;

    #[test]
    fn test_input() {
        let data = r#"
            {
                "name": "supply1",
                "type": "Input",
                "max_flow": 15.0
            }
            "#;

        let node: InputNode = serde_json::from_str(data).unwrap();

        assert_eq!(node.meta.name, "supply1");
    }
}
