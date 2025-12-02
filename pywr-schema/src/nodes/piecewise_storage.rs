use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::LoadArgs;
use crate::node_attribute_subset_enum;
#[cfg(feature = "core")]
use crate::nodes::NodeAttribute;
use crate::nodes::{NodeMeta, NodeSlot, StorageInitialVolume};
use crate::parameters::Parameter;
#[cfg(feature = "core")]
use pywr_core::{
    derived_metric::DerivedMetric,
    metric::{MetricF64, SimpleMetricF64},
    parameters::{DifferenceParameter, ParameterIndex, ParameterName, VolumeBetweenControlCurvesParameter},
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

#[doc = svgbobdoc::transform!(
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
/// ```svgbob
///
///            <node>.00            D
///     -*---------->S ----------->*-
///      U           ^
///                  |
///                  v
///       <node>.01  S
///                  ^
///                  :
///                  v
///      <node>.n    S
/// ```
///
/// # Available attributes and components
///
/// The enum [`PiecewiseStorageNodeAttribute`] defines the available attributes. There are no components
/// to choose from.
///
)]
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

    fn step_sub_name(i: usize) -> Option<String> {
        Some(format!("store-{i:02}"))
    }

    pub fn input_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::InputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(vec![(self.meta.name.as_str(), Self::step_sub_name(self.steps.len()))])
        }
    }
    pub fn output_connectors(&self, slot: Option<&NodeSlot>) -> Result<Vec<(&str, Option<String>)>, SchemaError> {
        if let Some(slot) = slot {
            Err(SchemaError::OutputNodeSlotNotSupported { slot: slot.clone() })
        } else {
            Ok(vec![(self.meta.name.as_str(), Self::step_sub_name(self.steps.len()))])
        }
    }

    pub fn default_attribute(&self) -> PiecewiseStorageNodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl PiecewiseStorageNode {
    fn agg_sub_name() -> Option<&'static str> {
        Some("agg-store")
    }

    pub fn node_indices_for_storage_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        // Get the indices of all the sub-nodes for this piecewise storage node (including
        // the final one that represents the residual part above the last step).
        let indices = (0..self.steps.len() + 1)
            .map(|i| {
                network
                    .get_node_index_by_name(self.meta.name.as_str(), Self::step_sub_name(i).as_deref())
                    .ok_or_else(|| SchemaError::CoreNodeNotFound {
                        name: self.meta.name.clone(),
                        sub_name: Self::step_sub_name(i),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(indices)
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        let mut store_node_indices = Vec::new();

        // create a storage node for each step
        for (i, _step) in self.steps.iter().enumerate() {
            // Assume each store is full to start with
            let initial_volume = pywr_core::node::StorageInitialVolume::Proportional(1.0);

            let idx = network.add_storage_node(
                self.meta.name.as_str(),
                Self::step_sub_name(i).as_deref(),
                initial_volume,
                None,
                None,
            )?;

            if let Some(prev_idx) = store_node_indices.last() {
                // There was a lower store; connect to it in both directions
                network.connect_nodes(idx, *prev_idx)?;
                network.connect_nodes(*prev_idx, idx)?;
            }

            store_node_indices.push(idx);
        }

        // Assume each store is full to start with
        let initial_volume = pywr_core::node::StorageInitialVolume::Proportional(1.0);

        // And one for the residual part above the less step
        let idx = network.add_storage_node(
            self.meta.name.as_str(),
            Self::step_sub_name(self.steps.len()).as_deref(),
            initial_volume,
            None,
            None,
        )?;

        if let Some(prev_idx) = store_node_indices.last() {
            // There was a lower store; connect to it in both directions
            network.connect_nodes(idx, *prev_idx)?;
            network.connect_nodes(*prev_idx, idx)?;
        }

        store_node_indices.push(idx);

        // Finally, add an aggregate storage node covering all the individual stores
        network.add_aggregated_storage_node(self.meta.name.as_str(), Self::agg_sub_name(), store_node_indices)?;

        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        // These are the min and max volume of the overall node
        let total_volume: SimpleMetricF64 = self.max_volume.load(network, args, Some(&self.meta.name))?.try_into()?;
        let total_min_volume: Option<SimpleMetricF64> = match &self.min_volume {
            Some(min_volume) => Some(min_volume.load(network, args, Some(&self.meta.name))?.try_into()?),
            None => None,
        };

        let mut prior_max_volumes: Vec<SimpleMetricF64> = Vec::new();

        for (i, step) in self.steps.iter().enumerate() {
            let sub_name = Self::step_sub_name(i);

            // The volume of this step is the proportion between the last control curve
            // (or zero if first) and this control curve.
            let lower = if i > 0 {
                Some(
                    self.steps[i - 1]
                        .control_curve
                        .load(network, args, Some(&self.meta.name))?
                        .try_into()?,
                )
            } else {
                None
            };

            let upper = step.control_curve.load(network, args, Some(&self.meta.name))?;

            let max_volume_parameter_idx =
                self.set_sub_node_max_volume(i, total_volume.clone(), Some(upper.try_into()?), lower, network)?;

            let prior_max_volume = pywr_core::parameters::AggregatedParameter::new(
                ParameterName::new(
                    format!("{}-prior-max-volume", Self::step_sub_name(i).unwrap()).as_str(),
                    Some(&self.meta.name),
                ),
                &prior_max_volumes,
                pywr_core::agg_funcs::AggFuncF64::Sum,
            );
            let prior_max_volume_idx = network.add_simple_parameter(Box::new(prior_max_volume))?;

            if let Some(total_min_volume) = total_min_volume.clone() {
                self.set_sub_node_min_volume(
                    i,
                    total_min_volume.clone(),
                    prior_max_volume_idx,
                    max_volume_parameter_idx,
                    network,
                )?;
            }

            self.set_sub_node_initial_volume(i, total_volume.clone(), prior_max_volume_idx, network)?;

            if let Some(cost) = &step.cost {
                let value = cost.load(network, args, Some(&self.meta.name))?;
                network.set_node_cost(self.meta.name.as_str(), sub_name.as_deref(), value.into())?;
            }

            // Append the max volume parameter of this node to the prior list
            prior_max_volumes.push(max_volume_parameter_idx.try_into()?);
        }

        // The volume of this store is the remain proportion above the last control curve
        let lower = match self.steps.last() {
            Some(step) => Some(
                step.control_curve
                    .load(network, args, Some(&self.meta.name))?
                    .try_into()?,
            ),
            None => None,
        };

        let upper = None;

        let max_volume_parameter_idx =
            self.set_sub_node_max_volume(self.steps.len(), total_volume.clone(), upper, lower, network)?;

        let prior_max_volume = pywr_core::parameters::AggregatedParameter::new(
            ParameterName::new(
                format!("{}-prior-max-volume", Self::step_sub_name(self.steps.len()).unwrap()).as_str(),
                Some(&self.meta.name),
            ),
            &prior_max_volumes,
            pywr_core::agg_funcs::AggFuncF64::Sum,
        );
        let prior_max_volume_idx = network.add_simple_parameter(Box::new(prior_max_volume))?;

        if let Some(total_min_volume) = total_min_volume.clone() {
            self.set_sub_node_min_volume(
                self.steps.len(),
                total_min_volume.clone(),
                prior_max_volume_idx,
                max_volume_parameter_idx,
                network,
            )?;
        }

        self.set_sub_node_initial_volume(self.steps.len(), total_volume.clone(), prior_max_volume_idx, network)?;

        // Set the cost for the last step
        if let Some(cost) = &self.cost {
            let value = cost.load(network, args, Some(&self.meta.name))?;
            network.set_node_cost(
                self.meta.name.as_str(),
                Self::step_sub_name(self.steps.len()).as_deref(),
                value.into(),
            )?;
        }

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
        total_volume: SimpleMetricF64,
        upper: Option<SimpleMetricF64>,
        lower: Option<SimpleMetricF64>,
        network: &mut pywr_core::network::Network,
    ) -> Result<ParameterIndex<f64>, SchemaError> {
        let sub_name = Self::step_sub_name(step_index);

        let max_volume_parameter = VolumeBetweenControlCurvesParameter::new(
            // Node's name is the parent identifier
            ParameterName::new(
                format!("{}-max-volume", Self::step_sub_name(step_index).unwrap()).as_str(),
                Some(&self.meta.name),
            ),
            total_volume.clone(),
            upper,
            lower,
        );
        let max_volume_parameter_idx = network.add_simple_parameter(Box::new(max_volume_parameter))?;
        let max_volume = Some(max_volume_parameter_idx.try_into()?);
        network.set_node_max_volume(self.meta.name.as_str(), sub_name.as_deref(), max_volume)?;

        Ok(max_volume_parameter_idx)
    }

    /// Set the minimum volume of the node at a specific step index.
    ///
    /// This method sets the minimum volume for a specific step in the piecewise storage node. It
    /// creates a `DifferenceParameter` that defines the minimum volume for that step, based on the
    /// total minimum volume and the maximum volume of the previous steps.
    fn set_sub_node_min_volume(
        &self,
        step_index: usize,
        total_min_volume: SimpleMetricF64,
        prior_max_volume_idx: ParameterIndex<f64>,
        max_volume_parameter_idx: ParameterIndex<f64>,
        network: &mut pywr_core::network::Network,
    ) -> Result<(), SchemaError> {
        let sub_name = Self::step_sub_name(step_index);
        // The minimum volume is the difference between the total volume and
        // the maximum volume of the previous steps, but limited to be between zero and the maximum volume of this step.
        let min_volume_parameter = DifferenceParameter::new(
            ParameterName::new(
                format!("{}-min-volume", Self::step_sub_name(step_index).unwrap()).as_str(),
                Some(&self.meta.name),
            ),
            total_min_volume,
            prior_max_volume_idx.try_into()?,
            Some(0.0.into()),
            Some(max_volume_parameter_idx.try_into()?),
        );
        let min_volume_parameter_idx = network.add_simple_parameter(Box::new(min_volume_parameter))?;
        network.set_node_min_volume(
            self.meta.name.as_str(),
            sub_name.as_deref(),
            Some(min_volume_parameter_idx.try_into()?),
        )?;

        Ok(())
    }

    /// Set the initial volume of the node at a specific step index.
    fn set_sub_node_initial_volume(
        &self,
        step_index: usize,
        total_volume: SimpleMetricF64,
        prior_max_volume_idx: ParameterIndex<f64>,
        network: &mut pywr_core::network::Network,
    ) -> Result<(), SchemaError> {
        let sub_name = Self::step_sub_name(step_index);
        // Set the initial volume of this step
        let initial_volume = match &self.initial_volume {
            StorageInitialVolume::Proportional { proportion } => {
                pywr_core::node::StorageInitialVolume::DistributedProportional {
                    total_volume: total_volume.clone(),
                    proportion: *proportion,
                    prior_max_volume: prior_max_volume_idx.try_into()?,
                }
            }
            StorageInitialVolume::Absolute { volume } => pywr_core::node::StorageInitialVolume::DistributedAbsolute {
                absolute: *volume,
                prior_max_volume: prior_max_volume_idx.try_into()?,
            },
        };
        network.set_node_initial_volume(self.meta.name.as_str(), sub_name.as_deref(), initial_volume)?;

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
            .get_aggregated_storage_node_index_by_name(self.meta.name.as_str(), Self::agg_sub_name())
            .ok_or_else(|| SchemaError::CoreNodeNotFound {
                name: self.meta.name.clone(),
                sub_name: Self::agg_sub_name().map(String::from),
            })?;

        let metric = match attr {
            PiecewiseStorageNodeAttribute::Volume => MetricF64::AggregatedNodeVolume(idx),
            PiecewiseStorageNodeAttribute::ProportionalVolume => {
                let dm = DerivedMetric::AggregatedNodeProportionalVolume(idx);
                let derived_metric_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(derived_metric_idx)
            }
        };

        Ok(metric)
    }
}
