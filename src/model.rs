use crate::edge::{Edge, EdgeIndex};
use crate::node::Node;

use crate::scenario::{ScenarioGroupCollection, ScenarioIndex};
use crate::solvers::Solver;
use crate::state::{EdgeState, NetworkState, ParameterState};
use crate::timestep::{Timestep, Timestepper};
use crate::{parameters, recorders, PywrError};

use std::time::Instant;

pub struct Model {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    parameters: Vec<parameters::Parameter>,
    index_parameters: Vec<parameters::IndexParameter>,
    parameters_resolve_order: Vec<parameters::ParameterType>,
    recorders: Vec<recorders::Recorder>,
    scenarios: ScenarioGroupCollection,
}

// Required for Python API
unsafe impl Send for Model {}

impl Default for Model {
    fn default() -> Self {
        Self::new()
    }
}

impl Model {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            parameters: Vec::new(),
            index_parameters: Vec::new(),
            parameters_resolve_order: Vec::new(),
            recorders: Vec::new(),
            scenarios: ScenarioGroupCollection::new(),
        }
    }

    /// Returns the initial state of the network
    pub(crate) fn get_initial_state(&self, scenario_indices: &[ScenarioIndex]) -> Vec<NetworkState> {
        let mut states: Vec<NetworkState> = Vec::new();

        for _scenario_index in scenario_indices {
            let mut state = NetworkState::new();

            for node in &self.nodes {
                state.push_node_state(node.new_state());
            }

            for _edge in &self.edges {
                state.push_edge_state(EdgeState::new());
            }

            states.push(state)
        }
        states
    }

    fn setup(&self, timesteps: &Vec<Timestep>, scenario_indices: &Vec<ScenarioIndex>) -> Result<(), PywrError> {
        // Setup parameters
        for parameter in self.parameters.iter() {
            parameter.setup(self, timesteps, scenario_indices)?;
        }

        for parameter in self.index_parameters.iter() {
            parameter.setup(self, timesteps, scenario_indices)?;
        }

        // Setup recorders
        for recorder in self.recorders.iter() {
            recorder.setup(self, timesteps, scenario_indices)?;
        }

        Ok(())
    }

    fn finalise(&self) -> Result<(), PywrError> {
        // Setup recorders
        for recorder in self.recorders.iter() {
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

        let timesteps = timestepper.timesteps();
        let scenario_indices = scenarios.scenario_indices();
        // One state per scenario
        let mut current_states = self.get_initial_state(&scenario_indices);

        // Setup the solver
        let mut count = 0;
        solver.setup(self)?;
        self.setup(&timesteps, &scenario_indices)?;

        // Step a timestep
        for timestep in timesteps.iter() {
            let next_states = self.step(timestep, &scenario_indices, solver, &current_states)?;
            current_states = next_states;
            count += scenario_indices.len();
        }
        println!("speed: {} ts/s", count as f64 / now.elapsed().as_secs_f64());
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
    ) -> Result<Vec<NetworkState>, PywrError> {
        let mut next_states = Vec::with_capacity(current_states.len());

        for scenario_index in scenario_indices.iter() {
            let current_state = match current_states.get(scenario_index.index) {
                Some(s) => s,
                None => return Err(PywrError::ScenarioStateNotFound),
            };
            let pstate = self.compute_parameters(timestep, scenario_index, current_state)?;

            let next_state = solver.solve(self, timestep, current_state, &pstate)?;

            self.save_recorders(timestep, scenario_index, &next_state, &pstate)?;
            next_states.push(next_state);
        }

        self.after_save_recorders(timestep)?;

        Ok(next_states)
    }

    fn compute_parameters(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        state: &NetworkState,
    ) -> Result<ParameterState, PywrError> {
        let mut parameter_state = ParameterState::with_capacity(self.parameters.len(), 0);

        for parameter in &self.parameters {
            let value = parameter.compute(timestep, scenario_index, self, state, &parameter_state)?;
            parameter_state.push_value(value);
        }

        for _parameter in &self.index_parameters {}

        Ok(parameter_state)
    }

    fn save_recorders(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<(), PywrError> {
        for recorder in self.recorders.iter() {
            recorder.save(timestep, scenario_index, self, network_state, parameter_state)?;
        }
        Ok(())
    }

    fn after_save_recorders(&self, timestep: &Timestep) -> Result<(), PywrError> {
        for recorder in self.recorders.iter() {
            recorder.after_save(timestep)?;
        }
        Ok(())
    }

    /// Get a NodeIndex from a node's name
    pub fn get_node_by_name(&self, name: &str) -> Result<Node, PywrError> {
        match self.nodes.iter().find(|&n| n.name() == name) {
            Some(node) => Ok(node.clone()),
            None => Err(PywrError::NodeNotFound(name.to_string())),
        }
    }

    /// Get a `ParameterIndex` from a parameter's name
    pub fn get_parameter_by_name(&self, name: &str) -> Result<parameters::Parameter, PywrError> {
        match self.parameters.iter().find(|p| p.name() == name) {
            Some(parameter) => Ok(parameter.clone()),
            None => Err(PywrError::ParameterNotFound(name.to_string())),
        }
    }

    /// Get a `IndexParameterIndex` from a parameter's name
    pub fn get_index_parameter_by_name(&self, name: &str) -> Result<parameters::IndexParameter, PywrError> {
        match self.index_parameters.iter().find(|p| p.name() == name) {
            Some(parameter) => Ok(parameter.clone()),
            None => Err(PywrError::ParameterNotFound(name.to_string())),
        }
    }

    /// Get a `RecorderIndex` from a recorder's name
    pub fn get_recorder_by_name(&self, name: &str) -> Result<recorders::Recorder, PywrError> {
        match self.recorders.iter().find(|r| r.name() == name) {
            Some(recorder) => Ok(recorder.clone()),
            None => Err(PywrError::RecorderNotFound),
        }
    }

    /// Add a new Node::Input to the model.
    pub fn add_input_node(&mut self, name: &str) -> Result<Node, PywrError> {
        // Check for name.
        if let Ok(_node) = self.get_node_by_name(name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        // Now add the node to the network.
        let node_index = self.nodes.len();
        let node = Node::new_input(&node_index, name);
        self.nodes.push(node.clone());
        Ok(node)
    }

    /// Add a new Node::Link to the model.
    pub fn add_link_node(&mut self, name: &str) -> Result<Node, PywrError> {
        // Check for name.
        if let Ok(_node) = self.get_node_by_name(name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        // Now add the node to the network.
        let node_index = self.nodes.len();
        let node = Node::new_link(&node_index, name);
        self.nodes.push(node.clone());
        Ok(node)
    }

    /// Add a new Node::Link to the model.
    pub fn add_output_node(&mut self, name: &str) -> Result<Node, PywrError> {
        // Check for name.
        if let Ok(_node) = self.get_node_by_name(name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        // Now add the node to the network.
        let node_index = self.nodes.len();
        let node = Node::new_output(&node_index, name);
        self.nodes.push(node.clone());
        Ok(node)
    }

    /// Add a new Node::Link to the model.
    pub fn add_storage_node(&mut self, name: &str, initial_volume: f64) -> Result<Node, PywrError> {
        // Check for name.
        if let Ok(_node) = self.get_node_by_name(name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        // Now add the node to the network.
        let node_index = self.nodes.len();
        let node = Node::new_storage(&node_index, name, initial_volume);
        self.nodes.push(node.clone());
        Ok(node)
    }

    /// Add a `parameters::Parameter` to the model
    pub fn add_parameter(
        &mut self,
        parameter: Box<dyn parameters::_Parameter>,
    ) -> Result<parameters::Parameter, PywrError> {
        // TODO reinstate this check
        // if let Ok(idx) = self.get_parameter_index(&parameter.meta().name) {
        //     return Err(PywrError::ParameterNameAlreadyExists(
        //         parameter.meta().name.to_string(),
        //         idx,
        //     ));
        // }

        let parameter_index = self.parameters.len();

        let p = parameters::Parameter::new(parameter, parameter_index);
        self.parameters.push(p.clone());
        Ok(p)
    }

    /// Add a `parameters::IndexParameter` to the model
    pub fn add_index_parameter(
        &mut self,
        index_parameter: Box<dyn parameters::_IndexParameter>,
    ) -> Result<parameters::IndexParameter, PywrError> {
        // TODO reinstate this check
        // if let Ok(idx) = self.get_parameter_index(&parameter.meta().name) {
        //     return Err(PywrError::ParameterNameAlreadyExists(
        //         parameter.meta().name.to_string(),
        //         idx,
        //     ));
        // }

        let parameter_index = self.index_parameters.len();

        let p = parameters::IndexParameter::new(index_parameter, parameter_index);
        self.index_parameters.push(p.clone());
        Ok(p)
    }

    /// Add a `recorders::Recorder` to the model
    pub fn add_recorder(&mut self, recorder: Box<dyn recorders::_Recorder>) -> Result<recorders::Recorder, PywrError> {
        // TODO reinstate this check
        // if let Ok(idx) = self.get_recorder_by_name(&recorder.meta().name) {
        //     return Err(PywrError::RecorderNameAlreadyExists(
        //         recorder.meta().name.to_string(),
        //         idx,
        //     ));
        // }

        let recorder_index = self.recorders.len();
        let r = recorders::Recorder::new(recorder, recorder_index);
        self.recorders.push(r.clone());
        Ok(r)
    }

    /// Connect two nodes together
    pub(crate) fn connect_nodes(&mut self, from_node: &Node, to_node: &Node) -> Result<Edge, PywrError> {
        // TODO check whether an edge between these two nodes already exists.

        // Self connections are not allowed.
        if from_node == to_node {
            return Err(PywrError::InvalidNodeConnection);
        }

        // Next edge index
        let edge_index = self.edges.len() as EdgeIndex;
        let edge = Edge::new(&edge_index, from_node, to_node);

        // The model can get in a bad state here if the edge is added to the `from_node`
        // successfully, but fails on the `to_node`.
        // Suggest to do a check before attempting to add.
        from_node.add_outgoing_edge(edge.clone())?;
        to_node.add_incoming_edge(edge.clone())?;

        self.edges.push(edge.clone());

        Ok(edge)
    }

    /// Add a scenario to the model.
    pub fn add_scenario(&mut self, name: &str, size: usize) -> Result<(), PywrError> {
        self.scenarios.add_group(name, size);
        Ok(())
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
    use crate::solvers::clp::ClpSolver;
    use crate::solvers::Solver;
    use crate::timestep::Timestepper;
    use float_cmp::approx_eq;

    use ndarray::Array2;

    fn default_timestepper() -> Timestepper {
        Timestepper::new("2020-01-01", "2020-01-15", "%Y-%m-%d", 1).unwrap()
    }

    fn default_scenarios() -> ScenarioGroupCollection {
        let mut scenarios = ScenarioGroupCollection::new();
        scenarios.add_group("test-scenario", 10);
        scenarios
    }

    #[test]
    fn test_simple_model() {
        let mut model = Model::new();

        let input_node = model.add_input_node("input").unwrap();
        let link_node = model.add_link_node("link").unwrap();
        let output_node = model.add_output_node("output").unwrap();

        assert_eq!(input_node.index(), 0);
        assert_eq!(link_node.index(), 1);
        assert_eq!(output_node.index(), 2);

        let edge = model.connect_nodes(&input_node, &link_node).unwrap();
        assert_eq!(edge.index(), 0);
        let edge = model.connect_nodes(&link_node, &output_node).unwrap();
        assert_eq!(edge.index(), 1);

        // Now assert the internal instructure is as expected.
        assert_eq!(input_node.get_outgoing_edges().unwrap().len(), 1);
        assert_eq!(link_node.get_incoming_edges().unwrap().len(), 1);
        assert_eq!(link_node.get_outgoing_edges().unwrap().len(), 1);
        assert_eq!(output_node.get_incoming_edges().unwrap().len(), 1);
    }

    #[test]
    /// Test the duplicate node names are not permitted.
    fn test_duplicate_node_name() {
        let mut model = Model::new();

        model.add_input_node("my-node").unwrap();
        // Second add with the same name
        assert_eq!(
            model.add_input_node("my-node"),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );

        assert_eq!(
            model.add_link_node("my-node"),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );

        assert_eq!(
            model.add_output_node("my-node"),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );

        assert_eq!(
            model.add_storage_node("my-node", 10.0),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );
    }

    /// Create a simple test model with three nodes.
    fn simple_model() -> Model {
        let mut model = Model::new();

        let input_node = model.add_input_node("input").unwrap();
        let link_node = model.add_link_node("link").unwrap();
        let output_node = model.add_output_node("output").unwrap();

        model.connect_nodes(&input_node, &link_node).unwrap();
        model.connect_nodes(&link_node, &output_node).unwrap();

        let inflow = parameters::VectorParameter::new("inflow", vec![10.0; 366]);
        let inflow = model.add_parameter(Box::new(inflow)).unwrap();

        input_node
            .set_constraint(ConstraintValue::Parameter(inflow), Constraint::MaxFlow)
            .unwrap();

        let base_demand = parameters::ConstantParameter::new("base-demand", 10.0);
        let base_demand = model.add_parameter(Box::new(base_demand)).unwrap();

        let demand_factor = parameters::ConstantParameter::new("demand-factor", 1.2);
        let demand_factor = model.add_parameter(Box::new(demand_factor)).unwrap();

        let total_demand = parameters::AggregatedParameter::new(
            "total-demand",
            vec![base_demand, demand_factor],
            parameters::AggFunc::Product,
        );
        let total_demand = model.add_parameter(Box::new(total_demand)).unwrap();

        output_node
            .set_constraint(ConstraintValue::Parameter(total_demand), Constraint::MaxFlow)
            .unwrap();

        let demand_cost = parameters::ConstantParameter::new("demand-cost", -10.0);
        let demand_cost = model.add_parameter(Box::new(demand_cost)).unwrap();

        output_node.set_cost(ConstraintValue::Parameter(demand_cost));

        model
    }

    /// A test model with a single storage node.
    fn simple_storage_model() -> Model {
        let mut model = Model::new();

        let storage_node = model.add_storage_node("reservoir", 100.0).unwrap();
        let output_node = model.add_output_node("output").unwrap();

        model.connect_nodes(&storage_node, &output_node).unwrap();

        // Apply demand to the model
        // TODO convenience function for adding a constant constraint.
        let demand = parameters::ConstantParameter::new("demand", 10.0);
        let demand = model.add_parameter(Box::new(demand)).unwrap();
        output_node
            .set_constraint(ConstraintValue::Parameter(demand), Constraint::MaxFlow)
            .unwrap();

        let demand_cost = parameters::ConstantParameter::new("demand-cost", -10.0);
        let demand_cost = model.add_parameter(Box::new(demand_cost)).unwrap();
        output_node.set_cost(ConstraintValue::Parameter(demand_cost));

        let max_volume = parameters::ConstantParameter::new("max-volume", 100.0);
        let max_volume = model.add_parameter(Box::new(max_volume)).unwrap();

        storage_node
            .set_constraint(ConstraintValue::Parameter(max_volume), Constraint::MaxVolume)
            .unwrap();

        model
    }

    #[test]
    /// Test adding a constant parameter to a model.
    fn test_constant_parameter() {
        let mut model = Model::new();
        let node = model.add_input_node("input").unwrap();

        let input_max_flow = parameters::ConstantParameter::new("my-constant", 10.0);
        let parameter = model.add_parameter(Box::new(input_max_flow)).unwrap();
        assert_eq!(parameter.index(), 0);
        // assign the new parameter to one of the nodes.
        node.set_constraint(ConstraintValue::Parameter(parameter.clone()), Constraint::MaxFlow)
            .unwrap();

        // Try to assign a constraint not defined for particular node type
        assert_eq!(
            node.set_constraint(ConstraintValue::Parameter(parameter), Constraint::MaxVolume),
            Err(PywrError::StorageConstraintsUndefined)
        );
    }

    #[test]
    fn test_step() {
        let mut model = simple_model();
        let timestepper = default_timestepper();
        let scenarios = default_scenarios();
        let mut solver: Box<dyn Solver> = Box::new(ClpSolver::new());

        solver.setup(&model).unwrap();

        let timesteps = timestepper.timesteps();
        let mut ts_iter = timesteps.iter();
        let scenario_indices = scenarios.scenario_indices();
        let ts = ts_iter.next().unwrap();
        let current_state = model.get_initial_state(&scenario_indices);
        assert_eq!(current_state.len(), scenario_indices.len());

        let next_state = model.step(ts, &scenario_indices, &mut solver, &current_state).unwrap();

        assert_eq!(next_state.len(), scenario_indices.len());

        let output_node = model.get_node_by_name("output").unwrap();

        let state0 = next_state.get(0).unwrap();
        let output_inflow = state0.get_node_in_flow(output_node.index()).unwrap();
        assert!(approx_eq!(f64, output_inflow, 10.0));
    }

    #[test]
    /// Test running a simple model
    fn test_run() {
        let mut model = simple_model();
        let timestepper = default_timestepper();
        let scenarios = default_scenarios();
        let mut solver: Box<dyn Solver> = Box::new(ClpSolver::new());

        // Set-up assertion for "input" node
        let idx = model.get_node_by_name("input").unwrap().index();
        let expected = Array2::from_elem((366, 10), 10.0);
        let recorder = AssertionRecorder::new("input-flow", Metric::NodeOutFlow(idx), expected);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("link").unwrap().index();
        let expected = Array2::from_elem((366, 10), 10.0);
        let recorder = AssertionRecorder::new("link-flow", Metric::NodeOutFlow(idx), expected);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("output").unwrap().index();
        let expected = Array2::from_elem((366, 10), 10.0);
        let recorder = AssertionRecorder::new("output-flow", Metric::NodeInFlow(idx), expected);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_parameter_by_name("total-demand").unwrap().index();
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
        let mut solver: Box<dyn Solver> = Box::new(ClpSolver::new());

        let idx = model.get_node_by_name("output").unwrap().index();

        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| if i < 10 { 10.0 } else { 0.0 });

        let recorder = AssertionRecorder::new("output-flow", Metric::NodeInFlow(idx), expected);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("reservoir").unwrap().index();

        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (90.0 - 10.0 * i as f64).max(0.0));

        let recorder = AssertionRecorder::new("reservoir-volume", Metric::NodeVolume(idx), expected);
        model.add_recorder(Box::new(recorder)).unwrap();

        model.run(timestepper, scenarios, &mut solver).unwrap();
    }

    #[test]
    /// Test `ScenarioGroupCollection` iteration
    fn test_scenario_iteration() {
        let mut collection = ScenarioGroupCollection::new();
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
