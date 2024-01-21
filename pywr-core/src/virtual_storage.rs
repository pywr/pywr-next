use crate::network::Network;
use crate::node::{ConstraintValue, FlowConstraints, NodeMeta, StorageConstraints, StorageInitialVolume};
use crate::state::{State, VirtualStorageState};
use crate::timestep::Timestep;
use crate::{NodeIndex, PywrError};
use std::ops::{Deref, DerefMut};
use time::{Date, Month};

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct VirtualStorageIndex(usize);

impl Deref for VirtualStorageIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Default)]
pub struct VirtualStorageVec {
    nodes: Vec<VirtualStorage>,
}

impl Deref for VirtualStorageVec {
    type Target = Vec<VirtualStorage>;

    fn deref(&self) -> &Self::Target {
        &self.nodes
    }
}

impl DerefMut for VirtualStorageVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.nodes
    }
}

impl VirtualStorageVec {
    pub fn get(&self, index: &VirtualStorageIndex) -> Result<&VirtualStorage, PywrError> {
        self.nodes.get(index.0).ok_or(PywrError::NodeIndexNotFound)
    }

    pub fn get_mut(&mut self, index: &VirtualStorageIndex) -> Result<&mut VirtualStorage, PywrError> {
        self.nodes.get_mut(index.0).ok_or(PywrError::NodeIndexNotFound)
    }

    pub fn push_new(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        nodes: &[NodeIndex],
        factors: Option<&[f64]>,
        initial_volume: StorageInitialVolume,
        min_volume: ConstraintValue,
        max_volume: ConstraintValue,
        reset: VirtualStorageReset,
        cost: ConstraintValue,
    ) -> VirtualStorageIndex {
        let node_index = VirtualStorageIndex(self.nodes.len());
        let node = VirtualStorage::new(
            &node_index,
            name,
            sub_name,
            nodes,
            factors,
            initial_volume,
            min_volume,
            max_volume,
            reset,
            cost,
        );
        self.nodes.push(node);
        node_index
    }
}

pub enum VirtualStorageReset {
    Never,
    DayOfYear { day: u8, month: Month },
    NumberOfMonths { months: i32 },
}

// #[derive(Debug)]
pub struct VirtualStorage {
    pub meta: NodeMeta<VirtualStorageIndex>,
    pub flow_constraints: FlowConstraints,
    pub nodes: Vec<NodeIndex>,
    pub factors: Option<Vec<f64>>,
    pub initial_volume: StorageInitialVolume,
    pub storage_constraints: StorageConstraints,
    pub reset: VirtualStorageReset,
    pub cost: ConstraintValue,
}

impl VirtualStorage {
    pub fn new(
        index: &VirtualStorageIndex,
        name: &str,
        sub_name: Option<&str>,
        nodes: &[NodeIndex],
        factors: Option<&[f64]>,
        initial_volume: StorageInitialVolume,
        min_volume: ConstraintValue,
        max_volume: ConstraintValue,
        reset: VirtualStorageReset,
        cost: ConstraintValue,
    ) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            flow_constraints: FlowConstraints::new(),
            nodes: nodes.to_vec(),
            factors: factors.map(|f| f.to_vec()),
            initial_volume,
            storage_constraints: StorageConstraints::new(min_volume, max_volume),
            reset,
            cost,
        }
    }

    pub fn name(&self) -> &str {
        self.meta.name()
    }

    /// Get a node's sub_name
    pub fn sub_name(&self) -> Option<&str> {
        self.meta.sub_name()
    }

    /// Get a node's full name
    pub fn full_name(&self) -> (&str, Option<&str>) {
        self.meta.full_name()
    }

    pub fn index(&self) -> VirtualStorageIndex {
        *self.meta.index()
    }

    pub fn has_factors(&self) -> bool {
        self.factors.is_some()
    }

    pub fn default_state(&self) -> VirtualStorageState {
        VirtualStorageState::new(0.0)
    }

    pub fn get_cost(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        match &self.cost {
            ConstraintValue::None => Ok(0.0),
            ConstraintValue::Scalar(v) => Ok(*v),
            ConstraintValue::Metric(m) => m.get_value(network, state),
        }
    }

    pub fn before(&self, timestep: &Timestep, network: &Network, state: &mut State) -> Result<(), PywrError> {
        let do_reset = if timestep.is_first() {
            // Set the initial volume if it is the first timestep.
            true
        } else {
            // Otherwise we check the reset condition
            match self.reset {
                VirtualStorageReset::Never => false,
                VirtualStorageReset::DayOfYear { day, month } => {
                    (timestep.date.day() == day) && (timestep.date.month() == month)
                }
                VirtualStorageReset::NumberOfMonths { months } => {
                    // Get the date when the virtual storage was last reset
                    match state
                        .get_network_state()
                        .get_virtual_storage_last_reset(&self.index())?
                    {
                        // Reset if last reset is more than `months` ago.
                        Some(last_reset) => months_since_last_reset(&timestep.date, &last_reset.date) >= months,
                        None => true,
                    }
                }
            }
        };

        if do_reset {
            let volume = match &self.initial_volume {
                StorageInitialVolume::Absolute(iv) => *iv,
                StorageInitialVolume::Proportional(ipc) => {
                    let max_volume = self.get_max_volume(network, state)?;
                    max_volume * ipc
                }
            };

            state.reset_virtual_storage_node_volume(*self.meta.index(), volume, timestep)?;
        }

        Ok(())
    }

    pub fn get_nodes(&self) -> Vec<NodeIndex> {
        self.nodes.to_vec()
    }

    pub fn get_nodes_with_factors(&self) -> Option<Vec<(NodeIndex, f64)>> {
        self.factors
            .as_ref()
            .map(|factors| self.nodes.iter().zip(factors.iter()).map(|(n, f)| (*n, *f)).collect())
    }

    pub fn get_min_volume(&self, model: &Network, state: &State) -> Result<f64, PywrError> {
        self.storage_constraints.get_min_volume(model, state)
    }

    pub fn get_max_volume(&self, model: &Network, state: &State) -> Result<f64, PywrError> {
        self.storage_constraints.get_max_volume(model, state)
    }

    pub fn get_current_available_volume_bounds(&self, model: &Network, state: &State) -> Result<(f64, f64), PywrError> {
        let min_vol = self.get_min_volume(model, state)?;
        let max_vol = self.get_max_volume(model, state)?;

        let current_volume = state.get_network_state().get_virtual_storage_volume(&self.index())?;

        let available = (current_volume - min_vol).max(0.0);
        let missing = (max_vol - current_volume).max(0.0);
        Ok((available, missing))
    }
}

/// Calculate the number of months between `current` [Timestep] and the `last_reset` [Timestep].
fn months_since_last_reset(current: &Date, last_reset: &Date) -> i32 {
    (current.year() - last_reset.year()) * 12 + current.month() as i32 - last_reset.month() as i32
}

#[cfg(test)]
mod tests {
    use crate::metric::Metric;
    use crate::models::Model;
    use crate::network::Network;
    use crate::node::{ConstraintValue, StorageInitialVolume};
    use crate::recorders::{AssertionFnRecorder, AssertionRecorder};
    use crate::scenario::ScenarioIndex;
    use crate::test_utils::{default_timestepper, run_all_solvers, simple_model};
    use crate::timestep::Timestep;
    use crate::virtual_storage::{months_since_last_reset, VirtualStorageReset};
    use ndarray::Array;
    use time::macros::date;

    /// Test the calculation of number of months since last reset
    #[test]
    fn test_months_since_last_reset() {
        assert_eq!(
            months_since_last_reset(&date!(2022 - 12 - 31), &date!(2022 - 12 - 31)),
            0
        );
        assert_eq!(
            months_since_last_reset(&date!(2023 - 12 - 31), &date!(2022 - 12 - 31)),
            12
        );
        assert_eq!(
            months_since_last_reset(&date!(2023 - 01 - 1), &date!(2022 - 12 - 31)),
            1
        );
        assert_eq!(
            months_since_last_reset(&date!(2022 - 12 - 1), &date!(2022 - 12 - 31)),
            0
        );
    }

    /// Test the virtual storage constraints
    #[test]
    fn test_basic_virtual_storage() {
        let mut network = Network::default();

        let input_node = network.add_input_node("input", None).unwrap();
        let link_node0 = network.add_link_node("link", Some("0")).unwrap();
        let output_node0 = network.add_output_node("output", Some("0")).unwrap();

        network.connect_nodes(input_node, link_node0).unwrap();
        network.connect_nodes(link_node0, output_node0).unwrap();

        let link_node1 = network.add_link_node("link", Some("1")).unwrap();
        let output_node1 = network.add_output_node("output", Some("1")).unwrap();

        network.connect_nodes(input_node, link_node1).unwrap();
        network.connect_nodes(link_node1, output_node1).unwrap();

        // Virtual storage with contributions from link-node0 than link-node1
        let _vs = network.add_virtual_storage_node(
            "virtual-storage",
            None,
            &[link_node0, link_node1],
            Some(&[2.0, 1.0]),
            StorageInitialVolume::Absolute(100.0),
            ConstraintValue::Scalar(0.0),
            ConstraintValue::Scalar(100.0),
            VirtualStorageReset::Never,
            ConstraintValue::Scalar(0.0),
        );

        // Setup a demand on output-0 and output-1
        for sub_name in &["0", "1"] {
            let output_node = network.get_mut_node_by_name("output", Some(sub_name)).unwrap();
            output_node
                .set_max_flow_constraint(ConstraintValue::Scalar(10.0))
                .unwrap();
            output_node.set_cost(ConstraintValue::Scalar(-10.0));
        }

        // With a demand of 10 on each link node. The virtual storage will depleted at a rate of
        // 30 per day.
        // TODO assert let expected_vol = |ts: &Timestep, _si| (70.0 - ts.index as f64 * 30.0).max(0.0);
        // Set-up assertion for "link" node
        let idx = network.get_node_by_name("link", Some("0")).unwrap().index();
        let expected = |ts: &Timestep, _si: &ScenarioIndex| {
            if ts.index < 3 {
                10.0
            } else {
                0.0
            }
        };
        let recorder = AssertionFnRecorder::new("link-0-flow", Metric::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Set-up assertion for "input" node
        let idx = network.get_node_by_name("link", Some("1")).unwrap().index();
        let expected = |ts: &Timestep, _si: &ScenarioIndex| {
            if ts.index < 4 {
                10.0
            } else {
                0.0
            }
        };
        let recorder = AssertionFnRecorder::new("link-1-flow", Metric::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        let model = Model::new(default_timestepper().into(), network);
        // Test all solvers
        run_all_solvers(&model);
    }

    #[test]
    /// Test virtual storage node costs
    fn test_virtual_storage_node_costs() {
        let mut model = simple_model(1);
        let network = model.network_mut();
        let _timestepper = default_timestepper();

        let nodes = vec![network.get_node_index_by_name("input", None).unwrap()];
        // Virtual storage node cost is high enough to prevent any flow
        network
            .add_virtual_storage_node(
                "vs",
                None,
                &nodes,
                None,
                StorageInitialVolume::Proportional(1.0),
                ConstraintValue::Scalar(0.0),
                ConstraintValue::Scalar(100.0),
                VirtualStorageReset::Never,
                ConstraintValue::Scalar(20.0),
            )
            .unwrap();

        let expected = Array::zeros((366, 1));
        let idx = network.get_node_by_name("output", None).unwrap().index();
        let recorder = AssertionRecorder::new("output-flow", Metric::NodeInFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model);
    }
}
