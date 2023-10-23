use crate::data_tables::LoadedTableCollection;
use crate::error::SchemaError;
use crate::nodes::NodeMeta;
use crate::parameters::DynamicFloatValue;
use pywr_core::metric::{Metric, VolumeBetweenControlCurves};
use pywr_core::models::ModelDomain;
use pywr_core::node::{ConstraintValue, StorageInitialVolume};
use std::path::Path;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct PiecewiseStore {
    pub control_curve: DynamicFloatValue,
    pub cost: Option<DynamicFloatValue>,
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
#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct PiecewiseStorageNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub max_volume: DynamicFloatValue,
    // TODO implement min volume
    // pub min_volume: Option<DynamicFloatValue>,
    pub steps: Vec<PiecewiseStore>,
    // TODO implement initial volume
    // pub initial_volume: Option<f64>,
    // pub initial_volume_pc: Option<f64>,
}

impl PiecewiseStorageNode {
    fn step_sub_name(i: usize) -> Option<String> {
        Some(format!("store-{i:02}"))
    }

    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<(), SchemaError> {
        // These are the min and max volume of the overall node
        let max_volume = self.max_volume.load(network, domain, tables, data_path)?;

        let mut store_node_indices = Vec::new();

        // create a storage node for each step
        for (i, step) in self.steps.iter().enumerate() {
            // The volume of this step is the proportion between the last control curve
            // (or zero if first) and this control curve.
            let lower = if i > 0 {
                Some(
                    self.steps[i - 1]
                        .control_curve
                        .load(network, domain, tables, data_path)?,
                )
            } else {
                None
            };

            let upper = step.control_curve.load(network, domain, tables, data_path)?;

            let max_volume = ConstraintValue::Metric(Metric::VolumeBetweenControlCurves(
                VolumeBetweenControlCurves::new(max_volume.clone(), Some(upper), lower),
            ));

            // Each store has min volume of zero
            let min_volume = ConstraintValue::Scalar(0.0);
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
            Some(step) => Some(step.control_curve.load(network, domain, tables, data_path)?),
            None => None,
        };

        let upper = None;

        let max_volume = ConstraintValue::Metric(Metric::VolumeBetweenControlCurves(VolumeBetweenControlCurves::new(
            max_volume.clone(),
            upper,
            lower,
        )));

        // Each store has min volume of zero
        let min_volume = ConstraintValue::Scalar(0.0);
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
        network.add_aggregated_storage_node(self.meta.name.as_str(), None, store_node_indices)?;

        Ok(())
    }

    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        domain: &ModelDomain,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<(), SchemaError> {
        for (i, step) in self.steps.iter().enumerate() {
            let sub_name = Self::step_sub_name(i);

            if let Some(cost) = &step.cost {
                let value = cost.load(network, domain, tables, data_path)?;
                network.set_node_cost(self.meta.name.as_str(), sub_name.as_deref(), value.into())?;
            }
        }

        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), Self::step_sub_name(self.steps.len()))]
    }
    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        vec![(self.meta.name.as_str(), Self::step_sub_name(self.steps.len()))]
    }

    pub fn default_metric(&self, network: &pywr_core::network::Network) -> Result<Metric, SchemaError> {
        let idx = network.get_aggregated_storage_node_index_by_name(self.meta.name.as_str(), None)?;
        Ok(Metric::AggregatedNodeVolume(idx))
    }
}

#[cfg(test)]
mod tests {
    use crate::model::PywrModel;
    use ndarray::Array2;
    use pywr_core::metric::Metric;
    use pywr_core::recorders::AssertionRecorder;
    use pywr_core::test_utils::run_all_solvers;
    use pywr_core::timestep::Timestepper;

    fn piecewise_storage1_str() -> &'static str {
        include_str!("../test_models/piecewise_storage1.json")
    }

    fn piecewise_storage2_str() -> &'static str {
        include_str!("../test_models/piecewise_storage2.json")
    }

    /// Test running `piecewise_storage1.json`
    #[test]
    fn test_piecewise_storage1() {
        let data = piecewise_storage1_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let mut model = schema.build_model(None, None).unwrap();

        let network = model.network_mut();
        assert_eq!(network.nodes().len(), 5);
        assert_eq!(network.edges().len(), 6);

        // TODO put this assertion data in the test model file.
        let idx = network
            .get_aggregated_storage_node_index_by_name("storage1", None)
            .unwrap();

        let expected = Array2::from_shape_fn((366, 1), |(i, _)| {
            if i < 33 {
                // Draw-down top store at 15 until it is emptied
                985.0 - i as f64 * 15.0
            } else if i < 58 {
                // The second store activates the input (via costs) such that the net draw down is now 10
                495.0 - (i as f64 - 33.0) * 10.0
            } else {
                // Finally the abstraction stops to maintain the bottom store at 250
                250.0
            }
        });

        let recorder = AssertionRecorder::new(
            "storage1-volume",
            Metric::AggregatedNodeVolume(idx),
            expected,
            None,
            None,
        );
        network.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model);
    }

    /// Test running `piecewise_storage2.json`
    #[test]
    fn test_piecewise_storage2() {
        let data = piecewise_storage2_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let mut model = schema.build_model(None, None).unwrap();

        let network = model.network_mut();
        assert_eq!(network.nodes().len(), 5);
        assert_eq!(network.edges().len(), 6);

        // TODO put this assertion data in the test model file.
        let idx = network
            .get_aggregated_storage_node_index_by_name("storage1", None)
            .unwrap();

        let expected = Array2::from_shape_fn((366, 1), |(i, _)| {
            if i < 49 {
                // Draw-down top store at 5 until it is emptied (control curve starts at 75%)
                995.0 - i as f64 * 5.0
            } else if i < 89 {
                // The second store activates the input (via costs) such that the net draw down is now 2
                750.0 - (i as f64 - 49.0) * 2.0
            } else if i < 123 {
                // The control curve lowers such that the first store re-activates and therefore
                // disables the input (via costs). The net draw down returns to 5
                670.0 - (i as f64 - 89.0) * 5.0
            } else if i < 180 {
                // The second store re-activates the input (via costs) such that the net draw down is now 2
                500.0 - (i as f64 - 123.0) * 2.0
            } else if i < 198 {
                // The control curve lowers (again) such that the first store re-activates and therefore
                // disables the input (via costs). The net draw down returns to 5
                386.0 - (i as f64 - 180.0) * 5.0
            } else if i < 223 {
                // The second store re-activates the input (via costs) such that the net draw down is now 2
                299.0 - (i as f64 - 198.0) * 2.0
            } else {
                // Finally the abstraction stops to maintain the bottom store at 250
                250.0
            }
        });

        let recorder = AssertionRecorder::new(
            "storage1-volume",
            Metric::AggregatedNodeVolume(idx),
            expected,
            None,
            None,
        );
        network.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model);
    }
}
