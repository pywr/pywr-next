use crate::model::Model;
use crate::node::{ConstraintValue, FlowConstraints, NodeMeta, StorageConstraints, StorageInitialVolume};
use crate::state::{State, StorageState};
use crate::timestep::Timestep;
use crate::{NodeIndex, PywrError};
use std::ops::{Deref, DerefMut};
use time::Month;

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
        min_volume: f64,
        max_volume: ConstraintValue,
        reset: VirtualStorageReset,
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
        );
        self.nodes.push(node);
        node_index
    }
}

pub enum VirtualStorageReset {
    Never,
    DayOfYear { day: u8, month: Month },
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
}

impl VirtualStorage {
    pub fn new(
        index: &VirtualStorageIndex,
        name: &str,
        sub_name: Option<&str>,
        nodes: &[NodeIndex],
        factors: Option<&[f64]>,
        initial_volume: StorageInitialVolume,
        min_volume: f64,
        max_volume: ConstraintValue,
        reset: VirtualStorageReset,
    ) -> Self {
        Self {
            meta: NodeMeta::new(index, name, sub_name),
            flow_constraints: FlowConstraints::new(),
            nodes: nodes.to_vec(),
            factors: factors.map(|f| f.to_vec()),
            initial_volume,
            storage_constraints: StorageConstraints::new(min_volume, max_volume),
            reset,
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

    pub fn default_state(&self) -> StorageState {
        StorageState::new(0.0)
    }

    pub fn before(&self, timestep: &Timestep, model: &Model, state: &mut State) -> Result<(), PywrError> {
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
            }
        };

        if do_reset {
            let volume = match &self.initial_volume {
                StorageInitialVolume::Absolute(iv) => *iv,
                StorageInitialVolume::Proportional(ipc) => {
                    let max_volume = self.get_max_volume(model, state)?;
                    max_volume * ipc
                }
            };

            state.set_virtual_storage_node_volume(*self.meta.index(), volume)?;
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

    pub fn get_min_volume(&self) -> f64 {
        self.storage_constraints.get_min_volume()
    }

    pub fn get_max_volume(&self, model: &Model, state: &State) -> Result<f64, PywrError> {
        self.storage_constraints.get_max_volume(model, state)
    }

    pub fn get_current_available_volume_bounds(&self, model: &Model, state: &State) -> Result<(f64, f64), PywrError> {
        let min_vol = self.get_min_volume();
        let max_vol = self.get_max_volume(model, state)?;

        let current_volume = state.get_network_state().get_virtual_storage_volume(&self.index())?;

        let available = (current_volume - min_vol).max(0.0);
        let missing = (max_vol - current_volume).max(0.0);
        Ok((available, missing))
    }
}

#[cfg(test)]
mod tests {
    use crate::metric::Metric;
    use crate::model::Model;
    use crate::node::{ConstraintValue, StorageInitialVolume};
    use crate::recorders::AssertionFnRecorder;
    use crate::scenario::ScenarioIndex;
    use crate::solvers::ClpSolver;
    use crate::test_utils::{default_scenarios, default_timestepper};
    use crate::timestep::Timestep;
    use crate::virtual_storage::VirtualStorageReset;

    /// Test the virtual storage constraints
    #[test]
    fn test_basic_virtual_storage() {
        let mut model = Model::default();
        let timestepper = default_timestepper();
        let scenarios = default_scenarios();

        let input_node = model.add_input_node("input", None).unwrap();
        let link_node0 = model.add_link_node("link", Some("0")).unwrap();
        let output_node0 = model.add_output_node("output", Some("0")).unwrap();

        model.connect_nodes(input_node, link_node0).unwrap();
        model.connect_nodes(link_node0, output_node0).unwrap();

        let link_node1 = model.add_link_node("link", Some("1")).unwrap();
        let output_node1 = model.add_output_node("output", Some("1")).unwrap();

        model.connect_nodes(input_node, link_node1).unwrap();
        model.connect_nodes(link_node1, output_node1).unwrap();

        // Virtual storage with contributions from link-node0 than link-node1

        let _vs = model.add_virtual_storage_node(
            "virtual-storage",
            None,
            &[link_node0, link_node1],
            Some(&[2.0, 1.0]),
            StorageInitialVolume::Absolute(100.0),
            0.0,
            ConstraintValue::Scalar(100.0),
            VirtualStorageReset::Never,
        );

        // Setup a demand on output-0 and output-1
        for sub_name in &["0", "1"] {
            let output_node = model.get_mut_node_by_name("output", Some(sub_name)).unwrap();
            output_node
                .set_max_flow_constraint(ConstraintValue::Scalar(10.0))
                .unwrap();
            output_node.set_cost(ConstraintValue::Scalar(-10.0));
        }

        // With a demand of 10 on each link node. The virtual storage will depleted at a rate of
        // 30 per day.
        // TODO assert let expected_vol = |ts: &Timestep, _si| (70.0 - ts.index as f64 * 30.0).max(0.0);
        // Set-up assertion for "link" node
        let idx = model.get_node_by_name("link", Some("0")).unwrap().index();
        let expected = |ts: &Timestep, _si: &ScenarioIndex| {
            if ts.index < 3 {
                10.0
            } else {
                0.0
            }
        };
        let recorder = AssertionFnRecorder::new("link-0-flow", Metric::NodeOutFlow(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        // Set-up assertion for "input" node
        let idx = model.get_node_by_name("link", Some("1")).unwrap().index();
        let expected = |ts: &Timestep, _si: &ScenarioIndex| {
            if ts.index < 4 {
                10.0
            } else {
                0.0
            }
        };
        let recorder = AssertionFnRecorder::new("link-1-flow", Metric::NodeOutFlow(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        model.run::<ClpSolver>(&timestepper, &scenarios).unwrap();
    }
}
