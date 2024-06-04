use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::{NodeAttribute, NodeMeta};

use crate::parameters::TryIntoV2Parameter;
#[cfg(feature = "core")]
use pywr_core::{
    derived_metric::DerivedMetric, metric::MetricF64, node::StorageInitialVolume as CoreStorageInitialVolume,
};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::nodes::{
    AggregatedNode as AggregatedNodeV1, AggregatedStorageNode as AggregatedStorageNodeV1,
    CatchmentNode as CatchmentNodeV1, InputNode as InputNodeV1, LinkNode as LinkNodeV1, OutputNode as OutputNodeV1,
    ReservoirNode as ReservoirNodeV1, StorageNode as StorageNodeV1,
};
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
pub struct InputNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_flow: Option<Metric>,
    pub min_flow: Option<Metric>,
    pub cost: Option<Metric>,
}

impl InputNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }
    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl InputNode {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_input_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(network, args)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args)?;
            network.set_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args)?;
            network.set_node_min_flow(self.meta.name.as_str(), None, value.into())?;
        }

        Ok(())
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Outflow => MetricF64::NodeOutFlow(idx),
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "InputNode".to_string(),
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

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
pub struct LinkNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_flow: Option<Metric>,
    pub min_flow: Option<Metric>,
    pub cost: Option<Metric>,
}

impl LinkNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }
    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl LinkNode {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_link_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(network, args)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args)?;
            network.set_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args)?;
            network.set_node_min_flow(self.meta.name.as_str(), None, value.into())?;
        }

        Ok(())
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Outflow => MetricF64::NodeOutFlow(idx),
            NodeAttribute::Inflow => MetricF64::NodeInFlow(idx),
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "LinkNode".to_string(),
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

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
pub struct OutputNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_flow: Option<Metric>,
    pub min_flow: Option<Metric>,
    pub cost: Option<Metric>,
}

impl OutputNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Inflow;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl OutputNode {
    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Inflow => MetricF64::NodeInFlow(idx),
            NodeAttribute::Deficit => {
                let dm = DerivedMetric::NodeInFlowDeficit(idx);
                let dm_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(dm_idx)
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "OutputNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_output_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(network, args)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args)?;
            network.set_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args)?;
            network.set_node_min_flow(self.meta.name.as_str(), None, value.into())?;
        }

        Ok(())
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

#[derive(serde::Deserialize, serde::Serialize, Clone, PartialEq, Copy, Debug, JsonSchema, PywrVisitAll)]
pub enum StorageInitialVolume {
    Absolute(f64),
    Proportional(f64),
}

impl Default for StorageInitialVolume {
    fn default() -> Self {
        StorageInitialVolume::Proportional(1.0)
    }
}

#[cfg(feature = "core")]
impl From<StorageInitialVolume> for CoreStorageInitialVolume {
    fn from(v: StorageInitialVolume) -> Self {
        match v {
            StorageInitialVolume::Absolute(v) => CoreStorageInitialVolume::Absolute(v),
            StorageInitialVolume::Proportional(v) => CoreStorageInitialVolume::Proportional(v),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
pub struct StorageNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_volume: Option<Metric>,
    pub min_volume: Option<Metric>,
    pub cost: Option<Metric>,
    pub initial_volume: StorageInitialVolume,
}

impl StorageNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Volume;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl StorageNode {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        // Add the node with no constraints
        network.add_storage_node(self.meta.name.as_str(), None, self.initial_volume.into(), None, None)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(network, args)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_volume) = &self.min_volume {
            let value = min_volume.load(network, args)?;
            network.set_node_min_volume(self.meta.name.as_str(), None, Some(value.try_into()?))?;
        }

        if let Some(max_volume) = &self.max_volume {
            let value = max_volume.load(network, args)?;
            network.set_node_max_volume(self.meta.name.as_str(), None, Some(value.try_into()?))?;
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

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Volume => MetricF64::NodeVolume(idx),
            NodeAttribute::ProportionalVolume => {
                let dm = DerivedMetric::NodeProportionalVolume(idx);
                let derived_metric_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(derived_metric_idx)
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "StorageNode".to_string(),
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
            max_volume,
            min_volume,
            cost,
            initial_volume,
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

        let initial_volume = if let Some(v) = v1.initial_volume {
            StorageInitialVolume::Absolute(v)
        } else if let Some(v) = v1.initial_volume_pc {
            StorageInitialVolume::Proportional(v)
        } else {
            StorageInitialVolume::default()
        };

        let n = Self {
            meta,
            max_volume,
            min_volume,
            cost,
            initial_volume,
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
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
pub struct CatchmentNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub flow: Option<Metric>,
    pub cost: Option<Metric>,
}

impl CatchmentNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), None)]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl CatchmentNode {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        network.add_input_node(self.meta.name.as_str(), None)?;
        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(cost) = &self.cost {
            let value = cost.load(network, args)?;
            network.set_node_cost(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(flow) = &self.flow {
            let value = flow.load(network, args)?;
            network.set_node_min_flow(self.meta.name.as_str(), None, value.clone().into())?;
            network.set_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        Ok(())
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Outflow => MetricF64::NodeOutFlow(idx),
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "CatchmentNode".to_string(),
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

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
#[serde(tag = "type")]
pub enum Factors {
    Proportion { factors: Vec<Metric> },
    Ratio { factors: Vec<Metric> },
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
pub struct AggregatedNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub nodes: Vec<String>,
    pub max_flow: Option<Metric>,
    pub min_flow: Option<Metric>,
    pub factors: Option<Factors>,
}

impl AggregatedNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Not connectable
        // TODO this should be a trait? And error if you try to connect to a non-connectable node.
        vec![]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Not connectable
        vec![]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl AggregatedNode {
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
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        if let Some(max_flow) = &self.max_flow {
            let value = max_flow.load(network, args)?;
            network.set_aggregated_node_max_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(min_flow) = &self.min_flow {
            let value = min_flow.load(network, args)?;
            network.set_aggregated_node_min_flow(self.meta.name.as_str(), None, value.into())?;
        }

        if let Some(factors) = &self.factors {
            let f = match factors {
                Factors::Proportion { factors } => pywr_core::aggregated_node::Factors::Proportion(
                    factors
                        .iter()
                        .map(|f| f.load(network, args))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
                Factors::Ratio { factors } => pywr_core::aggregated_node::Factors::Ratio(
                    factors
                        .iter()
                        .map(|f| f.load(network, args))
                        .collect::<Result<Vec<_>, _>>()?,
                ),
            };

            network.set_aggregated_node_factors(self.meta.name.as_str(), None, Some(f))?;
        }

        Ok(())
    }

    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_aggregated_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Outflow => MetricF64::AggregatedNodeOutFlow(idx),
            NodeAttribute::Inflow => MetricF64::AggregatedNodeInFlow(idx),
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "AggregatedNode".to_string(),
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

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
pub struct AggregatedStorageNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub storage_nodes: Vec<String>,
}

impl AggregatedStorageNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Not connectable
        // TODO this should be a trait? And error if you try to connect to a non-connectable node.
        vec![]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        // Not connectable
        vec![]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl AggregatedStorageNode {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        let nodes = self
            .storage_nodes
            .iter()
            .map(|name| network.get_node_index_by_name(name, None))
            .collect::<Result<_, _>>()?;

        network.add_aggregated_storage_node(self.meta.name.as_str(), None, nodes)?;
        Ok(())
    }

    pub fn create_metric(
        &self,
        network: &mut pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let idx = network.get_aggregated_storage_node_index_by_name(self.meta.name.as_str(), None)?;

        let metric = match attr {
            NodeAttribute::Volume => MetricF64::AggregatedNodeVolume(idx),
            NodeAttribute::ProportionalVolume => {
                let dm = DerivedMetric::AggregatedNodeProportionalVolume(idx);
                let derived_metric_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(derived_metric_idx)
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "AggregatedStorageNode".to_string(),
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
    use crate::nodes::core::StorageInitialVolume;
    use crate::nodes::InputNode;
    use crate::nodes::StorageNode;
    use crate::PywrModel;
    use pywr_core::test_utils::run_all_solvers;
    use std::str::FromStr;

    #[test]
    fn test_input() {
        let data = r#"
            {
                "name": "supply1",
                "type": "Input",
                "max_flow": {
                    "type": "Constant",
                    "value": 15.0
                }
            }
            "#;

        let node: InputNode = serde_json::from_str(data).unwrap();

        assert_eq!(node.meta.name, "supply1");
    }

    #[test]
    fn test_storage_initial_volume_absolute() {
        let data = r#"
            {
                "name": "storage1",
                "type": "Storage",
                "volume": 15.0,
                "initial_volume": {
                    "Absolute": 12.0
                }
            }
            "#;

        let storage: StorageNode = serde_json::from_str(data).unwrap();

        assert_eq!(storage.initial_volume, StorageInitialVolume::Absolute(12.0));
    }

    #[test]
    fn test_storage_initial_volume_proportional() {
        let data = r#"
            {
                "name": "storage1",
                "type": "Storage",
                "volume": 15.0,
                "initial_volume": {
                    "Proportional": 0.5
                }
            }
            "#;

        let storage: StorageNode = serde_json::from_str(data).unwrap();

        assert_eq!(storage.initial_volume, StorageInitialVolume::Proportional(0.5));
    }

    fn storage_max_volumes_str() -> &'static str {
        include_str!("../test_models/storage_max_volumes.json")
    }

    #[test]
    #[cfg(feature = "core")]
    fn test_storage_max_volumes_run() {
        let data = storage_max_volumes_str();
        let schema = PywrModel::from_str(data).unwrap();
        let model: pywr_core::models::Model = schema.build_model(None, None).unwrap();
        // Test all solvers
        run_all_solvers(&model);
    }
}
