use crate::metric::Metric;
use crate::nodes::{NodeMeta, StorageInitialVolume};
use crate::parameters::Parameter;
#[cfg(feature = "core")]
use crate::{
    error::SchemaError,
    network::LoadArgs,
    nodes::{NodeAttribute, NodeSlot},
};
use crate::{mermaid, node_attribute_subset_enum};
#[cfg(feature = "core")]
use pywr_core::{
    metric::UnresolvedMetricF64,
    node::{UnresolvedNode, UnresolvedStorageInitialVolume},
    parameters::ParameterName,
    parameters::{DifferenceParameterBuilder, VolumeBetweenControlCurvesParameterBuilder},
};
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct PiecewiseStore {
    pub control_curve: Metric,
    pub cost: Option<Metric>,
}

// This macro generates a subset enum for the `PiecewiseStorageNode` attributes.
// It allows for easy conversion between the enum and the `NodeAttribute` type.
node_attribute_subset_enum! {
    pub enum PiecewiseStorageNodeAttribute {
        Volume,
        ProportionalVolume,
    }
}

/// This node is used to create a series of storage nodes with separate costs.
///
/// The series of storage nodes are created with bi-directional transfers to enable transfer
/// between the layers of storage. This node can be used as a more sophisticated storage
/// node where it is important for the volume to follow a control curve that separates the
/// volume into two or more stores (zones). By applying different penalty costs in each store
/// (zone) the allocation algorithm makes independent decisions regarding the use of each.
///
/// Initial volume can be set as a proportion of the total volume or as an absolute value.
/// This volume is distributed across the individual storage nodes, from the bottom up.
///
/// Note that this node adds additional complexity to models over the standard storage node.
///
#[doc = mermaid!("doc_diagrams/piecewise-storage.mmd")]
///
/// # Available attributes and components
///
/// The enum [`PiecewiseStorageNodeAttribute`] defines the available attributes. There are no components
/// to choose from.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct PiecewiseStorageNode {
    pub meta: NodeMeta,
    /// Optional local parameters.
    pub parameters: Option<Vec<Parameter>>,
    pub max_volume: Metric,
    pub min_volume: Option<Metric>,
    pub cost: Option<Metric>,
    pub steps: Vec<PiecewiseStore>,
    pub initial_volume: StorageInitialVolume,
}

impl PiecewiseStorageNode {
    const DEFAULT_ATTRIBUTE: PiecewiseStorageNodeAttribute = PiecewiseStorageNodeAttribute::Volume;

    pub fn default_attribute(&self) -> PiecewiseStorageNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl PiecewiseStorageNode {
    fn step_sub_name(&self, i: usize) -> UnresolvedNode {
        UnresolvedNode::new(self.meta.name.as_str(), Some(&format!("store-{i:02}")))
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok((0..self.steps.len()).map(|i| self.step_sub_name(i)).collect())
        }
    }
    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<UnresolvedNode>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok((0..self.steps.len()).map(|i| self.step_sub_name(i)).collect())
        }
    }
    fn agg_sub_name(&self) -> UnresolvedNode {
        UnresolvedNode::new(&self.meta.name, Some("agg-store"))
    }

    pub fn nodes_for_storage_constraints(&self) -> Result<Vec<UnresolvedNode>, SchemaError> {
        // Get the indices of all the sub-nodes for this piecewise storage node (including
        // the final one that represents the residual part above the last step).
        let nodes = (0..self.steps.len() + 1)
            .map(|i| self.step_sub_name(i))
            .collect::<Vec<_>>();

        Ok(nodes)
    }

    pub fn add_to_network(
        &self,
        network: &mut pywr_core::network::NetworkBuilder,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        let mut store_nodes: Vec<UnresolvedNode> = Vec::new();

        // These are the min and max volume of the overall node
        let total_volume: UnresolvedMetricF64 = self.max_volume.load(network, args, Some(&self.meta.name))?;
        let total_min_volume: Option<UnresolvedMetricF64> = match &self.min_volume {
            Some(min_volume) => Some(min_volume.load(network, args, Some(&self.meta.name))?),
            None => None,
        };

        let mut prior_max_volumes: Vec<UnresolvedMetricF64> = Vec::new();

        for (i, step) in self.steps.iter().enumerate() {
            // Create a storage node builder for each step
            let mut storage = pywr_core::node::NodeBuilder::storage(self.step_sub_name(i));

            // Assume each store is full to start with
            let initial_volume = UnresolvedStorageInitialVolume::Proportional(1.0);
            storage.initial_volume(initial_volume);

            // The volume of this step is the proportion between the last control curve
            // (or zero if first) and this control curve.
            let lower = if i > 0 {
                Some(
                    self.steps[i - 1]
                        .control_curve
                        .load(network, args, Some(&self.meta.name))?,
                )
            } else {
                None
            };

            let upper = step.control_curve.load(network, args, Some(&self.meta.name))?;

            // Set the max volume
            let max_volume_metric = self.set_sub_node_max_volume(
                i,
                total_volume.clone(),
                Some(upper),
                lower.clone(),
                network,
                &mut storage,
            );

            let prior_max_volume_name =
                ParameterName::new(&format!("store-{i:02}-prior-max-volume"), Some(&self.meta.name));
            let mut prior_max_volume = pywr_core::parameters::AggregatedParameterBuilder::new(
                prior_max_volume_name.clone(),
                pywr_core::agg_funcs::AggFuncF64::Sum,
            );
            let prior_max_volume_metric = UnresolvedMetricF64::new_parameter_before(prior_max_volume_name.clone());

            // Add all the prior maxs to the aggregated node
            for pmv in &prior_max_volumes {
                prior_max_volume.metric(pmv.clone());
            }
            network.parameters().f64(Box::new(prior_max_volume));

            if let Some(total_min_volume) = total_min_volume.clone() {
                self.set_sub_node_min_volume(
                    i,
                    total_min_volume.clone(),
                    prior_max_volume_metric.clone(),
                    max_volume_metric.clone(),
                    network,
                    &mut storage,
                );
            }

            self.set_sub_node_initial_volume(total_volume.clone(), prior_max_volume_metric, &mut storage);

            // Append the max volume parameter of this node to the prior list
            prior_max_volumes.push(max_volume_metric);

            if let Some(cost) = &step.cost {
                let value = cost.load(network, args, Some(&self.meta.name))?;
                storage.cost(value);
            }

            let name = storage.name();

            if let Some(prev_name) = store_nodes.last() {
                // There was a lower store; connect to it in both directions
                network.connect(name.clone(), prev_name.clone());
                network.connect(prev_name.clone(), name.clone());
            }

            store_nodes.push(name.clone());
            network.node(storage);
        }

        // Assume each store is full to start with
        let initial_volume = UnresolvedStorageInitialVolume::Proportional(1.0);

        // And one for the residual part above the less step
        let mut storage = pywr_core::node::NodeBuilder::storage(self.step_sub_name(self.steps.len()));
        storage.initial_volume(initial_volume);

        // The volume of this store is the remain proportion above the last control curve
        let lower = match self.steps.last() {
            Some(step) => Some(step.control_curve.load(network, args, Some(&self.meta.name))?),
            None => None,
        };

        let upper = None;

        let max_volume_parameter_metric = self.set_sub_node_max_volume(
            self.steps.len(),
            total_volume.clone(),
            upper,
            lower,
            network,
            &mut storage,
        );

        let prior_max_volume_name = ParameterName::new(
            &format!("store-{:02}-prior-max-volume", self.steps.len()),
            Some(&self.meta.name),
        );

        let mut prior_max_volume = pywr_core::parameters::AggregatedParameterBuilder::new(
            prior_max_volume_name.clone(),
            pywr_core::agg_funcs::AggFuncF64::Sum,
        );

        for pmv in &prior_max_volumes {
            prior_max_volume.metric(pmv.clone());
        }
        let prior_max_volume_metric = UnresolvedMetricF64::new_parameter_before(prior_max_volume_name.clone());

        network.parameters().f64(Box::new(prior_max_volume));

        if let Some(total_min_volume) = total_min_volume.clone() {
            self.set_sub_node_min_volume(
                self.steps.len(),
                total_min_volume.clone(),
                prior_max_volume_metric.clone(),
                max_volume_parameter_metric,
                network,
                &mut storage,
            );
        }

        self.set_sub_node_initial_volume(total_volume.clone(), prior_max_volume_metric, &mut storage);

        // Set the cost for the last step
        if let Some(cost) = &self.cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            storage.cost(value);
        }

        let name = storage.name();

        if let Some(prev_name) = store_nodes.last() {
            // There was a lower store; connect to it in both directions
            network.connect(name.clone(), prev_name.clone());
            network.connect(prev_name.clone(), name.clone());
        }

        store_nodes.push(name.clone());
        network.node(storage);

        // Finally, add an aggregate storage node covering all the individual stores
        let mut agg_storage = pywr_core::AggregatedStorageNodeBuilder::new(self.agg_sub_name());

        for node in store_nodes {
            agg_storage.node(node);
        }
        network.agg_storage_node(agg_storage);

        Ok(())
    }

    /// Set the maximum volume of the node at a specific step index.
    ///
    /// This method sets the maximum volume for a specific step in the piecewise storage node. It
    /// creates a `VolumeBetweenControlCurvesParameter` that defines the maximum volume for that step,
    /// based on the provided lower and upper bounds, and the total volume of the node.
    ///
    fn set_sub_node_max_volume(
        &self,
        step_index: usize,
        total_volume: UnresolvedMetricF64,
        upper: Option<UnresolvedMetricF64>,
        lower: Option<UnresolvedMetricF64>,
        network: &mut pywr_core::network::NetworkBuilder,
        node: &mut pywr_core::node::NodeBuilder,
    ) -> UnresolvedMetricF64 {
        let max_volume_name = ParameterName::new(&format!("store-{:02}-max-volume", step_index), Some(&self.meta.name));

        let mut max_volume_parameter = VolumeBetweenControlCurvesParameterBuilder::new(
            // Node's name is the parent identifier
            max_volume_name.clone(),
            total_volume,
        );

        if let Some(upper) = upper {
            max_volume_parameter.upper(upper);
        }
        if let Some(lower) = lower {
            max_volume_parameter.lower(lower);
        }

        // Add the parameter and link it to the node's max volume.
        network.parameters().f64(Box::new(max_volume_parameter));
        let max_volume = UnresolvedMetricF64::new_parameter_before(max_volume_name);
        node.max_volume(max_volume.clone());
        max_volume
    }

    /// Set the minimum volume of the node at a specific step index.
    ///
    /// This method sets the minimum volume for a specific step in the piecewise storage node. It
    /// creates a `DifferenceParameter` that defines the minimum volume for that step, based on the
    /// total minimum volume and the maximum volume of the previous steps.
    fn set_sub_node_min_volume(
        &self,
        step_index: usize,
        total_min_volume: UnresolvedMetricF64,
        prior_max_volume: UnresolvedMetricF64,
        max_volume_parameter: UnresolvedMetricF64,
        network: &mut pywr_core::network::NetworkBuilder,
        node: &mut pywr_core::node::NodeBuilder,
    ) {
        let min_volume_name = ParameterName::new(&format!("store-{:02}-min-volume", step_index), Some(&self.meta.name));
        // The minimum volume is the difference between the total volume and
        // the maximum volume of the previous steps, but limited to be between zero and the maximum volume of this step.
        let mut min_volume_parameter =
            DifferenceParameterBuilder::new(min_volume_name.clone(), total_min_volume, prior_max_volume.clone());

        min_volume_parameter.min(0.0.into()).max(max_volume_parameter);

        network.parameters().f64(Box::new(min_volume_parameter));
        node.min_volume(UnresolvedMetricF64::new_parameter_before(min_volume_name));
    }

    /// Set the initial volume of the node at a specific step index.
    fn set_sub_node_initial_volume(
        &self,
        total_volume: UnresolvedMetricF64,
        prior_max_volume: UnresolvedMetricF64,
        node: &mut pywr_core::node::NodeBuilder,
    ) {
        // Set the initial volume of this step
        let initial_volume = match &self.initial_volume {
            StorageInitialVolume::Proportional { proportion } => {
                UnresolvedStorageInitialVolume::DistributedProportional {
                    total_volume: total_volume.clone(),
                    proportion: *proportion,
                    prior_max_volume,
                }
            }
            StorageInitialVolume::Absolute { volume } => UnresolvedStorageInitialVolume::DistributedAbsolute {
                absolute: *volume,
                prior_max_volume,
            },
        };

        node.initial_volume(initial_volume);
    }

    pub fn create_metric(&self, attribute: Option<NodeAttribute>) -> Result<UnresolvedMetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = match attribute {
            Some(attr) => attr.try_into()?,
            None => Self::DEFAULT_ATTRIBUTE,
        };

        let name = self.agg_sub_name();

        let metric = match attr {
            PiecewiseStorageNodeAttribute::Volume => UnresolvedMetricF64::AggregatedStorageNodeVolume(name),
            PiecewiseStorageNodeAttribute::ProportionalVolume => {
                UnresolvedMetricF64::AggregatedStorageNodeProportionalVolume(name)
            }
        };

        Ok(metric)
    }
}
