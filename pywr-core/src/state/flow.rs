/// A struct to hold the flow state of a node or link in the model.
#[derive(Clone, Copy, Debug, Default)]
pub struct FlowState {
    /// The total inflow
    pub in_flow: f64,
    /// The total outflow
    pub out_flow: f64,
}

impl FlowState {
    pub fn new() -> Self {
        Self {
            in_flow: 0.0,
            out_flow: 0.0,
        }
    }

    /// Reset the flow state to zero.
    pub fn reset(&mut self) {
        self.in_flow = 0.0;
        self.out_flow = 0.0;
    }

    /// Add to the inflow.
    pub fn add_in_flow(&mut self, flow: f64) {
        self.in_flow += flow;
    }

    /// Add to the outflow.
    pub fn add_out_flow(&mut self, flow: f64) {
        self.out_flow += flow;
    }
}
