use crate::edge::Edge;
use crate::timestep::Timestep;
use crate::PywrError;

#[derive(Clone, Copy, Debug)]
pub enum NodeState {
    Flow(FlowState),
    Storage(StorageState),
}

impl NodeState {
    pub(crate) fn new_flow_state() -> Self {
        Self::Flow(FlowState::new())
    }

    pub(crate) fn new_storage_state(volume: f64) -> Self {
        Self::Storage(StorageState::new(volume))
    }

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
    pub in_flow: f64,
    pub out_flow: f64,
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
    pub(crate) volume: f64,
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
    pub(crate) fn new() -> Self {
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
    pub(crate) node_states: Vec<NodeState>,
    pub(crate) edge_states: Vec<EdgeState>,
}

impl NetworkState {
    pub(crate) fn new() -> Self {
        Self {
            node_states: Vec::new(),
            edge_states: Vec::new(),
        }
    }

    pub(crate) fn with_capacity(&self) -> Self {
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

    pub(crate) fn add_flow(&mut self, edge: &Edge, flow: f64, timestep: &Timestep) -> Result<(), PywrError> {
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
