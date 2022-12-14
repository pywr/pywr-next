use crate::edge::{EdgeIndex, EdgeVec};
use crate::node::{ConstraintValue, Node, NodeVec, StorageInitialVolume};

use crate::scenario::{ScenarioGroupCollection, ScenarioIndex};
use crate::solvers::{Solver, SolverTimings};
use crate::state::{EdgeState, NetworkState, ParameterState};
use crate::timestep::{Timestep, Timestepper};

use crate::aggregated_node::{AggregatedNode, AggregatedNodeIndex, AggregatedNodeVec};
use std::ops::Deref;

use crate::{parameters, recorders, IndexParameterIndex, NodeIndex, ParameterIndex, PywrError, RecorderIndex};

use crate::parameters::ParameterType;
use crate::virtual_storage::{VirtualStorage, VirtualStorageIndex, VirtualStorageVec};

use crate::aggregated_storage_node::{AggregatedStorageNode, AggregatedStorageNodeIndex, AggregatedStorageNodeVec};
use crate::metric::Metric;
use indicatif::ProgressIterator;
use log::debug;
use std::time::Duration;
use std::time::Instant;

#[derive(Default)]
pub struct RunTimings {
    parameter_calculation: Duration,
    recorder_saving: Duration,
    solve: SolverTimings,
}

#[derive(Default)]
pub struct Model {
    pub nodes: NodeVec,
    pub edges: EdgeVec,
    pub aggregated_nodes: AggregatedNodeVec,
    pub aggregated_storage_nodes: AggregatedStorageNodeVec,
    pub virtual_storage_nodes: VirtualStorageVec,
    parameters: Vec<Box<dyn parameters::Parameter>>,
    index_parameters: Vec<Box<dyn parameters::IndexParameter>>,
    parameters_resolve_order: Vec<ParameterType>,
    recorders: Vec<Box<dyn recorders::Recorder>>,
}

// Required for Python API
unsafe impl Send for Model {}

impl Model {
    /// Returns the initial state of the network
    pub(crate) fn get_initial_state(&self, scenario_indices: &[ScenarioIndex]) -> Vec<NetworkState> {
        let mut states: Vec<NetworkState> = Vec::new();

        for _scenario_index in scenario_indices {
            let mut state = NetworkState::new();

            for node in self.nodes.deref() {
                state.push_node_state(node.new_state());
            }

            for _edge in self.edges.deref() {
                state.push_edge_state(EdgeState::default());
            }

            states.push(state)
        }
        states
    }

    fn setup(&mut self, timesteps: &[Timestep], scenario_indices: &[ScenarioIndex]) -> Result<(), PywrError> {
        // Setup parameters

        for parameter in self.parameters.iter_mut() {
            parameter.setup(timesteps, scenario_indices)?;
        }

        for parameter in self.index_parameters.iter_mut() {
            parameter.setup(timesteps, scenario_indices)?;
        }

        // Setup recorders
        for recorder in self.recorders.iter_mut() {
            recorder.setup(timesteps, scenario_indices)?;
        }

        Ok(())
    }

    fn finalise(&mut self) -> Result<(), PywrError> {
        // Setup recorders
        for recorder in self.recorders.iter_mut() {
            recorder.finalise()?;
        }

        Ok(())
    }

    pub fn run(
        &mut self,
        timestepper: Timestepper,
        scenarios: ScenarioGroupCollection,
        solver: &mut Box<dyn Solver>,
    ) -> Result<(), PywrError> {
        let now = Instant::now();

        let mut timings = RunTimings::default();

        let timesteps = timestepper.timesteps();
        let scenario_indices = scenarios.scenario_indices();
        // One state per scenario
        let mut current_states = self.get_initial_state(&scenario_indices);

        // Setup the solver
        let mut count = 0;
        solver.setup(self)?;
        self.setup(&timesteps, &scenario_indices)?;

        // Step a timestep
        for timestep in timesteps.iter().progress() {
            debug!("Starting timestep {:?}", timestep);
            let next_states = self.step(timestep, &scenario_indices, solver, &current_states, &mut timings)?;
            current_states = next_states;
            count += scenario_indices.len();
        }

        let total_duration = now.elapsed().as_secs_f64();
        println!("total run time: {}s", total_duration);
        println!(
            "total parameter calculation time: {}s",
            timings.parameter_calculation.as_secs_f64()
        );
        println!("total recorder save time: {}s", timings.recorder_saving.as_secs_f64());
        println!(
            "total update objective time: {}s",
            timings.solve.update_objective.as_secs_f64()
        );
        println!(
            "total update constraints time: {}s",
            timings.solve.update_constraints.as_secs_f64()
        );
        println!("total LP solve time: {}s", timings.solve.solve.as_secs_f64());
        println!(
            "total save solution time: {}s",
            timings.solve.save_solution.as_secs_f64()
        );
        println!("speed: {} ts/s", count as f64 / total_duration);
        self.finalise()?;
        Ok(())
    }

    /// Perform a single timestep with the current state, and return the updated states.
    pub(crate) fn step(
        &mut self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        solver: &mut Box<dyn Solver>,
        current_states: &[NetworkState],
        timings: &mut RunTimings,
    ) -> Result<Vec<NetworkState>, PywrError> {
        let mut next_states = Vec::with_capacity(current_states.len());

        for scenario_index in scenario_indices.iter() {
            let current_state = match current_states.get(scenario_index.index) {
                Some(s) => s,
                None => return Err(PywrError::ScenarioStateNotFound),
            };
            let start_p_calc = Instant::now();
            let pstate = self.compute_parameters(timestep, scenario_index, current_state)?;
            timings.parameter_calculation += start_p_calc.elapsed();

            let (next_state, solve_timings) = solver.solve(self, timestep, current_state, &pstate)?;
            timings.solve += solve_timings;

            let start_r_save = Instant::now();
            self.save_recorders(timestep, scenario_index, &next_state, &pstate)?;
            timings.recorder_saving += start_r_save.elapsed();
            next_states.push(next_state);
        }

        let start_r_save = Instant::now();
        self.after_save_recorders(timestep)?;
        timings.recorder_saving += start_r_save.elapsed();

        Ok(next_states)
    }

    fn compute_parameters(
        &mut self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        state: &NetworkState,
    ) -> Result<ParameterState, PywrError> {
        let mut parameter_state = ParameterState::with_capacity(self.parameters.len(), 0);

        for p_type in &self.parameters_resolve_order {
            match p_type {
                ParameterType::Parameter(idx) => {
                    let p = self
                        .parameters
                        .get_mut(*idx.deref())
                        .ok_or(PywrError::ParameterIndexNotFound(*idx))?;
                    let value = p.compute(timestep, scenario_index, state, &parameter_state)?;
                    // debug!("Current value of parameter {}: {}", p.name(), value);
                    if value.is_nan() {
                        panic!("NaN value computed in parameter: {}", p.name());
                    }
                    parameter_state.push_value(value);
                }
                ParameterType::Index(idx) => {
                    let p = self
                        .index_parameters
                        .get_mut(*idx.deref())
                        .ok_or(PywrError::IndexParameterIndexNotFound(*idx))?;

                    let value = p.compute(timestep, scenario_index, state, &parameter_state)?;
                    // debug!("Current value of index parameter {}: {}", p.name(), value);
                    parameter_state.push_index(value);
                }
            }
        }

        for _parameter in &self.index_parameters {}

        Ok(parameter_state)
    }

    fn save_recorders(
        &mut self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<(), PywrError> {
        for recorder in self.recorders.iter_mut() {
            recorder.save(timestep, scenario_index, network_state, parameter_state)?;
        }
        Ok(())
    }

    fn after_save_recorders(&mut self, timestep: &Timestep) -> Result<(), PywrError> {
        for recorder in self.recorders.iter_mut() {
            recorder.after_save(timestep)?;
        }
        Ok(())
    }

    /// Get a Node from a node's name
    pub fn get_node_index_by_name(&self, name: &str, sub_name: Option<&str>) -> Result<NodeIndex, PywrError> {
        Ok(self.get_node_by_name(name, sub_name)?.index())
    }

    /// Get a Node from a node's name
    pub fn get_node_by_name(&self, name: &str, sub_name: Option<&str>) -> Result<&Node, PywrError> {
        match self.nodes.iter().find(|&n| n.full_name() == (name, sub_name)) {
            Some(node) => Ok(node),
            None => Err(PywrError::NodeNotFound(name.to_string())),
        }
    }

    /// Get a NodeIndex from a node's name
    pub fn get_mut_node_by_name(&mut self, name: &str, sub_name: Option<&str>) -> Result<&mut Node, PywrError> {
        match self.nodes.iter_mut().find(|n| n.full_name() == (name, sub_name)) {
            Some(node) => Ok(node),
            None => Err(PywrError::NodeNotFound(name.to_string())),
        }
    }

    pub fn set_node_cost(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: ConstraintValue,
    ) -> Result<(), PywrError> {
        let node = self.get_mut_node_by_name(name, sub_name)?;
        node.set_cost(value);
        Ok(())
    }

    pub fn set_node_max_flow(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: ConstraintValue,
    ) -> Result<(), PywrError> {
        let node = self.get_mut_node_by_name(name, sub_name)?;
        node.set_max_flow_constraint(value)
    }

    pub fn set_node_min_flow(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: ConstraintValue,
    ) -> Result<(), PywrError> {
        let node = self.get_mut_node_by_name(name, sub_name)?;
        node.set_min_flow_constraint(value)
    }

    /// Get a `AggregatedNodeIndex` from a node's name
    pub fn get_aggregated_node_by_name(
        &self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Result<&AggregatedNode, PywrError> {
        match self
            .aggregated_nodes
            .iter()
            .find(|&n| n.full_name() == (name, sub_name))
        {
            Some(node) => Ok(node),
            None => Err(PywrError::NodeNotFound(name.to_string())),
        }
    }

    pub fn get_mut_aggregated_node_by_name(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Result<&mut AggregatedNode, PywrError> {
        match self
            .aggregated_nodes
            .iter_mut()
            .find(|n| n.full_name() == (name, sub_name))
        {
            Some(node) => Ok(node),
            None => Err(PywrError::NodeNotFound(name.to_string())),
        }
    }

    pub fn set_aggregated_node_max_flow(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: ConstraintValue,
    ) -> Result<(), PywrError> {
        let node = self.get_mut_aggregated_node_by_name(name, sub_name)?;
        node.set_max_flow_constraint(value);
        Ok(())
    }

    pub fn set_aggregated_node_min_flow(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        value: ConstraintValue,
    ) -> Result<(), PywrError> {
        let node = self.get_mut_aggregated_node_by_name(name, sub_name)?;
        node.set_min_flow_constraint(value);
        Ok(())
    }

    /// Get a `&AggregatedStorageNode` from a node's name
    pub fn get_aggregated_storage_node_by_name(
        &self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Result<&AggregatedStorageNode, PywrError> {
        match self
            .aggregated_storage_nodes
            .iter()
            .find(|&n| n.full_name() == (name, sub_name))
        {
            Some(node) => Ok(node),
            None => Err(PywrError::NodeNotFound(name.to_string())),
        }
    }

    /// Get a `AggregatedStorageNodeIndex` from a node's name
    pub fn get_aggregated_storage_node_index_by_name(
        &self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Result<AggregatedStorageNodeIndex, PywrError> {
        match self
            .aggregated_storage_nodes
            .iter()
            .find(|&n| n.full_name() == (name, sub_name))
        {
            Some(node) => Ok(node.index()),
            None => Err(PywrError::NodeNotFound(name.to_string())),
        }
    }

    pub fn get_mut_aggregated_storage_node_by_name(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Result<&mut AggregatedStorageNode, PywrError> {
        match self
            .aggregated_storage_nodes
            .iter_mut()
            .find(|n| n.full_name() == (name, sub_name))
        {
            Some(node) => Ok(node),
            None => Err(PywrError::NodeNotFound(name.to_string())),
        }
    }

    /// Get a `VirtualStorageNodeIndex` from a node's name
    pub fn get_virtual_storage_node_by_name(
        &self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Result<&VirtualStorage, PywrError> {
        match self
            .virtual_storage_nodes
            .iter()
            .find(|&n| n.full_name() == (name, sub_name))
        {
            Some(node) => Ok(node),
            None => Err(PywrError::NodeNotFound(name.to_string())),
        }
    }

    pub fn get_storage_node_metric(
        &self,
        name: &str,
        sub_name: Option<&str>,
        proportional: bool,
    ) -> Result<Metric, PywrError> {
        if let Ok(idx) = self.get_node_index_by_name(name, sub_name) {
            // A regular node
            if proportional {
                Ok(Metric::NodeProportionalVolume(idx))
            } else {
                Ok(Metric::NodeVolume(idx))
            }
        } else if let Ok(node) = self.get_aggregated_storage_node_by_name(name, sub_name) {
            if proportional {
                Ok(Metric::AggregatedNodeProportionalVolume(node.nodes.clone()))
            } else {
                Ok(Metric::AggregatedNodeVolume(node.nodes.clone()))
            }
        } else if let Ok(node) = self.get_virtual_storage_node_by_name(name, sub_name) {
            if proportional {
                Ok(Metric::VirtualStorageProportionalVolume(node.index()))
            } else {
                Ok(Metric::VirtualStorageVolume(node.index()))
            }
        } else {
            Err(PywrError::NodeNotFound(name.to_string()))
        }
    }

    pub fn get_node_default_metrics(&self) -> Vec<(Metric, (String, Option<String>))> {
        self.nodes
            .iter()
            .map(|n| {
                let metric = n.default_metric();
                let (name, sub_name) = n.full_name();
                (metric, (name.to_string(), sub_name.map(|s| s.to_string())))
            })
            .collect()
    }

    pub fn get_parameter_metrics(&self) -> Vec<(Metric, (String, Option<String>))> {
        self.parameters
            .iter()
            .enumerate()
            .map(|(idx, p)| {
                let metric = Metric::ParameterValue(ParameterIndex::new(idx));

                (metric, (p.name().to_string(), None))
            })
            .collect()
    }

    /// Get a `Parameter` from a parameter's name
    pub fn get_parameter_by_name(&self, name: &str) -> Result<&dyn parameters::Parameter, PywrError> {
        match self.parameters.iter().find(|p| p.name() == name) {
            Some(parameter) => Ok(parameter.as_ref()),
            None => Err(PywrError::ParameterNotFound(name.to_string())),
        }
    }

    /// Get a `ParameterIndex` from a parameter's name
    pub fn get_parameter_index_by_name(&self, name: &str) -> Result<ParameterIndex, PywrError> {
        match self.parameters.iter().position(|p| p.name() == name) {
            Some(idx) => Ok(ParameterIndex::new(idx)),
            None => Err(PywrError::ParameterNotFound(name.to_string())),
        }
    }

    /// Get a `IndexParameter` from a parameter's name
    pub fn get_index_parameter_by_name(&self, name: &str) -> Result<&dyn parameters::IndexParameter, PywrError> {
        match self.index_parameters.iter().find(|p| p.name() == name) {
            Some(parameter) => Ok(parameter.as_ref()),
            None => Err(PywrError::ParameterNotFound(name.to_string())),
        }
    }

    /// Get a `IndexParameterIndex` from a parameter's name
    pub fn get_index_parameter_index_by_name(&self, name: &str) -> Result<IndexParameterIndex, PywrError> {
        match self.index_parameters.iter().position(|p| p.name() == name) {
            Some(idx) => Ok(IndexParameterIndex::new(idx)),
            None => Err(PywrError::ParameterNotFound(name.to_string())),
        }
    }

    /// Get a `RecorderIndex` from a recorder's name
    pub fn get_recorder_by_name(&self, name: &str) -> Result<&dyn recorders::Recorder, PywrError> {
        match self.recorders.iter().find(|r| r.name() == name) {
            Some(recorder) => Ok(recorder.as_ref()),
            None => Err(PywrError::RecorderNotFound),
        }
    }

    /// Add a new Node::Input to the model.
    pub fn add_input_node(&mut self, name: &str, sub_name: Option<&str>) -> Result<NodeIndex, PywrError> {
        // Check for name.
        // TODO move this check to `NodeVec`
        if let Ok(_node) = self.get_node_by_name(name, sub_name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        // Now add the node to the network.
        let node_index = self.nodes.push_new_input(name, sub_name);
        Ok(node_index)
    }

    /// Add a new Node::Link to the model.
    pub fn add_link_node(&mut self, name: &str, sub_name: Option<&str>) -> Result<NodeIndex, PywrError> {
        // Check for name.
        // TODO move this check to `NodeVec`
        if let Ok(_node) = self.get_node_by_name(name, sub_name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        // Now add the node to the network.
        let node_index = self.nodes.push_new_link(name, sub_name);
        Ok(node_index)
    }

    /// Add a new Node::Link to the model.
    pub fn add_output_node(&mut self, name: &str, sub_name: Option<&str>) -> Result<NodeIndex, PywrError> {
        // Check for name.
        // TODO move this check to `NodeVec`
        if let Ok(_node) = self.get_node_by_name(name, sub_name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        // Now add the node to the network.
        let node_index = self.nodes.push_new_output(name, sub_name);
        Ok(node_index)
    }

    /// Add a new Node::Link to the model.
    pub fn add_storage_node(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        initial_volume: StorageInitialVolume,
        min_volume: f64,
        max_volume: f64,
    ) -> Result<NodeIndex, PywrError> {
        // Check for name.
        // TODO move this check to `NodeVec`
        if let Ok(_node) = self.get_node_by_name(name, sub_name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        // Now add the node to the network.
        let node_index = self
            .nodes
            .push_new_storage(name, sub_name, initial_volume, min_volume, max_volume);
        Ok(node_index)
    }

    /// Add a new `aggregated_node::AggregatedNode` to the model.
    pub fn add_aggregated_node(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        nodes: Vec<NodeIndex>,
    ) -> Result<AggregatedNodeIndex, PywrError> {
        if let Ok(_agg_node) = self.get_aggregated_node_by_name(name, sub_name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        let node_index = self.aggregated_nodes.push_new(name, sub_name, nodes);
        Ok(node_index)
    }

    /// Add a new `aggregated_storage_node::AggregatedStorageNode` to the model.
    pub fn add_aggregated_storage_node(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        nodes: Vec<NodeIndex>,
    ) -> Result<AggregatedStorageNodeIndex, PywrError> {
        if let Ok(_agg_node) = self.get_aggregated_storage_node_by_name(name, sub_name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        let node_index = self.aggregated_storage_nodes.push_new(name, sub_name, nodes);
        Ok(node_index)
    }

    /// Add a new `VirtualStorage` to the model.
    pub fn add_virtual_storage_node(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        nodes: Vec<NodeIndex>,
        factors: Option<Vec<f64>>,
    ) -> Result<VirtualStorageIndex, PywrError> {
        if let Ok(_agg_node) = self.get_virtual_storage_node_by_name(name, sub_name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        let node_index = self.virtual_storage_nodes.push_new(name, sub_name, nodes, factors);
        Ok(node_index)
    }

    /// Add a `parameters::Parameter` to the model
    pub fn add_parameter(&mut self, parameter: Box<dyn parameters::Parameter>) -> Result<ParameterIndex, PywrError> {
        // TODO reinstate this check
        // if let Ok(idx) = self.get_parameter_index(&parameter.meta().name) {
        //     return Err(PywrError::ParameterNameAlreadyExists(
        //         parameter.meta().name.to_string(),
        //         idx,
        //     ));
        // }

        let parameter_index = ParameterIndex::new(self.parameters.len());

        // Add the parameter ...
        self.parameters.push(parameter);
        // .. and add it to the resolve order
        self.parameters_resolve_order
            .push(ParameterType::Parameter(parameter_index));
        Ok(parameter_index)
    }

    /// Add a `parameters::IndexParameter` to the model
    pub fn add_index_parameter(
        &mut self,
        index_parameter: Box<dyn parameters::IndexParameter>,
    ) -> Result<IndexParameterIndex, PywrError> {
        // TODO reinstate this check
        // if let Ok(idx) = self.get_parameter_index(&parameter.meta().name) {
        //     return Err(PywrError::ParameterNameAlreadyExists(
        //         parameter.meta().name.to_string(),
        //         idx,
        //     ));
        // }

        let parameter_index = IndexParameterIndex::new(self.index_parameters.len());

        self.index_parameters.push(index_parameter);
        // .. and add it to the resolve order
        self.parameters_resolve_order
            .push(ParameterType::Index(parameter_index));
        Ok(parameter_index)
    }

    /// Add a `recorders::Recorder` to the model
    pub fn add_recorder(&mut self, recorder: Box<dyn recorders::Recorder>) -> Result<RecorderIndex, PywrError> {
        // TODO reinstate this check
        // if let Ok(idx) = self.get_recorder_by_name(&recorder.meta().name) {
        //     return Err(PywrError::RecorderNameAlreadyExists(
        //         recorder.meta().name.to_string(),
        //         idx,
        //     ));
        // }

        let recorder_index = RecorderIndex::new(self.index_parameters.len());
        self.recorders.push(recorder);
        Ok(recorder_index)
    }

    /// Connect two nodes together
    pub fn connect_nodes(
        &mut self,
        from_node_index: NodeIndex,
        to_node_index: NodeIndex,
    ) -> Result<EdgeIndex, PywrError> {
        // Self connections are not allowed.
        if from_node_index == to_node_index {
            return Err(PywrError::InvalidNodeConnection);
        }

        // Next edge index
        let edge_index = self.edges.push(from_node_index, to_node_index);

        // The model can get in a bad state here if the edge is added to the `from_node`
        // successfully, but fails on the `to_node`.
        // Suggest to do a check before attempting to add.
        let from_node = self.nodes.get_mut(&from_node_index)?;
        from_node.add_outgoing_edge(edge_index)?;
        let to_node = self.nodes.get_mut(&to_node_index)?;
        to_node.add_incoming_edge(edge_index)?;

        Ok(edge_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::Metric;
    use crate::model::Model;
    use crate::node::{Constraint, ConstraintValue};
    use crate::recorders::AssertionRecorder;
    use crate::scenario::{ScenarioGroupCollection, ScenarioIndex};
    use crate::solvers::clp::{ClpSimplex, ClpSolver};
    use crate::solvers::Solver;
    use crate::timestep::Timestepper;
    use float_cmp::approx_eq;
    use ndarray::Array2;
    use std::ops::Deref;
    use time::macros::date;

    fn default_timestepper() -> Timestepper {
        Timestepper::new(date!(2020 - 01 - 01), date!(2020 - 01 - 15), 1)
    }

    fn default_scenarios() -> ScenarioGroupCollection {
        let mut scenarios = ScenarioGroupCollection::default();
        scenarios.add_group("test-scenario", 10);
        scenarios
    }

    #[test]
    fn test_simple_model() {
        let mut model = Model::default();

        let input_node = model.add_input_node("input", None).unwrap();
        let link_node = model.add_link_node("link", None).unwrap();
        let output_node = model.add_output_node("output", None).unwrap();

        assert_eq!(*input_node.deref(), 0);
        assert_eq!(*link_node.deref(), 1);
        assert_eq!(*output_node.deref(), 2);

        let edge = model.connect_nodes(input_node, link_node).unwrap();
        assert_eq!(*edge.deref(), 0);
        let edge = model.connect_nodes(link_node, output_node).unwrap();
        assert_eq!(*edge.deref(), 1);

        // Now assert the internal structure is as expected.
        let input_node = model.get_node_by_name("input", None).unwrap();
        let link_node = model.get_node_by_name("link", None).unwrap();
        let output_node = model.get_node_by_name("output", None).unwrap();
        assert_eq!(input_node.get_outgoing_edges().unwrap().len(), 1);
        assert_eq!(link_node.get_incoming_edges().unwrap().len(), 1);
        assert_eq!(link_node.get_outgoing_edges().unwrap().len(), 1);
        assert_eq!(output_node.get_incoming_edges().unwrap().len(), 1);
    }

    #[test]
    /// Test the duplicate node names are not permitted.
    fn test_duplicate_node_name() {
        let mut model = Model::default();

        model.add_input_node("my-node", None).unwrap();
        // Second add with the same name
        assert_eq!(
            model.add_input_node("my-node", None),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );

        model.add_input_node("my-node", Some("a")).unwrap();
        // Second add with the same name
        assert_eq!(
            model.add_input_node("my-node", Some("a")),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );

        assert_eq!(
            model.add_link_node("my-node", None),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );

        assert_eq!(
            model.add_output_node("my-node", None),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );

        assert_eq!(
            model.add_storage_node("my-node", None, StorageInitialVolume::Absolute(10.0), 0.0, 10.0),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );
    }

    /// Create a simple test model with three nodes.
    fn simple_model() -> Model {
        let mut model = Model::default();

        let input_node = model.add_input_node("input", None).unwrap();
        let link_node = model.add_link_node("link", None).unwrap();
        let output_node = model.add_output_node("output", None).unwrap();

        model.connect_nodes(input_node, link_node).unwrap();
        model.connect_nodes(link_node, output_node).unwrap();

        let inflow = parameters::VectorParameter::new("inflow", vec![10.0; 366]);
        let inflow = model.add_parameter(Box::new(inflow)).unwrap();

        let input_node = model.get_mut_node_by_name("input", None).unwrap();
        input_node
            .set_constraint(ConstraintValue::Parameter(inflow), Constraint::MaxFlow)
            .unwrap();

        let base_demand = 10.0;

        let demand_factor = parameters::ConstantParameter::new("demand-factor", 1.2);
        let demand_factor = model.add_parameter(Box::new(demand_factor)).unwrap();

        let total_demand = parameters::AggregatedParameter::new(
            "total-demand",
            vec![
                parameters::FloatValue::Constant(base_demand),
                parameters::FloatValue::Dynamic(demand_factor),
            ],
            parameters::AggFunc::Product,
        );
        let total_demand = model.add_parameter(Box::new(total_demand)).unwrap();

        let demand_cost = parameters::ConstantParameter::new("demand-cost", -10.0);
        let demand_cost = model.add_parameter(Box::new(demand_cost)).unwrap();

        let output_node = model.get_mut_node_by_name("output", None).unwrap();
        output_node
            .set_constraint(ConstraintValue::Parameter(total_demand), Constraint::MaxFlow)
            .unwrap();
        output_node.set_cost(ConstraintValue::Parameter(demand_cost));

        model
    }

    /// A test model with a single storage node.
    fn simple_storage_model() -> Model {
        let mut model = Model::default();

        let storage_node = model
            .add_storage_node("reservoir", None, StorageInitialVolume::Absolute(100.0), 0.0, 100.0)
            .unwrap();
        let output_node = model.add_output_node("output", None).unwrap();

        model.connect_nodes(storage_node, output_node).unwrap();

        // Apply demand to the model
        // TODO convenience function for adding a constant constraint.
        let demand = parameters::ConstantParameter::new("demand", 10.0);
        let demand = model.add_parameter(Box::new(demand)).unwrap();

        let demand_cost = parameters::ConstantParameter::new("demand-cost", -10.0);
        let demand_cost = model.add_parameter(Box::new(demand_cost)).unwrap();

        let output_node = model.get_mut_node_by_name("output", None).unwrap();
        output_node
            .set_constraint(ConstraintValue::Parameter(demand), Constraint::MaxFlow)
            .unwrap();
        output_node.set_cost(ConstraintValue::Parameter(demand_cost));

        let max_volume = 100.0;

        let storage_node = model.get_mut_node_by_name("reservoir", None).unwrap();
        storage_node
            .set_constraint(ConstraintValue::Scalar(max_volume), Constraint::MaxVolume)
            .unwrap();

        model
    }

    #[test]
    /// Test adding a constant parameter to a model.
    fn test_constant_parameter() {
        let mut model = Model::default();
        let node_index = model.add_input_node("input", None).unwrap();

        let input_max_flow = parameters::ConstantParameter::new("my-constant", 10.0);
        let parameter = model.add_parameter(Box::new(input_max_flow)).unwrap();

        // assign the new parameter to one of the nodes.
        let node = model.get_mut_node_by_name("input", None).unwrap();
        node.set_constraint(ConstraintValue::Parameter(parameter.clone()), Constraint::MaxFlow)
            .unwrap();

        // Try to assign a constraint not defined for particular node type
        assert_eq!(
            node.set_constraint(ConstraintValue::Scalar(10.0), Constraint::MaxVolume),
            Err(PywrError::StorageConstraintsUndefined)
        );
    }

    #[test]
    fn test_step() {
        let mut model = simple_model();
        let timestepper = default_timestepper();
        let scenarios = default_scenarios();
        let mut solver: Box<dyn Solver> = Box::new(ClpSolver::<ClpSimplex>::default());

        solver.setup(&model).unwrap();

        let mut timings = RunTimings::default();
        let timesteps = timestepper.timesteps();
        let mut ts_iter = timesteps.iter();
        let scenario_indices = scenarios.scenario_indices();
        let ts = ts_iter.next().unwrap();
        let current_state = model.get_initial_state(&scenario_indices);
        assert_eq!(current_state.len(), scenario_indices.len());

        let next_state = model
            .step(ts, &scenario_indices, &mut solver, &current_state, &mut timings)
            .unwrap();

        assert_eq!(next_state.len(), scenario_indices.len());

        let output_node = model.get_node_by_name("output", None).unwrap();

        let state0 = next_state.get(0).unwrap();
        let output_inflow = state0.get_node_in_flow(&output_node.index()).unwrap();
        assert!(approx_eq!(f64, output_inflow, 10.0));
    }

    #[test]
    /// Test running a simple model
    fn test_run() {
        let mut model = simple_model();
        let timestepper = default_timestepper();
        let scenarios = default_scenarios();
        let mut solver: Box<dyn Solver> = Box::new(ClpSolver::<ClpSimplex>::default());

        // Set-up assertion for "input" node
        let idx = model.get_node_by_name("input", None).unwrap().index();
        let expected = Array2::from_elem((366, 10), 10.0);
        let recorder = AssertionRecorder::new("input-flow", Metric::NodeOutFlow(idx), expected);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("link", None).unwrap().index();
        let expected = Array2::from_elem((366, 10), 10.0);
        let recorder = AssertionRecorder::new("link-flow", Metric::NodeOutFlow(idx), expected);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("output", None).unwrap().index();
        let expected = Array2::from_elem((366, 10), 10.0);
        let recorder = AssertionRecorder::new("output-flow", Metric::NodeInFlow(idx), expected);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_parameter_index_by_name("total-demand").unwrap();
        let expected = Array2::from_elem((366, 10), 12.0);
        let recorder = AssertionRecorder::new("total-demand", Metric::ParameterValue(idx), expected);
        model.add_recorder(Box::new(recorder)).unwrap();

        model.run(timestepper, scenarios, &mut solver).unwrap();
    }

    #[test]
    fn test_run_storage() {
        let mut model = simple_storage_model();
        let timestepper = default_timestepper();
        let scenarios = default_scenarios();
        let mut solver: Box<dyn Solver> = Box::new(ClpSolver::<ClpSimplex>::default());

        let idx = model.get_node_by_name("output", None).unwrap().index();

        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| if i < 10 { 10.0 } else { 0.0 });

        let recorder = AssertionRecorder::new("output-flow", Metric::NodeInFlow(idx), expected);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("reservoir", None).unwrap().index();

        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (90.0 - 10.0 * i as f64).max(0.0));

        let recorder = AssertionRecorder::new("reservoir-volume", Metric::NodeVolume(idx), expected);
        model.add_recorder(Box::new(recorder)).unwrap();

        model.run(timestepper, scenarios, &mut solver).unwrap();
    }

    #[test]
    /// Test `ScenarioGroupCollection` iteration
    fn test_scenario_iteration() {
        let mut collection = ScenarioGroupCollection::default();
        collection.add_group("Scenarion A", 10);
        collection.add_group("Scenarion B", 2);
        collection.add_group("Scenarion C", 5);

        let scenario_indices = collection.scenario_indices();
        let mut iter = scenario_indices.iter();

        // Test generation of scenario indices
        assert_eq!(
            iter.next(),
            Some(&ScenarioIndex {
                index: 0,
                indices: vec![0, 0, 0]
            })
        );
        assert_eq!(
            iter.next(),
            Some(&ScenarioIndex {
                index: 1,
                indices: vec![0, 0, 1]
            })
        );
        assert_eq!(
            iter.next(),
            Some(&ScenarioIndex {
                index: 2,
                indices: vec![0, 0, 2]
            })
        );
        assert_eq!(
            iter.next(),
            Some(&ScenarioIndex {
                index: 3,
                indices: vec![0, 0, 3]
            })
        );
        assert_eq!(
            iter.next(),
            Some(&ScenarioIndex {
                index: 4,
                indices: vec![0, 0, 4]
            })
        );
        assert_eq!(
            iter.next(),
            Some(&ScenarioIndex {
                index: 5,
                indices: vec![0, 1, 0]
            })
        );
        assert_eq!(
            iter.next(),
            Some(&ScenarioIndex {
                index: 6,
                indices: vec![0, 1, 1]
            })
        );
        assert_eq!(
            iter.next(),
            Some(&ScenarioIndex {
                index: 7,
                indices: vec![0, 1, 2]
            })
        );
        assert_eq!(
            iter.next(),
            Some(&ScenarioIndex {
                index: 8,
                indices: vec![0, 1, 3]
            })
        );
        assert_eq!(
            iter.next(),
            Some(&ScenarioIndex {
                index: 9,
                indices: vec![0, 1, 4]
            })
        );
        assert_eq!(
            iter.next(),
            Some(&ScenarioIndex {
                index: 10,
                indices: vec![1, 0, 0]
            })
        );

        // Test final index
        assert_eq!(
            iter.last(),
            Some(&ScenarioIndex {
                index: 99,
                indices: vec![9, 1, 4]
            })
        );
    }
}
