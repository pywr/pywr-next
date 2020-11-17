use chrono::{Duration as ChronoDuration, NaiveDate, ParseError};
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::PyErr;
use std::cmp::Ordering;
use std::ops::Add;
use std::time::Instant;
use thiserror::Error;

mod glpk;
pub mod parameters;

#[derive(Error, Debug, PartialEq)]
pub enum PywrError {
    #[error("invalid node connect")]
    InvalidNodeConnection,
    #[error("connection to node is already defined")]
    NodeConnectionAlreadyExists,
    #[error("node index not found")]
    NodeIndexNotFound,
    #[error("edge index not found")]
    EdgeIndexNotFound,
    #[error("parameter index not found")]
    ParameterIndexNotFound,
    #[error("node name `{0}` already exists on node {1}")]
    NodeNameAlreadyExists(String, NodeIndex),
    #[error("parameter name `{0}` already exists on parameter {1}")]
    ParameterNameAlreadyExists(String, ParameterIndex),
    #[error("connections from output nodes are invalid")]
    InvalidNodeConnectionFromOutput,
    #[error("connections to input nodes are invalid")]
    InvalidNodeConnectionToInput,
    #[error("flow constraints are undefined for this node")]
    FlowConstraintsUndefined,
    #[error("storage constraints are undefined for this node")]
    StorageConstraintsUndefined,
    #[error("unable to parse date")]
    ParseError(#[from] ParseError),
    #[error("timestep index out of range")]
    TimestepIndexOutOfRange,
    #[error("glpk error")]
    GlpkError(#[from] glpk::GlpkError),
    #[error("solver not initialised")]
    SolverNotSetup,
    #[error("no edges defined")]
    NoEdgesDefined,
    #[error("Python error")]
    PythonError,
    #[error("Unrecognised solver")]
    UnrecognisedSolver,
    #[error("Solve failed")]
    SolveFailed,
    #[error("atleast one parameter is required")]
    AtleastOneParameterRequired,
    #[error("scenario state not found")]
    ScenarioStateNotFound
}

type NodeIndex = usize;
type EdgeIndex = usize;
type ParameterIndex = usize;
type TimestepIndex = usize;

#[derive(Debug)]
enum Node {
    Input(InputNode),
    Output(OutputNode),
    Link(LinkNode),
    Storage(StorageNode),
}

#[derive(Debug, Clone)]
pub enum Constraint {
    MinFlow,
    MaxFlow,
    MinAndMaxFlow,
    MinVolume,
    MaxVolume,
}

impl Node {
    /// Get a node's name
    fn name(&self) -> &str {
        &self.meta().name
    }

    /// Get a node's name
    fn index(&self) -> &NodeIndex {
        &self.meta().index
    }

    /// Get a node's metadata
    fn meta(&self) -> &NodeMeta {
        match self {
            Node::Input(n) => &n.meta,
            Node::Output(n) => &n.meta,
            Node::Link(n) => &n.meta,
            Node::Storage(n) => &n.meta,
        }
    }

    /// Connect one node to another
    fn connect(&mut self, other: &mut Node, next_edge_index: &EdgeIndex) -> Result<Edge, PywrError> {
        // Connections to from output nodes are invalid.
        match self {
            Node::Output(_) => return Err(PywrError::InvalidNodeConnectionFromOutput),
            _ => {}
        };

        // Connections to input nodes are invalid.
        match other {
            Node::Input(_) => return Err(PywrError::InvalidNodeConnectionToInput),
            _ => {}
        };

        // Create the edge
        let edge = Edge::new(next_edge_index, self.index(), other.index());

        // Add the outgoing connection
        match self {
            Node::Input(n) => n.outgoing_edges.push(next_edge_index.clone()),
            Node::Link(n) => n.outgoing_edges.push(next_edge_index.clone()),
            Node::Storage(n) => n.outgoing_edges.push(next_edge_index.clone()),
            _ => panic!("This should not happen!!"),
        }

        // Add the outgoing connection
        match other {
            Node::Output(n) => n.incoming_edges.push(next_edge_index.clone()),
            Node::Link(n) => n.incoming_edges.push(next_edge_index.clone()),
            Node::Storage(n) => n.incoming_edges.push(next_edge_index.clone()),
            _ => panic!("This should not happen!!"),
        }

        Ok(edge)
    }

    // /// Return a reference to a node's flow constraints if they exist.
    // fn flow_constraints(&self) -> Option<&FlowConstraints> {
    //     match self {
    //         Node::Input(n) => Some(&n.flow_constraints),
    //         Node::Link(n) => Some(&n.flow_constraints),
    //         Node::Output(n) => Some(&n.flow_constraints),
    //         Node::Storage(n) => None,
    //     }
    // }

    /// Return a mutable reference to a node's flow constraints if they exist.
    fn flow_constraints_mut(&mut self) -> Result<&mut FlowConstraints, PywrError> {
        match self {
            Node::Input(n) => Ok(&mut n.flow_constraints),
            Node::Link(n) => Ok(&mut n.flow_constraints),
            Node::Output(n) => Ok(&mut n.flow_constraints),
            Node::Storage(_) => Err(PywrError::FlowConstraintsUndefined),
        }
    }

    // /// Return a reference to a node's storage constraints if they exist.
    // fn storage_constraints(&self) -> Result<&StorageConstraints, PywrError> {
    //     match self {
    //         Node::Input(_) => Err(PywrError::StorageConstraintsUndefined),
    //         Node::Link(_) => Err(PywrError::StorageConstraintsUndefined),
    //         Node::Output(_) => Err(PywrError::StorageConstraintsUndefined),
    //         Node::Storage(n) => Ok(&n.storage_constraints),
    //     }
    // }

    /// Return a mutable reference to a node's storage constraints if they exist.
    fn storage_constraints_mut(&mut self) -> Result<&mut StorageConstraints, PywrError> {
        match self {
            Node::Input(_) => Err(PywrError::StorageConstraintsUndefined),
            Node::Link(_) => Err(PywrError::StorageConstraintsUndefined),
            Node::Output(_) => Err(PywrError::StorageConstraintsUndefined),
            Node::Storage(n) => Ok(&mut n.storage_constraints),
        }
    }

    /// Set a constraint on a node.
    fn set_constraint(&mut self, param_idx: Option<ParameterIndex>, constraint: Constraint) -> Result<(), PywrError> {
        match constraint {
            Constraint::MinFlow => {
                let flow_constraints = self.flow_constraints_mut()?;
                flow_constraints.min_flow = param_idx;
            }
            Constraint::MaxFlow => {
                let flow_constraints = self.flow_constraints_mut()?;
                flow_constraints.max_flow = param_idx;
            }
            Constraint::MinAndMaxFlow => {
                let flow_constraints = self.flow_constraints_mut()?;
                flow_constraints.min_flow = param_idx;
                flow_constraints.max_flow = param_idx;
            }
            Constraint::MinVolume => {
                let storage_constraints = self.storage_constraints_mut()?;
                storage_constraints.min_volume = param_idx;
            }
            Constraint::MaxVolume => {
                let storage_constraints = self.storage_constraints_mut()?;
                storage_constraints.max_volume = param_idx;
            }
        }
        Ok(())
    }

    // fn cost(&self) -> Result<Option<ParameterIndex>, PywrError> {
    //     match self {
    //         Node::Input(n) => Ok(n.cost),
    //         Node::Link(n) => Ok(n.cost),
    //         Node::Output(n) => Ok(n.cost),
    //         Node::Storage(n) => Ok(n.cost),
    //     }
    // }

    fn set_cost(&mut self, param_idx: Option<ParameterIndex>) -> Result<(), PywrError> {
        match self {
            Node::Input(n) => n.cost = param_idx,
            Node::Link(n) => n.cost = param_idx,
            Node::Output(n) => n.cost = param_idx,
            Node::Storage(n) => n.cost = param_idx,
        }
        Ok(())
    }
}

/// Meta data common to all nodes.
#[derive(Debug)]
pub struct NodeMeta {
    index: NodeIndex,
    name: String,
    comment: String,
}

impl NodeMeta {
    fn new(index: &NodeIndex, name: &str) -> Self {
        Self {
            index: index.clone(),
            name: name.to_string(),
            comment: "".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct FlowConstraints {
    min_flow: Option<ParameterIndex>,
    max_flow: Option<ParameterIndex>,
}

impl FlowConstraints {
    fn new() -> Self {
        Self {
            min_flow: None,
            max_flow: None,
        }
    }
}

#[derive(Debug)]
pub struct StorageConstraints {
    min_volume: Option<ParameterIndex>,
    max_volume: Option<ParameterIndex>,
}

impl StorageConstraints {
    fn new() -> Self {
        Self {
            min_volume: None,
            max_volume: None,
        }
    }
}

#[derive(Debug)]
pub struct InputNode {
    meta: NodeMeta,
    cost: Option<ParameterIndex>,
    flow_constraints: FlowConstraints,
    outgoing_edges: Vec<EdgeIndex>,
}

impl InputNode {
    fn new(index: &NodeIndex, name: &str) -> Self {
        Self {
            meta: NodeMeta::new(index, name),
            cost: None,
            flow_constraints: FlowConstraints::new(),
            outgoing_edges: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct OutputNode {
    meta: NodeMeta,
    cost: Option<ParameterIndex>,
    flow_constraints: FlowConstraints,
    incoming_edges: Vec<EdgeIndex>,
}

impl OutputNode {
    fn new(index: &NodeIndex, name: &str) -> Self {
        Self {
            meta: NodeMeta::new(index, name),
            cost: None,
            flow_constraints: FlowConstraints::new(),
            incoming_edges: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct LinkNode {
    meta: NodeMeta,
    cost: Option<ParameterIndex>,
    flow_constraints: FlowConstraints,
    incoming_edges: Vec<EdgeIndex>,
    outgoing_edges: Vec<EdgeIndex>,
}

impl LinkNode {
    fn new(index: &NodeIndex, name: &str) -> Self {
        Self {
            meta: NodeMeta::new(index, name),
            cost: None,
            flow_constraints: FlowConstraints::new(),
            incoming_edges: Vec::new(),
            outgoing_edges: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct StorageNode {
    meta: NodeMeta,
    cost: Option<ParameterIndex>,
    storage_constraints: StorageConstraints,
    incoming_edges: Vec<EdgeIndex>,
    outgoing_edges: Vec<EdgeIndex>,
}

impl StorageNode {
    fn new(index: &NodeIndex, name: &str) -> Self {
        Self {
            meta: NodeMeta::new(index, name),
            cost: None,
            storage_constraints: StorageConstraints::new(),
            incoming_edges: Vec::new(),
            outgoing_edges: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct Edge {
    index: EdgeIndex,
    from_node_index: NodeIndex,
    to_node_index: NodeIndex,
}

impl Edge {
    fn new(index: &EdgeIndex, from_node_index: &NodeIndex, to_node_index: &NodeIndex) -> Self {
        Self {
            index: index.clone(),
            from_node_index: from_node_index.clone(),
            to_node_index: to_node_index.clone(),
        }
    }

    fn cost(&self, model: &Model, parameter_states: &ParameterState) -> Result<f64, PywrError> {
        let from_node = match model.nodes.get(self.from_node_index) {
            Some(n) => n,
            None => return Err(PywrError::NodeIndexNotFound),
        };

        let to_node = match model.nodes.get(self.to_node_index) {
            Some(n) => n,
            None => return Err(PywrError::NodeIndexNotFound),
        };

        let from_cost = match from_node {
            Node::Input(n) => match n.cost {
                Some(cost_idx) => match parameter_states.get(cost_idx) {
                    Some(s) => *s,
                    None => return Err(PywrError::ParameterIndexNotFound),
                },
                None => 0.0,
            },
            Node::Link(n) => match n.cost {
                Some(cost_idx) => match parameter_states.get(cost_idx) {
                    Some(s) => *s,
                    None => return Err(PywrError::ParameterIndexNotFound),
                },
                None => 0.0,
            },
            Node::Output(n) => match n.cost {
                Some(cost_idx) => match parameter_states.get(cost_idx) {
                    Some(s) => s / 2.0,
                    None => return Err(PywrError::ParameterIndexNotFound),
                },
                None => 0.0,
            },
            Node::Storage(n) => {
                match n.cost {
                    // Storage provides -ve cost for outgoing edges (i.e. if the storage node is
                    // the "from" node.
                    Some(cost_idx) => match parameter_states.get(cost_idx) {
                        Some(s) => -s,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    },
                    None => 0.0,
                }
            }
        };

        let to_cost = match to_node {
            Node::Input(n) => match n.cost {
                Some(cost_idx) => match parameter_states.get(cost_idx) {
                    Some(s) => *s,
                    None => return Err(PywrError::ParameterIndexNotFound),
                },
                None => 0.0,
            },
            Node::Link(n) => match n.cost {
                Some(cost_idx) => match parameter_states.get(cost_idx) {
                    Some(s) => s / 2.0,
                    None => return Err(PywrError::ParameterIndexNotFound),
                },
                None => 0.0,
            },
            Node::Output(n) => match n.cost {
                Some(cost_idx) => match parameter_states.get(cost_idx) {
                    Some(s) => *s,
                    None => return Err(PywrError::ParameterIndexNotFound),
                },
                None => 0.0,
            },
            Node::Storage(n) => {
                match n.cost {
                    // Storage provides +ve cost for incoming edges (i.e. if the storage node is
                    // the "to" node.
                    Some(cost_idx) => match parameter_states.get(cost_idx) {
                        Some(s) => *s,
                        None => return Err(PywrError::ParameterIndexNotFound),
                    },
                    None => 0.0,
                }
            }
        };

        Ok(from_cost + to_cost)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum NodeState {
    Flow(FlowState),
    Storage(StorageState),
}

impl NodeState {
    fn reset(&mut self) {
        match self {
            Self::Flow(s) => s.reset(),
            Self::Storage(s) => s.reset(),
        }
    }

    fn add_in_flow(&mut self, flow: f64, timestep: &Timestep) {
        match self {
            Self::Flow(s) => s.add_in_flow(flow),
            Self::Storage(s) => s.add_in_flow(flow, timestep),
        };
    }

    fn add_out_flow(&mut self, flow: f64, timestep: &Timestep) {
        match self {
            Self::Flow(s) => s.add_out_flow(flow),
            Self::Storage(s) => s.add_out_flow(flow, timestep),
        };
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FlowState {
    in_flow: f64,
    out_flow: f64,
}

impl FlowState {
    fn new() -> Self {
        Self {
            in_flow: 0.0,
            out_flow: 0.0,
        }
    }

    fn reset(&mut self) {
        self.in_flow = 0.0;
        self.out_flow = 0.0;
    }

    fn add_in_flow(&mut self, flow: f64) {
        self.in_flow += flow;
    }
    fn add_out_flow(&mut self, flow: f64) {
        self.out_flow += flow;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct StorageState {
    volume: f64,
    flows: FlowState,
}

impl StorageState {
    fn new(volume: f64) -> Self {
        Self {
            volume,
            flows: FlowState::new(),
        }
    }

    fn reset(&mut self) {
        self.flows.reset();
        // Volume remains unchanged
    }

    fn add_in_flow(&mut self, flow: f64, timestep: &Timestep) {
        self.flows.add_in_flow(flow);
        self.volume += flow * timestep.days();
    }
    fn add_out_flow(&mut self, flow: f64, timestep: &Timestep) {
        self.flows.add_out_flow(flow);
        self.volume -= flow * timestep.days();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct EdgeState {
    flow: f64,
}

impl EdgeState {
    fn new() -> Self {
        Self { flow: 0.0 }
    }
    fn add_flow(&mut self, flow: f64) {
        self.flow += flow;
    }
}

pub type ParameterState = Vec<f64>;

// State of the nodes and edges
#[derive(Clone, Debug)]
pub struct NetworkState {
    node_states: Vec<NodeState>,
    edge_states: Vec<EdgeState>,
}

impl NetworkState {
    fn new() -> Self {
        Self {
            node_states: Vec::new(),
            edge_states: Vec::new(),
        }
    }

    fn with_capacity(&self) -> Self {
        let mut node_states = self.node_states.clone();
        for node_state in node_states.iter_mut() {
            node_state.reset();
        }

        let mut edge_states = Vec::with_capacity(self.edge_states.len());
        for _ in 0..self.edge_states.len() {
            edge_states.push(EdgeState::new())
        }

        Self {
            node_states,
            edge_states,
        }
    }

    fn add_flow(&mut self, edge: &Edge, flow: f64, timestep: &Timestep) -> Result<(), PywrError> {
        match self.node_states.get_mut(edge.from_node_index) {
            Some(s) => s.add_out_flow(flow, timestep),
            None => return Err(PywrError::NodeIndexNotFound),
        };

        match self.node_states.get_mut(edge.to_node_index) {
            Some(s) => s.add_in_flow(flow, timestep),
            None => return Err(PywrError::NodeIndexNotFound),
        };

        match self.edge_states.get_mut(edge.index) {
            Some(s) => s.add_flow(flow),
            None => return Err(PywrError::EdgeIndexNotFound),
        };

        Ok(())
    }
}

// The current state of a model
// pub struct ModelState {
//     timestep: Timestep,
//     initial_network_state: NetworkState,
//     final_network_state: NetworkState,
//     parameter_states: ParameterState,
// }
//
// impl ModelState {
//     fn new(timestep: Timestep) -> Self {
//         Self {
//             timestep,
//             initial_network_state: NetworkState::new(),
//             final_network_state: NetworkState::new(),
//             parameter_states: Vec::new(),
//         }
//     }

// fn from_initial_network_state(timestep: Timestep, initial_state: NetworkState, nparameters: usize) -> Self {
//     let final_network_state = NetworkState::with_capacity(&initial_state);
//     Self {
//         timestep,
//         initial_network_state: initial_state,
//         final_network_state,
//         parameter_states: Vec::with_capacity(nparameters),
//     }
// }

// /// Create a new ModelState from a previous state
// ///
// /// This method clones the final state of the self as the initial
// /// state in the returned `ModelState`.
// fn step(&self, timestep: Timestep) -> Self {
//     Self {
//         timestep,
//         initial_network_state: self.final_network_state.clone(),
//         final_network_state: NetworkState::with_capacity(&self.final_network_state),
//         parameter_states: Vec::with_capacity(self.parameter_states.len()),
//     }
// }

//     /// Return a parameter's current value
//     pub fn get_parameter_value(&self, index: &ParameterIndex) -> Result<f64, PywrError> {
//         match self.parameter_states.get(index.clone()) {
//             Some(v) => Ok(*v),
//             None => return Err(PywrError::ParameterIndexNotFound),
//         }
//     }
// }

#[derive(Clone, Debug)]
pub struct ScenarioGroup {
    name: String,
    size: usize,
    // TODO labels
    // labels: Option<Vec<String>>
}

impl ScenarioGroup {
    fn new(name: &str, size: usize) -> Self {
        Self {
            name: name.to_string(),
            size,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ScenarioGroupCollection {
    groups: Vec<ScenarioGroup>,
    next_index: Option<ScenarioIndex>,
}

impl ScenarioGroupCollection {
    fn new() -> Self {
        Self {
            groups: Vec::new(),
            next_index: None,
        }
    }

    /// Add a `ScenarioGroup` to the collection
    fn add_group(&mut self, name: &str, size: usize) {
        // TODO error with duplicate names
        self.groups.push(ScenarioGroup::new(name, size));
    }

    /// Return a vector of `ScenarioIndex`s for all combinations of the groups.
    fn scenario_indices(&self) -> Vec<ScenarioIndex> {
        let num: usize = self.groups.iter().map(|grp| grp.size).product();
        let mut scenario_indices: Vec<ScenarioIndex> = Vec::with_capacity(num);

        for scenario_id in 0..num {
            let mut remaining = scenario_id;
            let mut indices: Vec<usize> = Vec::with_capacity(self.groups.len());
            for grp in self.groups.iter().rev() {
                let idx = remaining % grp.size;
                remaining = remaining / grp.size;
                indices.push(idx);
            }
            indices.reverse();
            scenario_indices.push(ScenarioIndex {
                index: scenario_id,
                indices: indices,
            });
        }
        scenario_indices
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioIndex {
    index: usize,
    indices: Vec<usize>,
}

#[derive(Debug, Copy, Clone)]
pub struct Timestep {
    date: NaiveDate,
    index: TimestepIndex,
    duration: ChronoDuration,
}

impl Timestep {
    fn days(&self) -> f64 {
        self.duration.num_seconds() as f64 / 3600.0 / 24.0
    }
}

impl Add<ChronoDuration> for Timestep {
    type Output = Timestep;

    fn add(self, other: ChronoDuration) -> Self {
        Self {
            date: self.date + other,
            index: self.index + 1,
            duration: other,
        }
    }
}

#[derive(Debug)]
pub struct Timestepper {
    start: NaiveDate,
    end: NaiveDate,
    timestep: ChronoDuration,
}

impl Timestepper {
    fn new(start: &str, end: &str, fmt: &str, timestep: i64) -> Result<Self, PywrError> {
        Ok(Self {
            start: NaiveDate::parse_from_str(start, fmt)?,
            end: NaiveDate::parse_from_str(end, fmt)?,
            timestep: ChronoDuration::days(timestep),
        })
    }

    /// Create a vector of `Timestep`s between the start and end dates at the given duration.
    fn timesteps(&self) -> Vec<Timestep> {
        let mut timesteps: Vec<Timestep> = Vec::new();
        let mut current = Timestep {
            date: self.start.clone(),
            index: 0,
            duration: self.timestep.clone(),
        };

        while current.date <= self.end {
            let next = current + self.timestep;
            timesteps.push(current);
            current = next;
        }
        timesteps
    }
}

pub struct Model {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    parameters: Vec<Box<dyn parameters::Parameter>>,
    scenarios: ScenarioGroupCollection,
}

// Required for Python API
unsafe impl Send for Model {}

impl Model {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            parameters: Vec::new(),
            scenarios: ScenarioGroupCollection::new(),
        }
    }

    /// Returns the initial state of the network
    fn get_initial_state(&self, scenario_indices: &Vec<ScenarioIndex>) -> Vec<NetworkState> {
        let mut states : Vec<NetworkState> = Vec::new();

        for _scenario_index in scenario_indices {
            let mut state = NetworkState::new();

            for node in &self.nodes {
                let node_state = match node {
                    Node::Input(_n) => NodeState::Flow(FlowState::new()),
                    Node::Link(_n) => NodeState::Flow(FlowState::new()),
                    Node::Output(_n) => NodeState::Flow(FlowState::new()),
                    // TODO initial volume
                    Node::Storage(_n) => NodeState::Storage(StorageState::new(0.0)),
                };

                state.node_states.push(node_state);
            }

            for _edge in &self.edges {
                state.edge_states.push(EdgeState::new());
            }

            states.push(state)
        }
        states
    }

    pub fn run(
        &self,
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

        // Step a timestep
        for timestep in timesteps.iter() {
            let next_states = self.step(timestep, &scenario_indices, solver, &current_states)?;
            current_states = next_states;
            count += scenario_indices.len();
        }
        println!("speed: {} ts/s", count as f64 / now.elapsed().as_secs_f64());
        // println!("final state: {:?}", initial_state);

        Ok(())
    }

    /// Perform a single timestep with the current state, and return the updated states.
    fn step(
        &self,
        timestep: &Timestep,
        scenario_indices: &Vec<ScenarioIndex>,
        solver: &mut Box<dyn Solver>,
        current_states: &Vec<NetworkState>,
    ) -> Result<Vec<NetworkState>, PywrError> {

        let mut next_states = Vec::with_capacity(current_states.len());

        for scenario_index in scenario_indices.iter() {
            let current_state = match current_states.get(scenario_index.index) {
                Some(s) => s,
                None => return Err(PywrError::ScenarioStateNotFound)
            };
            let pstate = self.compute_parameters(&timestep, &scenario_index, current_state)?;

            let next_state = solver.solve(&self, timestep, current_state, &pstate)?;

            next_states.push(next_state);
        }

        Ok(next_states)
    }

    fn compute_parameters(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        state: &NetworkState,
    ) -> Result<ParameterState, PywrError> {
        let mut parameter_state = ParameterState::with_capacity(self.parameters.len());
        for parameter in &self.parameters {
            let value = parameter.compute(timestep, scenario_index, state, &parameter_state)?;
            parameter_state.push(value);
        }

        Ok(parameter_state)
    }

    // TODO do these with macros??
    /// Get a NodeIndex from a node's name
    pub fn get_node_index(&self, name: &str) -> Result<NodeIndex, PywrError> {
        match self.nodes.iter().find(|&n| n.name() == name) {
            Some(node) => Ok(node.index().clone()),
            None => Err(PywrError::NodeIndexNotFound),
        }
    }

    /// Get a `ParameterIndex` from a parameter's name
    pub fn get_parameter_index(&self, name: &str) -> Result<ParameterIndex, PywrError> {
        match self.parameters.iter().position(|p| p.meta().name == name) {
            Some(idx) => Ok(idx),
            None => Err(PywrError::ParameterIndexNotFound),
        }
    }

    /// Add a new Node::Input to the model.
    pub fn add_input_node(&mut self, name: &str) -> Result<NodeIndex, PywrError> {
        // Check for name.
        if let Ok(idx) = self.get_node_index(name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string(), idx.clone()));
        }

        // Now add the node to the network.
        let node_index = self.nodes.len();
        let node = Node::Input(InputNode::new(&node_index, name));
        self.nodes.push(node);
        Ok(node_index)
    }

    /// Add a new Node::Link to the model.
    pub fn add_link_node(&mut self, name: &str) -> Result<NodeIndex, PywrError> {
        // Check for name.
        if let Ok(idx) = self.get_node_index(name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string(), idx.clone()));
        }

        // Now add the node to the network.
        let node_index = self.nodes.len();
        let node = Node::Link(LinkNode::new(&node_index, name));
        self.nodes.push(node);
        Ok(node_index)
    }

    /// Add a new Node::Link to the model.
    pub fn add_output_node(&mut self, name: &str) -> Result<NodeIndex, PywrError> {
        // Check for name.
        if let Ok(idx) = self.get_node_index(name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string(), idx.clone()));
        }

        // Now add the node to the network.
        let node_index = self.nodes.len();
        let node = Node::Output(OutputNode::new(&node_index, name));
        self.nodes.push(node);
        Ok(node_index)
    }

    /// Add a new Node::Link to the model.
    pub fn add_storage_node(&mut self, name: &str) -> Result<NodeIndex, PywrError> {
        // Check for name.
        if let Ok(idx) = self.get_node_index(name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string(), idx.clone()));
        }

        // Now add the node to the network.
        let node_index = self.nodes.len();
        let node = Node::Storage(StorageNode::new(&node_index, name));
        self.nodes.push(node);
        Ok(node_index)
    }

    /// Add a `parameters::Parameter` to the model
    pub fn add_parameter(&mut self, parameter: Box<dyn parameters::Parameter>) -> Result<ParameterIndex, PywrError> {
        if let Ok(idx) = self.get_parameter_index(&parameter.meta().name) {
            return Err(PywrError::ParameterNameAlreadyExists(
                parameter.meta().name.to_string(),
                idx.clone(),
            ));
        }

        let parameter_index = self.parameters.len();
        self.parameters.push(parameter);

        Ok(parameter_index)
    }

    /// Set a constraint on a node.
    fn set_node_constraint(
        &mut self,
        node_idx: NodeIndex,
        parameter_idx: Option<ParameterIndex>,
        constraint: Constraint,
    ) -> Result<(), PywrError> {
        if let Some(idx) = parameter_idx {
            if idx >= self.parameters.len() {
                return Err(PywrError::ParameterIndexNotFound);
            }
        }

        match self.nodes.get_mut(node_idx) {
            Some(node) => {
                // Try to add the parameter
                node.set_constraint(parameter_idx, constraint)?;
                Ok(())
            }
            None => Err(PywrError::NodeIndexNotFound),
        }
    }

    /// Set a cost on a node.
    fn set_node_cost(&mut self, node_idx: NodeIndex, parameter_idx: Option<ParameterIndex>) -> Result<(), PywrError> {
        if let Some(idx) = parameter_idx {
            if idx >= self.parameters.len() {
                return Err(PywrError::ParameterIndexNotFound);
            }
        }

        match self.nodes.get_mut(node_idx) {
            Some(node) => {
                // Try to add the parameter
                node.set_cost(parameter_idx)?;
                Ok(())
            }
            None => Err(PywrError::NodeIndexNotFound),
        }
    }

    /// Connect two nodes together
    fn connect_nodes(&mut self, from_node_index: NodeIndex, to_node_index: NodeIndex) -> Result<EdgeIndex, PywrError> {
        // Next edge index
        // let edge_index = self.edges.len();
        // TODO check the an edge with these indices doesn't already exist.

        // We need to get a mutable reference for each node.
        // The compiler needs to know these are not to the same element. We use split_at_mut to
        // give two mutable slices to the nodes array depending on the ordering of the indexes.
        let (from_node, to_node) = match from_node_index.cmp(&to_node_index) {
            Ordering::Less => {
                if to_node_index > self.nodes.len() {
                    return Err(PywrError::NodeIndexNotFound);
                }
                // Left will contain the "from" node at, and
                // right will contain the "to" node as the first index.
                let (left, right) = self.nodes.split_at_mut(to_node_index);
                (&mut left[from_node_index], &mut right[0])
            }
            Ordering::Equal => return Err(PywrError::InvalidNodeConnection),
            Ordering::Greater => {
                if from_node_index > self.nodes.len() {
                    return Err(PywrError::NodeIndexNotFound);
                }
                // Left will contain the "to" node, and
                // right will contain the "from" node as the first index.
                let (left, right) = self.nodes.split_at_mut(from_node_index);
                (&mut right[0], &mut left[to_node_index])
            }
        };

        // Next edge index
        let edge_index = self.edges.len() as EdgeIndex;
        let edge = from_node.connect(to_node, &edge_index)?;
        self.edges.push(edge);

        Ok(edge_index)
    }

    /// Add a scenario to the model.
    pub fn add_scenario(&mut self, name: &str, size: usize) -> Result<(), PywrError> {
        self.scenarios.add_group(name, size);
        Ok(())
    }
}

pub trait Solver {
    fn setup(&mut self, model: &Model) -> Result<(), PywrError>;
    fn solve(
        &mut self,
        model: &Model,
        timestep: &Timestep,
        network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<NetworkState, PywrError>;
}

struct GlpkSolver {
    problem: glpk::GlpProb,
    start_node_constraints: Option<usize>,
}

impl GlpkSolver {
    fn new() -> Result<Self, PywrError> {
        Ok(Self {
            problem: glpk::GlpProb::create(glpk::Direction::Min)?,
            start_node_constraints: None,
        })
    }

    /// Create a column for each edge
    fn create_columns(&mut self, model: &Model) -> Result<(), PywrError> {
        // One column per edge
        let ncols = model.edges.len();
        if ncols < 1 {
            return Err(PywrError::NoEdgesDefined);
        }
        self.problem.add_columns(ncols)?;

        // Explicitly set the columns as x >= 0.0 (i.e. no upper bounds)
        for i in 0..ncols {
            self.problem.set_col_bounds(i, glpk::Bounds::Lower(0.0))?;
        }

        Ok(())
    }

    /// Create mass balance constraints for each edge
    fn create_mass_balance_constraints(&mut self, model: &Model) -> Result<(), PywrError> {
        for node in &model.nodes {
            // Only link nodes create mass-balance constraints
            if let Node::Link(link) = node {
                // Create empty arrays to store the matrix data
                let mut indices: Vec<usize> = Vec::new();
                let mut values: Vec<f64> = Vec::new();

                for &edge_index in &link.incoming_edges {
                    indices.push(edge_index);
                    values.push(1.0);
                }
                for &edge_index in &link.outgoing_edges {
                    indices.push(edge_index);
                    values.push(-1.0);
                }

                let row = self.problem.add_rows(1)?;
                self.problem.set_matrix_row(row, &indices, &values)?;
                // Fix the row to make incoming = outgoing.
                self.problem.set_row_bounds(row, glpk::Bounds::Fixed(0.0))?;
            }
        }
        Ok(())
    }

    /// Create node constraints
    ///
    /// One constraint is created per node to enforce any constraints (flow or storage)
    /// that it may define.
    fn create_node_constraints(&mut self, model: &Model) -> Result<(), PywrError> {
        let start_row = self.problem.add_rows(model.nodes.len())?;
        for node in &model.nodes {
            // Create empty arrays to store the matrix data
            let (indices, values): (Vec<usize>, Vec<f64>) = match node {
                Node::Link(link) => (
                    link.outgoing_edges.clone(),
                    link.outgoing_edges.iter().map(|_| 1.0).collect(),
                ),
                Node::Input(input) => (
                    input.outgoing_edges.clone(),
                    input.outgoing_edges.iter().map(|_| 1.0).collect(),
                ),
                Node::Output(output) => (
                    output.incoming_edges.clone(),
                    output.incoming_edges.iter().map(|_| 1.0).collect(),
                ),
                Node::Storage(storage) => {
                    let mut indices: Vec<usize> = Vec::new();
                    let mut values: Vec<f64> = Vec::new();

                    for &edge_index in &storage.incoming_edges {
                        indices.push(edge_index);
                        values.push(1.0);
                    }
                    for &edge_index in &storage.outgoing_edges {
                        indices.push(edge_index);
                        values.push(-1.0);
                    }

                    (indices, values)
                }
            };
            let row = start_row + *node.index();

            self.problem.set_matrix_row(row, &indices, &values)?;
            // Initially fix the bounds to zero; these constraints will be updated during
            // each timestep.
            self.problem.set_row_bounds(row, glpk::Bounds::Fixed(0.0))?;
            self.start_node_constraints = Some(start_row);
        }

        Ok(())
    }

    /// Update edge objective coefficients
    fn update_edge_objectives(&mut self, model: &Model, parameter_states: &ParameterState) -> Result<(), PywrError> {
        for edge in &model.edges {
            let cost: f64 = edge.cost(model, parameter_states)?;
            self.problem.set_obj_coefficient(edge.index, cost)?;
        }
        Ok(())
    }

    /// Flow constraint to
    fn flow_constraints_to_bounds(
        &self,
        flow_constraints: &FlowConstraints,
        parameter_states: &ParameterState,
    ) -> Result<glpk::Bounds, PywrError> {
        // minimum flow defaults to zero if undefined.
        let min_flow = match flow_constraints.min_flow {
            Some(vol_idx) => match parameter_states.get(vol_idx) {
                Some(v) => *v,
                None => return Err(PywrError::ParameterIndexNotFound),
            },
            None => 0.0,
        };

        let bounds = match flow_constraints.max_flow {
            Some(vol_idx) => {
                // max flow is defined.
                let max_flow = match parameter_states.get(vol_idx) {
                    Some(v) => *v,
                    None => return Err(PywrError::ParameterIndexNotFound),
                };

                // TODO error if min_flow > max_flow
                if (max_flow - min_flow).abs() < 1e-6 {
                    // Very close; assume equality
                    glpk::Bounds::Fixed(max_flow)
                } else {
                    glpk::Bounds::Double(min_flow, max_flow)
                }
            }
            None => glpk::Bounds::Lower(min_flow),
        };

        Ok(bounds)
    }

    fn storage_constraints_to_bounds(
        &self,
        current_volume: &f64,
        timestep: &Timestep,
        storage_constraints: &StorageConstraints,
        parameter_states: &ParameterState,
    ) -> Result<glpk::Bounds, PywrError> {
        let min_vol = match storage_constraints.min_volume {
            Some(vol_idx) => match parameter_states.get(vol_idx) {
                Some(v) => *v,
                None => return Err(PywrError::ParameterIndexNotFound),
            },
            None => 0.0,
        };
        let max_vol = match storage_constraints.max_volume {
            Some(vol_idx) => match parameter_states.get(vol_idx) {
                Some(v) => *v,
                None => return Err(PywrError::ParameterIndexNotFound),
            },
            None => 0.0,
        };

        let dt = timestep.days();
        let lb = -(current_volume - min_vol).min(0.0) / dt;
        let ub = (max_vol - current_volume).max(0.0) / dt;

        if (ub - lb).abs() < 1e-6 {
            // Very close; assume equality
            Ok(glpk::Bounds::Fixed(ub))
        } else {
            Ok(glpk::Bounds::Double(lb, ub))
        }
    }

    /// Update node constraints
    fn update_node_constraint_bounds(
        &mut self,
        model: &Model,
        timestep: &Timestep,
        network_state: &NetworkState,
        parameter_states: &ParameterState,
    ) -> Result<(), PywrError> {
        let start_row = match self.start_node_constraints {
            Some(r) => r,
            None => return Err(PywrError::SolverNotSetup),
        };

        for node in &model.nodes {
            let bounds = match node {
                Node::Link(n) => self.flow_constraints_to_bounds(&n.flow_constraints, parameter_states)?,
                Node::Input(n) => self.flow_constraints_to_bounds(&n.flow_constraints, parameter_states)?,
                Node::Output(n) => self.flow_constraints_to_bounds(&n.flow_constraints, parameter_states)?,
                Node::Storage(n) => {
                    let current_volume = match network_state.node_states.get(n.meta.index) {
                        Some(s) => match s {
                            NodeState::Storage(s) => &s.volume,
                            _ => return Err(PywrError::NodeIndexNotFound),
                        },
                        None => return Err(PywrError::NodeIndexNotFound),
                    };

                    self.storage_constraints_to_bounds(
                        current_volume,
                        timestep,
                        &n.storage_constraints,
                        parameter_states,
                    )?
                }
            };

            self.problem.set_row_bounds(start_row + node.index(), bounds)?;
        }

        Ok(())
    }
}

impl Solver for GlpkSolver {
    /// Setup the linear programme constraint matrix
    fn setup(&mut self, model: &Model) -> Result<(), PywrError> {
        // Create the columns
        self.create_columns(model)?;
        // Create edge mass balance constraints
        self.create_mass_balance_constraints(model)?;
        // Create the nodal constraints
        self.create_node_constraints(model)?;

        Ok(())
    }

    fn solve(
        &mut self,
        model: &Model,
        timestep: &Timestep,
        network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<NetworkState, PywrError> {
        self.update_edge_objectives(model, parameter_state)?;
        self.update_node_constraint_bounds(model, timestep, network_state, parameter_state)?;

        self.problem.simplex()?;
        // Check solution status
        match self.problem.get_solution_status() {
            glpk::SolutionStatus::Optimal => {},
            _ => return Err(PywrError::SolveFailed)  // TODO more information in this error message
        }

        // Create the updated network state from the results
        let mut new_state = network_state.with_capacity();

        for edge in &model.edges {
            let flow = self.problem.get_col_primal(edge.index)?;
            new_state.add_flow(edge, flow, timestep)?;
        }

        Ok(new_state)
    }
}

/// Python API
///
/// The following structures provide a Python API to access the core model structures.

impl std::convert::From<PywrError> for PyErr {
    fn from(err: PywrError) -> PyErr {
        PyRuntimeError::new_err(err.to_string())
    }
}

#[pyclass]
struct PyModel {
    model: Model,
}

#[pymethods]
impl PyModel {
    #[new]
    fn new() -> Self {
        Self { model: Model::new() }
    }

    fn add_input_node(&mut self, name: &str) -> PyResult<NodeIndex> {
        let idx = self.model.add_input_node(name)?;
        Ok(idx)
    }

    fn add_link_node(&mut self, name: &str) -> PyResult<NodeIndex> {
        let idx = self.model.add_link_node(name)?;
        Ok(idx)
    }

    fn add_output_node(&mut self, name: &str) -> PyResult<NodeIndex> {
        let idx = self.model.add_output_node(name)?;
        Ok(idx)
    }

    fn connect_nodes(&mut self, from_node_name: &str, to_node_name: &str) -> PyResult<EdgeIndex> {
        let from_node_idx = self.model.get_node_index(from_node_name)?;
        let to_node_idx = self.model.get_node_index(to_node_name)?;

        let idx = self.model.connect_nodes(from_node_idx, to_node_idx)?;
        Ok(idx)
    }

    fn run(&mut self, solver_name: &str) -> PyResult<()> {
        let timestepper = Timestepper::new("2020-01-01", "2020-01-31", "%Y-%m-%d", 1)?;
        let mut scenarios = ScenarioGroupCollection::new();
        scenarios.add_group("test-scenario", 5);

        let mut solver: Box<dyn Solver> = match solver_name {
            "glpk" => Box::new(GlpkSolver::new().unwrap()),
            _ => return Err(PyErr::from(PywrError::UnrecognisedSolver)),
        };

        self.model.run(timestepper, scenarios, &mut solver)?;
        Ok(())
    }

    fn set_node_constraint(&mut self, node_name: &str, parameter_name: &str) -> PyResult<()> {
        let node_idx = self.model.get_node_index(node_name)?;
        let parameter_idx = self.model.get_parameter_index(parameter_name)?;
        // TODO support setting other constraints
        self.model
            .set_node_constraint(node_idx, Some(parameter_idx), Constraint::MaxFlow)?;
        Ok(())
    }

    fn set_node_cost(&mut self, node_name: &str, parameter_name: &str) -> PyResult<()> {
        let node_idx = self.model.get_node_index(node_name)?;
        let parameter_idx = self.model.get_parameter_index(parameter_name)?;

        self.model.set_node_cost(node_idx, Some(parameter_idx))?;
        Ok(())
    }

    /// Add a Python object as a parameter.
    fn add_python_parameter(&mut self, name: &str, object: PyObject) -> PyResult<ParameterIndex> {
        let parameter = parameters::py::PyParameter::new(name, object);
        let idx = self.model.add_parameter(Box::new(parameter))?;
        Ok(idx)
    }

    fn add_constant(&mut self, name: &str, value: f64) -> PyResult<ParameterIndex> {
        let parameter = parameters::ConstantParameter::new(name, value);
        let idx = self.model.add_parameter(Box::new(parameter))?;
        Ok(idx)
    }
}

/// A Python module implemented in Rust.
#[pymodule]
fn pywr(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyModel>()?;
    // m.add_function(wrap_pyfunction!(sum_as_string, m)?)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_timestepper() -> Timestepper {
        Timestepper::new("2020-01-01", "2020-01-05", "%Y-%m-%d", 1).unwrap()
    }

    fn default_scenarios() -> ScenarioGroupCollection {
        let mut scenarios = ScenarioGroupCollection::new();
        scenarios.add_group("test-scenario", 10);
        scenarios
    }

    #[test]
    fn test_simple_model() {
        let mut model = Model::new();

        let input_node_idx = model.add_input_node("input").unwrap();
        let link_node_idx = model.add_link_node("link").unwrap();
        let output_node_idx = model.add_output_node("output").unwrap();

        assert_eq!(input_node_idx, 0);
        assert_eq!(link_node_idx, 1);
        assert_eq!(output_node_idx, 2);

        let edge_idx = model.connect_nodes(input_node_idx, link_node_idx).unwrap();
        assert_eq!(edge_idx, 0);
        let edge_idx = model.connect_nodes(link_node_idx, output_node_idx).unwrap();
        assert_eq!(edge_idx, 1);

        // Now assert the internal instructure is as expected.
        if let Node::Input(node) = model.nodes.get(input_node_idx).unwrap() {
            assert_eq!(node.outgoing_edges.len(), 1);
        } else {
            assert!(false, "Incorrect node type returned.")
        };

        if let Node::Link(node) = model.nodes.get(link_node_idx).unwrap() {
            assert_eq!(node.incoming_edges.len(), 1);
            assert_eq!(node.outgoing_edges.len(), 1);
        } else {
            assert!(false, "Incorrect node type returned.")
        };

        if let Node::Output(node) = model.nodes.get(output_node_idx).unwrap() {
            assert_eq!(node.incoming_edges.len(), 1);
        } else {
            assert!(false, "Incorrect node type returned.")
        };
    }

    #[test]
    /// Test the duplicate node names are not permitted.
    fn test_duplicate_node_name() {
        let mut model = Model::new();

        model.add_input_node("my-node").unwrap();
        // Second add with the same name
        let node_idx = model.add_input_node("my-node");
        assert_eq!(
            node_idx,
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string(), 0))
        );
        let node_idx = model.add_link_node("my-node");
        assert_eq!(
            node_idx,
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string(), 0))
        );
        let node_idx = model.add_output_node("my-node");
        assert_eq!(
            node_idx,
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string(), 0))
        );
        let node_idx = model.add_storage_node("my-node");
        assert_eq!(
            node_idx,
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string(), 0))
        );
    }

    fn simple_model() -> Model {
        let mut model = Model::new();

        let input_node_idx = model.add_input_node("input").unwrap();
        let link_node_idx = model.add_link_node("link").unwrap();
        let output_node_idx = model.add_output_node("output").unwrap();

        model.connect_nodes(input_node_idx, link_node_idx).unwrap();
        model.connect_nodes(link_node_idx, output_node_idx).unwrap();

        let inflow = parameters::VectorParameter::new("inflow", vec![10.0; 366]);
        let inflow_idx = model.add_parameter(Box::new(inflow)).unwrap();
        model
            .set_node_constraint(input_node_idx, Some(inflow_idx), Constraint::MaxFlow)
            .unwrap();

        let base_demand = parameters::ConstantParameter::new("base-demand", 10.0);
        let base_demand_idx = model.add_parameter(Box::new(base_demand)).unwrap();

        let demand_factor = parameters::ConstantParameter::new("demand-factor", 1.2);
        let demand_factor_idx = model.add_parameter(Box::new(demand_factor)).unwrap();

        let total_demand = parameters::AggregatedParameter::new(
            "total-demand",
            vec![base_demand_idx, demand_factor_idx],
            parameters::AggFunc::Product,
        );
        let total_demand_idx = model.add_parameter(Box::new(total_demand)).unwrap();

        model
            .set_node_constraint(output_node_idx, Some(total_demand_idx), Constraint::MaxFlow)
            .unwrap();

        let demand_cost = parameters::ConstantParameter::new("demand-cost", -10.0);
        let demand_cost_idx = model.add_parameter(Box::new(demand_cost)).unwrap();
        model.set_node_cost(output_node_idx, Some(demand_cost_idx)).unwrap();

        model
    }

    #[test]
    /// Test adding a constant parameter to a model.
    fn test_constant_parameter() {
        let mut model = Model::new();
        let node_idx = model.add_input_node("input").unwrap();

        let input_max_flow = parameters::ConstantParameter::new("my-constant", 10.0);
        let param_idx = model.add_parameter(Box::new(input_max_flow)).unwrap();
        assert_eq!(param_idx, 0);
        // assign the new parameter to one of the nodes.
        model
            .set_node_constraint(node_idx, Some(param_idx), Constraint::MaxFlow)
            .unwrap();

        // Try to assign a constraint not defined for particular node type
        assert_eq!(
            model.set_node_constraint(node_idx, Some(param_idx), Constraint::MaxVolume),
            Err(PywrError::StorageConstraintsUndefined)
        )
    }

    #[test]
    fn test_step() {
        let model = simple_model();
        let mut timestepper = default_timestepper();
        let mut scenarios = default_scenarios();
        let mut solver: Box<dyn Solver> = Box::new(GlpkSolver::new().unwrap());

        solver.setup(&model).unwrap();

        let timesteps = timestepper.timesteps();
        let mut ts_iter = timesteps.iter();
        let scenario_indices = scenarios.scenario_indices();
        let ts = ts_iter.next().unwrap();
        let current_state = model.get_initial_state(&scenario_indices);
        assert_eq!(current_state.len(), scenario_indices.len());

        let next_state = model.step(ts, &scenario_indices, &mut solver, &current_state).unwrap();

        assert_eq!(next_state.len(), scenario_indices.len());

        let output_node_idx = model.get_node_index("output").unwrap();

        let state0 = next_state.get(0).unwrap();
        let output_state = state0.node_states.get(output_node_idx).unwrap();
        match output_state {
            NodeState::Flow(fs) => assert_eq!(fs.in_flow, 10.0),
            _ => assert!(false)
        };

    }

    #[test]
    fn test_run() {
        let model = simple_model();
        let mut timestepper = default_timestepper();
        let mut scenarios = default_scenarios();
        let mut solver: Box<dyn Solver> = Box::new(GlpkSolver::new().unwrap());

        model.run(timestepper, scenarios, &mut solver).unwrap();
        // TODO test results
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
