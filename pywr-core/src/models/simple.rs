use crate::models::ModelDomain;
use crate::network::{Network, NetworkState, RunTimings};
use crate::solvers::{MultiStateSolver, Solver, SolverSettings};
use crate::PywrError;
use rayon::ThreadPool;
use std::any::Any;
use std::time::Instant;
use tracing::debug;

pub struct ModelState<S> {
    current_time_step_idx: usize,
    state: NetworkState,
    recorder_state: Vec<Option<Box<dyn Any>>>,
    solvers: S,
}

impl<S> ModelState<S> {
    pub fn network_state(&self) -> &NetworkState {
        &self.state
    }

    pub fn network_state_mut(&mut self) -> &mut NetworkState {
        &mut self.state
    }

    pub fn recorder_state(&self) -> &Vec<Option<Box<dyn Any>>> {
        &self.recorder_state
    }
}

/// A standard Pywr model containing a single network.
pub struct Model {
    domain: ModelDomain,
    network: Network,
}

impl Model {
    /// Construct a new model from a [`ModelDomain`] and [`Network`].
    pub fn new(domain: ModelDomain, network: Network) -> Self {
        Self { domain, network }
    }

    /// Get a reference to the [`ModelDomain`]
    pub fn domain(&self) -> &ModelDomain {
        &self.domain
    }

    pub fn network(&self) -> &Network {
        &self.network
    }

    pub fn network_mut(&mut self) -> &mut Network {
        &mut self.network
    }

    /// Check whether a solver [`S`] has the required features to run this model.
    pub fn check_solver_features<S>(&self) -> bool
    where
        S: Solver,
    {
        self.network.check_solver_features::<S>()
    }

    /// Check whether a solver [`S`] has the required features to run this model.
    pub fn check_multi_scenario_solver_features<S>(&self) -> bool
    where
        S: MultiStateSolver,
    {
        self.network.check_multi_scenario_solver_features::<S>()
    }

    pub fn setup<S>(&self, settings: &S::Settings) -> Result<ModelState<Vec<Box<S>>>, PywrError>
    where
        S: Solver,
    {
        let timesteps = self.domain.time.timesteps();
        let scenario_indices = self.domain.scenarios.indices();

        let state = self.network.setup_network(timesteps, scenario_indices, 0)?;
        let recorder_state = self.network.setup_recorders(&self.domain)?;
        let solvers = self.network.setup_solver::<S>(scenario_indices, settings)?;

        Ok(ModelState {
            current_time_step_idx: 0,
            state,
            recorder_state,
            solvers,
        })
    }

    pub fn setup_multi_scenario<S>(&self, settings: &S::Settings) -> Result<ModelState<Box<S>>, PywrError>
    where
        S: MultiStateSolver,
    {
        let timesteps = self.domain.time.timesteps();
        let scenario_indices = self.domain.scenarios.indices();

        let state = self.network.setup_network(timesteps, scenario_indices, 0)?;
        let recorder_state = self.network.setup_recorders(&self.domain)?;
        let solvers = self
            .network
            .setup_multi_scenario_solver::<S>(scenario_indices, settings)?;

        Ok(ModelState {
            current_time_step_idx: 0,
            state,
            recorder_state,
            solvers,
        })
    }

    pub fn step<S>(
        &self,
        state: &mut ModelState<Vec<Box<S>>>,
        thread_pool: Option<&ThreadPool>,
        timings: &mut RunTimings,
    ) -> Result<(), PywrError>
    where
        S: Solver,
    {
        let timestep = self
            .domain
            .time
            .timesteps()
            .get(state.current_time_step_idx)
            .ok_or(PywrError::EndOfTimesteps)?;

        let scenario_indices = self.domain.scenarios.indices();
        debug!("Starting timestep {:?}", timestep);

        let solvers = &mut state.solvers;
        let network_state = &mut state.state;

        match thread_pool {
            Some(pool) => {
                // State is mutated in-place
                pool.install(|| {
                    self.network
                        .step_par(timestep, scenario_indices, solvers, network_state, timings)
                })?;
            }
            None => {
                self.network
                    .step(timestep, scenario_indices, solvers, network_state, timings)?;
            }
        }

        let start_r_save = Instant::now();

        self.network
            .save_recorders(timestep, scenario_indices, &state.state, &mut state.recorder_state)?;
        timings.recorder_saving += start_r_save.elapsed();

        // Finally increment the time-step index
        state.current_time_step_idx += 1;

        Ok(())
    }

    pub fn step_multi_scenario<S>(
        &self,
        state: &mut ModelState<Box<S>>,
        thread_pool: &ThreadPool,
        timings: &mut RunTimings,
    ) -> Result<(), PywrError>
    where
        S: MultiStateSolver,
    {
        let timestep = self
            .domain
            .time
            .timesteps()
            .get(state.current_time_step_idx)
            .ok_or(PywrError::EndOfTimesteps)?;

        let scenario_indices = self.domain.scenarios.indices();
        debug!("Starting timestep {:?}", timestep);

        let solvers = &mut state.solvers;
        let network_state = &mut state.state;

        // State is mutated in-place
        thread_pool.install(|| {
            self.network
                .step_multi_scenario(timestep, scenario_indices, solvers, network_state, timings)
        })?;

        let start_r_save = Instant::now();

        self.network
            .save_recorders(timestep, scenario_indices, &state.state, &mut state.recorder_state)?;
        timings.recorder_saving += start_r_save.elapsed();

        // Finally increment the time-step index
        state.current_time_step_idx += 1;

        Ok(())
    }

    /// Run a model through the given time-steps.
    ///
    /// This method will setup state and solvers, and then run the model through the time-steps.
    pub fn run<S>(&self, settings: &S::Settings) -> Result<Vec<Option<Box<dyn Any>>>, PywrError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        let mut state = self.setup::<S>(settings)?;

        self.run_with_state::<S>(&mut state, settings)?;

        Ok(state.recorder_state)
    }

    /// Run the model with the provided states and solvers.
    pub fn run_with_state<S>(
        &self,
        state: &mut ModelState<Vec<Box<S>>>,
        settings: &S::Settings,
    ) -> Result<(), PywrError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        let mut timings = RunTimings::default();
        let mut count = 0;

        // Setup thread pool if running in parallel
        let pool = if settings.parallel() {
            Some(
                rayon::ThreadPoolBuilder::new()
                    .num_threads(settings.threads())
                    .build()
                    .unwrap(),
            )
        } else {
            None
        };

        loop {
            match self.step::<S>(state, pool.as_ref(), &mut timings) {
                Ok(_) => {}
                Err(PywrError::EndOfTimesteps) => break,
                Err(e) => return Err(e),
            }

            count += self.domain.scenarios.indices().len();
        }

        self.network.finalise(
            state.state.all_metric_set_internal_states_mut(),
            &mut state.recorder_state,
        )?;
        // End the global timer and print the run statistics
        timings.finish(count);
        timings.print_table();

        Ok(())
    }

    /// Run a network through the given time-steps with [`MultiStateSolver`].
    ///
    /// This method will setup state and the solver, and then run the network through the time-steps.
    pub fn run_multi_scenario<S>(&self, settings: &S::Settings) -> Result<(), PywrError>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings,
    {
        // Setup the network and create the initial state
        let mut state = self.setup_multi_scenario(settings)?;

        self.run_multi_scenario_with_state::<S>(&mut state, settings)
    }

    /// Run the network with the provided states and [`MultiStateSolver`] solver.
    pub fn run_multi_scenario_with_state<S>(
        &self,
        state: &mut ModelState<Box<S>>,
        settings: &S::Settings,
    ) -> Result<(), PywrError>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings,
    {
        let mut timings = RunTimings::default();
        let mut count = 0;

        let num_threads = if settings.parallel() { settings.threads() } else { 1 };

        // Setup thread pool
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .unwrap();

        loop {
            match self.step_multi_scenario::<S>(state, &pool, &mut timings) {
                Ok(_) => {}
                Err(PywrError::EndOfTimesteps) => break,
                Err(e) => return Err(e),
            }

            count += self.domain.scenarios.indices().len();
        }

        self.network.finalise(
            state.state.all_metric_set_internal_states_mut(),
            &mut state.recorder_state,
        )?;

        // End the global timer and print the run statistics
        timings.finish(count);
        timings.print_table();

        Ok(())
    }
}
