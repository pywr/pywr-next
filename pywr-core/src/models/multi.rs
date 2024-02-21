use crate::metric::Metric;
use crate::models::ModelDomain;
use crate::network::{Network, NetworkState, RunTimings};
use crate::scenario::ScenarioIndex;
use crate::solvers::{Solver, SolverSettings};
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;
use std::cmp::Ordering;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::time::Instant;

/// An index to another model
///
/// The index is to either a model evaluated before this model, or after this model.
enum OtherNetworkIndex {
    Before(NonZeroUsize),
    After(NonZeroUsize),
}

impl OtherNetworkIndex {
    fn new(from_idx: usize, to_idx: usize) -> Self {
        match from_idx.cmp(&to_idx) {
            Ordering::Equal => panic!("Cannot create OtherNetworkIndex to self."),
            Ordering::Less => Self::Before(NonZeroUsize::new(to_idx - from_idx).unwrap()),
            Ordering::Greater => Self::After(NonZeroUsize::new(from_idx - to_idx).unwrap()),
        }
    }
}

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct MultiNetworkTransferIndex(pub usize);

impl Deref for MultiNetworkTransferIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for MultiNetworkTransferIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A special parameter that retrieves a value from a metric in another model.
struct MultiNetworkTransfer {
    /// The model to get the value from.
    from_model_idx: OtherNetworkIndex,
    /// The metric to get the value from.
    from_metric: Metric,
    /// Optional initial value to use on the first time-step
    initial_value: Option<f64>,
}

struct MultiNetworkEntry {
    name: String,
    network: Network,
    parameters: Vec<MultiNetworkTransfer>,
}

pub struct MultiNetworkModelState<S> {
    current_time_step_idx: usize,
    states: Vec<NetworkState>,
    recorder_states: Vec<Vec<Option<Box<dyn Any>>>>,
    solvers: Vec<S>,
}

/// A MultiNetwork is a collection of models that can be run together.
pub struct MultiNetworkModel {
    domain: ModelDomain,
    networks: Vec<MultiNetworkEntry>,
}

impl MultiNetworkModel {
    pub fn new(domain: ModelDomain) -> Self {
        Self {
            domain,
            networks: Vec::new(),
        }
    }

    /// Get a reference to the [`ModelDomain`]
    pub fn domain(&self) -> &ModelDomain {
        &self.domain
    }

    /// Get a reference to a network by index.
    pub fn network(&self, idx: usize) -> Result<&Network, PywrError> {
        self.networks
            .get(idx)
            .map(|n| &n.network)
            .ok_or(PywrError::NetworkIndexNotFound(idx))
    }

    /// Get a mutable reference to a network by index.
    pub fn network_mut(&mut self, idx: usize) -> Result<&mut Network, PywrError> {
        self.networks
            .get_mut(idx)
            .map(|n| &mut n.network)
            .ok_or(PywrError::NetworkIndexNotFound(idx))
    }

    /// Get the index of a network by name.
    pub fn get_network_index_by_name(&self, name: &str) -> Result<usize, PywrError> {
        self.networks
            .iter()
            .position(|n| n.name == name)
            .ok_or(PywrError::NetworkNotFound(name.to_string()))
    }

    pub fn add_network(&mut self, name: &str, network: Network) -> usize {
        // TODO check for duplicate names
        let idx = self.networks.len();
        self.networks.push(MultiNetworkEntry {
            name: name.to_string(),
            network,
            parameters: Vec::new(),
        });

        idx
    }

    /// Add a transfer of data from one network to another.
    pub fn add_inter_network_transfer(
        &mut self,
        from_network_idx: usize,
        from_metric: Metric,
        to_network_idx: usize,
        initial_value: Option<f64>,
    ) {
        let parameter = MultiNetworkTransfer {
            from_model_idx: OtherNetworkIndex::new(from_network_idx, to_network_idx),
            from_metric,
            initial_value,
        };

        self.networks[to_network_idx].parameters.push(parameter);
    }

    pub fn setup<S>(&self, settings: &S::Settings) -> Result<MultiNetworkModelState<Vec<Box<S>>>, PywrError>
    where
        S: Solver,
    {
        let timesteps = self.domain.time.timesteps();
        let scenario_indices = self.domain.scenarios.indices();

        let mut states = Vec::with_capacity(self.networks.len());
        let mut recorder_states = Vec::with_capacity(self.networks.len());
        let mut solvers = Vec::with_capacity(self.networks.len());

        for entry in &self.networks {
            let state = entry
                .network
                .setup_network(timesteps, scenario_indices, entry.parameters.len())?;
            let recorder_state = entry.network.setup_recorders(&self.domain)?;
            let solver = entry.network.setup_solver::<S>(scenario_indices, settings)?;

            states.push(state);
            recorder_states.push(recorder_state);
            solvers.push(solver);
        }

        Ok(MultiNetworkModelState {
            current_time_step_idx: 0,
            states,
            recorder_states,
            solvers,
        })
    }

    /// Compute inter model transfers
    fn compute_inter_network_transfers(
        &self,
        model_idx: usize,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        states: &mut [NetworkState],
    ) -> Result<(), PywrError> {
        // Get references to the models before and after this model
        let (before_models, after_models) = self.networks.split_at(model_idx);
        let (this_model, after_models) = after_models.split_first().unwrap();
        // Get references to the states before and after this model
        let (before, after) = states.split_at_mut(model_idx);
        let (this_models_state, after) = after.split_first_mut().unwrap();

        // Compute inter-model transfers for all scenarios
        for scenario_index in scenario_indices.iter() {
            compute_inter_network_transfers(
                timestep,
                scenario_index,
                &this_model.parameters,
                this_models_state,
                before_models,
                before,
                after_models,
                after,
            )?;
        }

        Ok(())
    }

    /// Perform a single time-step of the multi1-model.
    pub fn step<S>(&self, state: &mut MultiNetworkModelState<Vec<Box<S>>>) -> Result<(), PywrError>
    where
        S: Solver,
    {
        let mut timings = RunTimings::default();

        let timestep = self
            .domain
            .time
            .timesteps()
            .get(state.current_time_step_idx)
            .ok_or(PywrError::EndOfTimesteps)?;

        let scenario_indices = self.domain.scenarios.indices();

        for (idx, entry) in self.networks.iter().enumerate() {
            // Perform inter-model state updates
            self.compute_inter_network_transfers(idx, timestep, scenario_indices, &mut state.states)?;

            let sub_model_solvers = state.solvers.get_mut(idx).unwrap();
            let sub_model_states = state.states.get_mut(idx).unwrap();

            // Perform sub-model step
            entry
                .network
                .step(
                    timestep,
                    scenario_indices,
                    sub_model_solvers,
                    sub_model_states,
                    &mut timings,
                )
                .unwrap();

            let start_r_save = Instant::now();

            let sub_model_recorder_states = state.recorder_states.get_mut(idx).unwrap();

            entry
                .network
                .save_recorders(timestep, scenario_indices, sub_model_states, sub_model_recorder_states)?;
            timings.recorder_saving += start_r_save.elapsed();
        }

        // Finally increment the time-step index
        state.current_time_step_idx += 1;

        Ok(())
    }

    /// Run the model through the given time-steps.
    ///
    /// This method will setup state and solvers, and then run the model through the time-steps.
    pub fn run<S>(&self, settings: &S::Settings) -> Result<(), PywrError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        let mut state = self.setup::<S>(settings)?;

        self.run_with_state::<S>(&mut state, settings)?;

        Ok(())
    }

    /// Run the model with the provided states and solvers.
    pub fn run_with_state<S>(
        &self,
        state: &mut MultiNetworkModelState<Vec<Box<S>>>,
        _settings: &S::Settings,
    ) -> Result<(), PywrError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        let mut timings = RunTimings::default();
        let mut count = 0;

        // TODO: Setup thread pool if running in parallel

        loop {
            match self.step::<S>(state) {
                Ok(_) => {}
                Err(PywrError::EndOfTimesteps) => break,
                Err(e) => return Err(e),
            }

            count += self.domain.scenarios.indices().len();
        }

        for (idx, entry) in self.networks.iter().enumerate() {
            let sub_model_recorder_states = state.recorder_states.get_mut(idx).unwrap();
            entry.network.finalise(sub_model_recorder_states)?;
        }
        // End the global timer and print the run statistics
        timings.finish(count);
        timings.print_table();

        Ok(())
    }
}

/// Calculate inter-model parameters for the given scenario index.
///
///
fn compute_inter_network_transfers(
    timestep: &Timestep,
    scenario_index: &ScenarioIndex,
    inter_network_transfers: &[MultiNetworkTransfer],
    state: &mut NetworkState,
    before_models: &[MultiNetworkEntry],
    before_states: &[NetworkState],
    after_models: &[MultiNetworkEntry],
    after_states: &[NetworkState],
) -> Result<(), PywrError> {
    // Iterate through all of the inter-model transfers
    for (idx, parameter) in inter_network_transfers.iter().enumerate() {
        // Determine which model and state we are getting the value from
        let (other_model, other_model_state) = match parameter.from_model_idx {
            OtherNetworkIndex::Before(i) => {
                let rev_i = before_states.len() - i.get();
                (&before_models[rev_i], &before_states[rev_i])
            }
            OtherNetworkIndex::After(i) => (&after_models[i.get() - 1], &after_states[i.get() - 1]),
        };

        let value = match timestep.is_first().then(|| parameter.initial_value).flatten() {
            // Use the initial value if it is given and it is the first time-step.
            Some(initial_value) => initial_value,
            // Otherwise, get the value from the other model's state/metric
            None => parameter
                .from_metric
                .get_value(&other_model.network, other_model_state.state(scenario_index))?,
        };

        state
            .state_mut(scenario_index)
            .set_inter_network_transfer_value(MultiNetworkTransferIndex(idx), value)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::MultiNetworkModel;
    use crate::models::ModelDomain;
    use crate::network::Network;
    use crate::scenario::ScenarioGroupCollection;
    use crate::solvers::ClpSolver;
    use crate::test_utils::{default_timestepper, simple_network};

    /// Test basic [`MultiNetworkModel`] functionality by running two independent models.
    #[test]
    fn test_multi_model_step() {
        // Create two simple models
        let timestepper = default_timestepper();
        let mut scenario_collection = ScenarioGroupCollection::default();
        scenario_collection.add_group("test-scenario", 2);

        let mut multi_model = MultiNetworkModel::new(ModelDomain::from(timestepper, scenario_collection).unwrap());

        let test_scenario_group_idx = multi_model
            .domain()
            .scenarios
            .group_index("test-scenario")
            .expect("Scenario group not found.");

        let mut network1 = Network::default();
        simple_network(&mut network1, test_scenario_group_idx, 2);

        let mut network2 = Network::default();
        simple_network(&mut network2, test_scenario_group_idx, 2);

        let _network1_idx = multi_model.add_network("network1", network1);
        let _network2_idx = multi_model.add_network("network2", network2);

        let mut state = multi_model
            .setup::<ClpSolver>(&Default::default())
            .expect("Failed to setup multi1-model.");

        multi_model.step(&mut state).expect("Failed to step multi1-model.")
    }
}
