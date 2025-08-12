use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::{Metric, NodeComponentReference};
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::node_attribute_subset_enum;
#[cfg(feature = "core")]
use crate::nodes::NodeAttribute;
use crate::nodes::NodeMeta;
use crate::nodes::core::StorageInitialVolume;
use crate::parameters::Parameter;
use crate::v1::{ConversionData, TryFromV1, try_convert_initial_storage, try_convert_node_attr};
#[cfg(feature = "core")]
use pywr_core::{
    derived_metric::DerivedMetric,
    metric::MetricF64,
    virtual_storage::{VirtualStorageBuilder, VirtualStorageReset},
};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::nodes::VirtualStorageNode as VirtualStorageNodeV1;
use schemars::JsonSchema;

// This macro generates a subset enum for the `VirtualStorageNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum VirtualStorageNodeAttribute {
        Volume,
        ProportionalVolume,
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
// TODO write the cost documentation when linking a node to this cost is supported in the schema.
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
}

impl VirtualStorageNode {
    const DEFAULT_ATTRIBUTE: VirtualStorageNodeAttribute = VirtualStorageNodeAttribute::Volume;

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![]
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

        let node_idxs = self.node_indices_for_flow_constraints(network, args)?;

        // Standard virtual storage node never resets.
        let reset = VirtualStorageReset::Never;

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
        };
        Ok(n)
    }
}
