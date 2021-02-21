use crate::model::Model;
use crate::node::{FlowConstraints, Node, StorageConstraints};
use crate::solvers::Solver;
use crate::timestep::Timestep;
use crate::{glpk, NetworkState, NodeState, ParameterState, PywrError};

pub struct GlpkSolver {
    problem: glpk::GlpProb,
    start_node_constraints: Option<usize>,
}

impl GlpkSolver {
    pub(crate) fn new() -> Result<Self, PywrError> {
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
            glpk::SolutionStatus::Optimal => {}
            _ => return Err(PywrError::SolveFailed), // TODO more information in this error message
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
