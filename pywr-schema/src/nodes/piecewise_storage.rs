#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::{NodeAttribute, NodeMeta};
#[cfg(feature = "core")]
use pywr_core::{
    derived_metric::DerivedMetric,
    metric::{MetricF64, SimpleMetricF64},
    node::StorageInitialVolume,
    parameters::{ParameterName, VolumeBetweenControlCurvesParameter},
};
use pywr_schema_macros::PywrVisitAll;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct PiecewiseStore {
    pub control_curve: Metric,
    pub cost: Option<Metric>,
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
)]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct PiecewiseStorageNode {
    pub meta: NodeMeta,
    pub max_volume: Metric,
    // TODO implement min volume
    // pub min_volume: Option<DynamicFloatValue>,
    pub steps: Vec<PiecewiseStore>,
    // TODO implement initial volume
    // pub initial_volume: Option<f64>,
    // pub initial_volume_pc: Option<f64>,
}

impl PiecewiseStorageNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Volume;

    fn step_sub_name(i: usize) -> Option<String> {
        Some(format!("store-{i:02}"))
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), Self::step_sub_name(self.steps.len()))]
    }
    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), Self::step_sub_name(self.steps.len()))]
    }

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl PiecewiseStorageNode {
    fn agg_sub_name() -> Option<&'static str> {
        Some("agg-store")
    }

    pub fn node_indices_for_constraints(
        &self,
        network: &pywr_core::network::Network,
    ) -> Result<Vec<pywr_core::node::NodeIndex>, SchemaError> {
        let indices = self
            .steps
            .iter()
            .enumerate()
            .map(|(i, _)| network.get_node_index_by_name(self.meta.name.as_str(), Self::step_sub_name(i).as_deref()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(indices)
    }

    pub fn add_to_model(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<(), SchemaError> {
        // These are the min and max volume of the overall node
        let max_volume: SimpleMetricF64 = self.max_volume.load(network, args)?.try_into()?;

        let mut store_node_indices = Vec::new();

        // create a storage node for each step
        for (i, step) in self.steps.iter().enumerate() {
            // The volume of this step is the proportion between the last control curve
            // (or zero if first) and this control curve.
            let lower = if i > 0 {
                Some(self.steps[i - 1].control_curve.load(network, args)?.try_into()?)
            } else {
                None
            };

            let upper = step.control_curve.load(network, args)?;

            let max_volume_parameter = VolumeBetweenControlCurvesParameter::new(
                // Node's name is the parent identifier
                ParameterName::new(
                    format!("{}-max-volume", Self::step_sub_name(i).unwrap()).as_str(),
                    Some(&self.meta.name),
                ),
                max_volume.clone(),
                Some(upper.try_into()?),
                lower,
            );
            let max_volume_parameter_idx = network.add_simple_parameter(Box::new(max_volume_parameter))?;
            let max_volume = Some(max_volume_parameter_idx.try_into()?);

            // Each store has min volume of zero
            let min_volume = None;
            // Assume each store is full to start with
            let initial_volume = StorageInitialVolume::Proportional(1.0);

            let idx = network.add_storage_node(
                self.meta.name.as_str(),
                Self::step_sub_name(i).as_deref(),
                initial_volume,
                min_volume,
                max_volume,
            )?;

            if let Some(prev_idx) = store_node_indices.last() {
                // There was a lower store; connect to it in both directions
                network.connect_nodes(idx, *prev_idx)?;
                network.connect_nodes(*prev_idx, idx)?;
            }

            store_node_indices.push(idx);
        }

        // The volume of this store the remain proportion above the last control curve
        let lower = match self.steps.last() {
            Some(step) => Some(step.control_curve.load(network, args)?.try_into()?),
            None => None,
        };

        let upper = None;

        let max_volume_parameter = VolumeBetweenControlCurvesParameter::new(
            ParameterName::new(
                format!("{}-max-volume", Self::step_sub_name(self.steps.len()).unwrap()).as_str(),
                Some(&self.meta.name),
            ),
            max_volume.clone(),
            upper,
            lower,
        );
        let max_volume_parameter_idx = network.add_simple_parameter(Box::new(max_volume_parameter))?;
        let max_volume = Some(max_volume_parameter_idx.try_into()?);

        // Each store has min volume of zero
        let min_volume = None;
        // Assume each store is full to start with
        let initial_volume = StorageInitialVolume::Proportional(1.0);

        // And one for the residual part above the less step
        let idx = network.add_storage_node(
            self.meta.name.as_str(),
            Self::step_sub_name(self.steps.len()).as_deref(),
            initial_volume,
            min_volume,
            max_volume,
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
        for (i, step) in self.steps.iter().enumerate() {
            let sub_name = Self::step_sub_name(i);

            if let Some(cost) = &step.cost {
                let value = cost.load(network, args)?;
                network.set_node_cost(self.meta.name.as_str(), sub_name.as_deref(), value.into())?;
            }
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

        let idx = network.get_aggregated_storage_node_index_by_name(self.meta.name.as_str(), Self::agg_sub_name())?;

        let metric = match attr {
            NodeAttribute::Volume => MetricF64::AggregatedNodeVolume(idx),
            NodeAttribute::ProportionalVolume => {
                let dm = DerivedMetric::AggregatedNodeProportionalVolume(idx);
                let derived_metric_idx = network.add_derived_metric(dm);
                MetricF64::DerivedMetric(derived_metric_idx)
            }
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "PiecewiseStorageNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
    }
}
