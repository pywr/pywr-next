use crate::aggregated_node::{AggregatedNode, AggregatedNodeIndex, AggregatedNodeVec, Factors};
use crate::aggregated_storage_node::{AggregatedStorageNode, AggregatedStorageNodeIndex, AggregatedStorageNodeVec};
use crate::edge::{EdgeIndex, EdgeVec};
use crate::metric::Metric;
use crate::node::{ConstraintValue, Node, NodeVec, StorageInitialVolume};
use crate::parameters::{MultiValueParameterIndex, ParameterType};
use crate::scenario::{ScenarioGroupCollection, ScenarioIndex};
use crate::solvers::{MultiStateSolver, Solver, SolverTimings};
use crate::state::{ParameterStates, State};
use crate::timestep::{Timestep, Timestepper};
use crate::virtual_storage::{VirtualStorage, VirtualStorageIndex, VirtualStorageReset, VirtualStorageVec};
use crate::{parameters, recorders, IndexParameterIndex, NodeIndex, ParameterIndex, PywrError, RecorderIndex};
use indicatif::ProgressIterator;
use log::{debug, info};
use rayon::prelude::*;
use std::any::Any;
use std::ops::Deref;
use std::time::Duration;
use std::time::Instant;

enum RunDuration {
    Running(Instant),
    Finished(Duration, usize),
}

pub struct RunTimings {
    global: RunDuration,
    parameter_calculation: Duration,
    recorder_saving: Duration,
    solve: SolverTimings,
}

impl Default for RunTimings {
    fn default() -> Self {
        Self {
            global: RunDuration::Running(Instant::now()),
            parameter_calculation: Duration::default(),
            recorder_saving: Duration::default(),
            solve: SolverTimings::default(),
        }
    }
}

impl RunTimings {
    /// End the global timer for this timing instance.
    ///
    /// If the timer has already finished this method has no effect.
    fn finish(&mut self, count: usize) {
        if let RunDuration::Running(i) = self.global {
            self.global = RunDuration::Finished(i.elapsed(), count);
        }
    }

    fn total_duration(&self) -> Duration {
        match self.global {
            RunDuration::Running(i) => i.elapsed(),
            RunDuration::Finished(d, _c) => d,
        }
    }

    fn speed(&self) -> Option<f64> {
        match self.global {
            RunDuration::Running(_) => None,
            RunDuration::Finished(d, c) => Some(c as f64 / d.as_secs_f64()),
        }
    }

    fn print_table(&self) {
        info!("Run timing statistics:");
        let total = self.total_duration().as_secs_f64();
        info!("{: <24} | {: <10}", "Metric", "Value");
        info!("{: <24} | {: <10.5}s", "Total", total);

        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Parameter calc",
            self.parameter_calculation.as_secs_f64(),
            100.0 * self.parameter_calculation.as_secs_f64() / total,
        );

        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Recorder save",
            self.recorder_saving.as_secs_f64(),
            100.0 * self.recorder_saving.as_secs_f64() / total,
        );

        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Solver::obj update",
            self.solve.update_objective.as_secs_f64(),
            100.0 * self.solve.update_objective.as_secs_f64() / total,
        );

        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Solver::const update",
            self.solve.update_constraints.as_secs_f64(),
            100.0 * self.solve.update_constraints.as_secs_f64() / total
        );

        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Solver::solve",
            self.solve.solve.as_secs_f64(),
            100.0 * self.solve.solve.as_secs_f64() / total,
        );

        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Solver::result update",
            self.solve.save_solution.as_secs_f64(),
            100.0 * self.solve.save_solution.as_secs_f64() / total,
        );

        // Difference between total and the parts counted in the timings
        let not_counted = total
            - self.parameter_calculation.as_secs_f64()
            - self.recorder_saving.as_secs_f64()
            - self.solve.total().as_secs_f64();

        info!(
            "{: <24} | {: <10.5}s ({:5.2}%)",
            "Residual",
            not_counted,
            100.0 * not_counted / total,
        );

        match self.speed() {
            None => info!("{: <24} | Unknown", "Speed"),
            Some(speed) => info!("{: <24} | {: <10.5} ts/s", "Speed", speed),
        };
    }
}

enum ComponentType {
    Node(NodeIndex),
    VirtualStorageNode(VirtualStorageIndex),
    Parameter(ParameterType),
}

#[derive(Default)]
pub struct RunOptions {
    parallel: bool,
    threads: usize,
}

impl RunOptions {
    pub fn parallel(mut self) -> Self {
        self.parallel = true;
        self
    }

    pub fn threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }
}

#[derive(Default)]
pub struct Model {
    scenarios: ScenarioGroupCollection,
    pub nodes: NodeVec,
    pub edges: EdgeVec,
    pub aggregated_nodes: AggregatedNodeVec,
    pub aggregated_storage_nodes: AggregatedStorageNodeVec,
    pub virtual_storage_nodes: VirtualStorageVec,
    parameters: Vec<Box<dyn parameters::Parameter>>,
    index_parameters: Vec<Box<dyn parameters::IndexParameter>>,
    multi_parameters: Vec<Box<dyn parameters::MultiValueParameter>>,
    resolve_order: Vec<ComponentType>,
    recorders: Vec<Box<dyn recorders::Recorder>>,
}

impl Model {
    /// Setup the model and create the initial state for each scenario.
    pub fn setup(
        &self,
        timesteps: &[Timestep],
    ) -> Result<
        (
            Vec<ScenarioIndex>,
            Vec<State>,
            Vec<ParameterStates>,
            Vec<Option<Box<dyn Any>>>,
        ),
        PywrError,
    > {
        let scenario_indices = self.scenarios.scenario_indices();
        let mut states: Vec<State> = Vec::with_capacity(scenario_indices.len());
        let mut parameter_internal_states: Vec<ParameterStates> = Vec::with_capacity(scenario_indices.len());

        for scenario_index in &scenario_indices {
            // Initialise node states. Note that storage nodes will have a zero volume at this point.
            let initial_node_states = self.nodes.iter().map(|n| n.default_state()).collect();

            let initial_virtual_storage_states = self.virtual_storage_nodes.iter().map(|n| n.default_state()).collect();

            // Get the initial internal state
            let initial_values_states = self
                .parameters
                .iter()
                .map(|p| p.setup(timesteps, scenario_index))
                .collect::<Result<Vec<_>, _>>()?;

            let initial_indices_states = self
                .index_parameters
                .iter()
                .map(|p| p.setup(timesteps, scenario_index))
                .collect::<Result<Vec<_>, _>>()?;

            let initial_multi_param_states = self
                .multi_parameters
                .iter()
                .map(|p| p.setup(timesteps, scenario_index))
                .collect::<Result<Vec<_>, _>>()?;

            let state = State::new(
                initial_node_states,
                self.edges.len(),
                initial_virtual_storage_states,
                initial_values_states.len(),
                initial_indices_states.len(),
                initial_multi_param_states.len(),
            );

            states.push(state);

            parameter_internal_states.push(ParameterStates::new(
                initial_values_states,
                initial_indices_states,
                initial_multi_param_states,
            ));
        }

        // Setup recorders
        let mut recorder_internal_states = Vec::new();
        for recorder in &self.recorders {
            let initial_state = recorder.setup(timesteps, &scenario_indices, self)?;
            recorder_internal_states.push(initial_state);
        }

        Ok((
            scenario_indices,
            states,
            parameter_internal_states,
            recorder_internal_states,
        ))
    }

    pub fn setup_solver<S>(&self) -> Result<Vec<Box<S>>, PywrError>
    where
        S: Solver,
    {
        let scenario_indices = self.scenarios.scenario_indices();
        let mut solvers = Vec::with_capacity(scenario_indices.len());

        for _scenario_index in scenario_indices {
            // Create a solver for each scenario
            let solver = S::setup(self)?;
            solvers.push(solver);
        }

        Ok(solvers)
    }

    fn setup_multi_scenario<S>(&self, scenario_indices: &[ScenarioIndex]) -> Result<Box<S>, PywrError>
    where
        S: MultiStateSolver,
    {
        S::setup(self, scenario_indices.len())
    }

    fn finalise(&self, recorder_internal_states: &mut Vec<Option<Box<dyn Any>>>) -> Result<(), PywrError> {
        // Setup recorders
        for (recorder, internal_state) in self.recorders.iter().zip(recorder_internal_states) {
            recorder.finalise(internal_state)?;
        }

        Ok(())
    }

    pub fn run<S>(&self, timestepper: &Timestepper, options: &RunOptions) -> Result<(), PywrError>
    where
        S: Solver,
    {
        let mut timings = RunTimings::default();
        let timesteps = timestepper.timesteps();

        // Setup the solver
        let mut count = 0;
        // Setup the model and create the initial state
        let (scenario_indices, mut states, mut parameter_internal_states, mut recorder_internal_states) =
            self.setup(&timesteps)?;

        let mut solvers = self.setup_solver::<S>()?;

        // Setup thread pool if running in parallel
        let pool = if options.parallel {
            Some(
                rayon::ThreadPoolBuilder::new()
                    .num_threads(options.threads)
                    .build()
                    .unwrap(),
            )
        } else {
            None
        };

        // Step a timestep
        for timestep in timesteps.iter() {
            debug!("Starting timestep {:?}", timestep);

            if let Some(pool) = &pool {
                // State is mutated in-place
                pool.install(|| {
                    self.step_par(
                        timestep,
                        &scenario_indices,
                        &mut solvers,
                        &mut states,
                        &mut parameter_internal_states,
                        &mut timings,
                    )
                })?;
            } else {
                // State is mutated in-place
                self.step(
                    timestep,
                    &scenario_indices,
                    &mut solvers,
                    &mut states,
                    &mut parameter_internal_states,
                    &mut timings,
                )?;
            }

            let start_r_save = Instant::now();
            self.save_recorders(timestep, &scenario_indices, &states, &mut recorder_internal_states)?;
            timings.recorder_saving += start_r_save.elapsed();

            count += scenario_indices.len();
        }

        self.finalise(&mut recorder_internal_states)?;
        // End the global timer and print the run statistics
        timings.finish(count);
        timings.print_table();

        Ok(())
    }

    pub fn run_multi_scenario<S>(&self, timestepper: &Timestepper) -> Result<(), PywrError>
    where
        S: MultiStateSolver,
    {
        let mut timings = RunTimings::default();
        let timesteps = timestepper.timesteps();

        // Setup the solver
        let mut count = 0;
        // Setup the model and create the initial state
        let (scenario_indices, mut states, mut parameter_internal_states, mut recorder_internal_states) =
            self.setup(&timesteps)?;

        let mut solver = self.setup_multi_scenario::<S>(&scenario_indices)?;

        // Step a timestep
        for timestep in timesteps.iter().progress() {
            debug!("Starting timestep {:?}", timestep);

            // State is mutated in-place
            self.step_multi_scenario(
                timestep,
                &scenario_indices,
                &mut solver,
                &mut states,
                &mut parameter_internal_states,
                &mut timings,
            )?;

            let start_r_save = Instant::now();
            self.save_recorders(timestep, &scenario_indices, &states, &mut recorder_internal_states)?;
            timings.recorder_saving += start_r_save.elapsed();

            count += scenario_indices.len();
        }

        self.finalise(&mut recorder_internal_states)?;

        // End the global timer and print the run statistics
        timings.finish(count);
        timings.print_table();

        Ok(())
    }

    /// Perform a single timestep mutating the current state.
    pub fn step<S>(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        solvers: &mut [Box<S>],
        states: &mut [State],
        parameter_internal_states: &mut [ParameterStates],
        timings: &mut RunTimings,
    ) -> Result<(), PywrError>
    where
        S: Solver,
    {
        scenario_indices
            .iter()
            .zip(states)
            .zip(parameter_internal_states)
            .zip(solvers)
            .for_each(|(((scenario_index, current_state), p_internal_state), solver)| {
                // TODO clear the current parameter values state (i.e. set them all to zero).

                let start_p_calc = Instant::now();
                self.compute_components(timestep, scenario_index, current_state, p_internal_state)
                    .unwrap();

                // State now contains updated parameter values BUT original network state
                timings.parameter_calculation += start_p_calc.elapsed();

                // Solve determines the new network state
                let solve_timings = solver.solve(self, timestep, current_state).unwrap();
                // State now contains updated parameter values AND updated network state
                timings.solve += solve_timings;

                // Now run the "after" method on all components
                let start_p_after = Instant::now();
                self.after(timestep, scenario_index, current_state, p_internal_state)
                    .unwrap();

                timings.parameter_calculation += start_p_after.elapsed();
            });

        Ok(())
    }

    /// Perform a single timestep in parallel using Rayon mutating the current state.
    ///
    /// Note that the `timings` struct will be incremented with the timing information from
    /// each scenario and therefore contain the total time across all parallel threads (i.e.
    /// not overall wall-time).
    pub(crate) fn step_par<S>(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        solvers: &mut [Box<S>],
        states: &mut [State],
        parameter_internal_states: &mut [ParameterStates],
        timings: &mut RunTimings,
    ) -> Result<(), PywrError>
    where
        S: Solver,
    {
        // Collect all the timings from each parallel solve
        let step_times: Vec<_> = scenario_indices
            .par_iter()
            .zip(states)
            .zip(parameter_internal_states)
            .zip(solvers)
            .map(|(((scenario_index, current_state), p_internal_state), solver)| {
                // TODO clear the current parameter values state (i.e. set them all to zero).

                let start_p_calc = Instant::now();
                self.compute_components(timestep, scenario_index, current_state, p_internal_state)
                    .unwrap();

                // State now contains updated parameter values BUT original network state
                let mut parameter_calculation = start_p_calc.elapsed();

                // Solve determines the new network state
                let solve_timings = solver.solve(self, timestep, current_state).unwrap();
                // State now contains updated parameter values AND updated network state

                // Now run the "after" method on all components
                let start_p_after = Instant::now();
                self.after(timestep, scenario_index, current_state, p_internal_state)
                    .unwrap();

                parameter_calculation += start_p_after.elapsed();

                (parameter_calculation, solve_timings)
            })
            .collect();

        // Add them all together
        for (parameter_calculation, solve_timings) in step_times.into_iter() {
            timings.parameter_calculation += parameter_calculation;
            timings.solve += solve_timings;
        }

        Ok(())
    }

    /// Perform a single timestep with a multi-state solver mutating the current state.
    pub(crate) fn step_multi_scenario<S>(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        solver: &mut Box<S>,
        states: &mut [State],
        parameter_internal_states: &mut [ParameterStates],
        timings: &mut RunTimings,
    ) -> Result<(), PywrError>
    where
        S: MultiStateSolver,
    {
        // First compute all the updated state

        let p_calc_timings: Vec<_> = scenario_indices
            .par_iter()
            .zip(&mut *states)
            .zip(&mut *parameter_internal_states)
            .map(|((scenario_index, current_state), p_internal_state)| {
                // TODO clear the current parameter values state (i.e. set them all to zero).

                let start_p_calc = Instant::now();
                self.compute_components(timestep, scenario_index, current_state, p_internal_state)
                    .unwrap();

                // State now contains updated parameter values BUT original network state
                start_p_calc.elapsed()
            })
            .collect();

        for t in p_calc_timings.into_iter() {
            timings.parameter_calculation += t;
        }

        // Now solve all the LPs simultaneously
        let solve_timings = solver.solve(self, timestep, states).unwrap();
        // State now contains updated parameter values AND updated network state
        timings.solve += solve_timings;

        // Now run the "after" method on all components
        let p_after_timings: Vec<_> = scenario_indices
            .par_iter()
            .zip(&mut *states)
            .zip(parameter_internal_states)
            .map(|((scenario_index, current_state), p_internal_state)| {
                let start_p_after = Instant::now();
                self.after(timestep, scenario_index, current_state, p_internal_state)
                    .unwrap();
                start_p_after.elapsed()
            })
            .collect();

        for t in p_after_timings.into_iter() {
            timings.parameter_calculation += t;
        }

        Ok(())
    }

    /// Undertake calculations for model components before solve.
    ///
    /// This method iterates through the model components (nodes, parameters, etc) to perform
    /// pre-solve calculations. For nodes this can be adjustments to storage volume (e.g. to
    /// set initial volume). For parameters this involves computing the current value for the
    /// the timestep. The `state` object is progressively updated with these values during this
    /// method.
    fn compute_components(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        state: &mut State,
        internal_states: &mut ParameterStates,
    ) -> Result<(), PywrError> {
        // TODO reset parameter state to zero

        for c_type in &self.resolve_order {
            match c_type {
                ComponentType::Node(idx) => {
                    let n = self.nodes.get(idx)?;
                    n.before(timestep, self, state)?;
                }
                ComponentType::VirtualStorageNode(idx) => {
                    let n = self.virtual_storage_nodes.get(idx)?;
                    n.before(timestep, self, state)?;
                }
                ComponentType::Parameter(p_type) => {
                    match p_type {
                        ParameterType::Parameter(idx) => {
                            // Find the parameter itself
                            let p = self
                                .parameters
                                .get(*idx.deref())
                                .ok_or(PywrError::ParameterIndexNotFound(*idx))?;
                            // .. and its internal state
                            let internal_state = internal_states
                                .get_mut_value_state(*idx)
                                .ok_or(PywrError::ParameterIndexNotFound(*idx))?;

                            let value = p.compute(timestep, scenario_index, self, state, internal_state)?;

                            // TODO move this check into the method below
                            if value.is_nan() {
                                panic!("NaN value computed in parameter: {}", p.name());
                            }
                            state.set_parameter_value(*idx, value)?;
                        }
                        ParameterType::Index(idx) => {
                            let p = self
                                .index_parameters
                                .get(*idx.deref())
                                .ok_or(PywrError::IndexParameterIndexNotFound(*idx))?;

                            // .. and its internal state
                            let internal_state = internal_states
                                .get_mut_index_state(*idx)
                                .ok_or(PywrError::IndexParameterIndexNotFound(*idx))?;

                            let value = p.compute(timestep, scenario_index, self, state, internal_state)?;
                            // debug!("Current value of index parameter {}: {}", p.name(), value);
                            state.set_parameter_index(*idx, value)?;
                        }
                        ParameterType::Multi(idx) => {
                            let p = self
                                .multi_parameters
                                .get(*idx.deref())
                                .ok_or(PywrError::MultiValueParameterIndexNotFound(*idx))?;

                            // .. and its internal state
                            let internal_state = internal_states
                                .get_mut_multi_state(*idx)
                                .ok_or(PywrError::MultiValueParameterIndexNotFound(*idx))?;

                            let value = p.compute(timestep, scenario_index, self, state, internal_state)?;
                            // debug!("Current value of index parameter {}: {}", p.name(), value);
                            state.set_multi_parameter_value(*idx, value)?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Undertake "after" for model components after solve.
    ///
    /// This method iterates through the model components (nodes, parameters, etc) to perform
    /// pre-solve calculations. For nodes this can be adjustments to storage volume (e.g. to
    /// set initial volume). For parameters this involves computing the current value for the
    /// the timestep. The `state` object is progressively updated with these values during this
    /// method.
    fn after(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        state: &mut State,
        internal_states: &mut ParameterStates,
    ) -> Result<(), PywrError> {
        // TODO reset parameter state to zero

        for c_type in &self.resolve_order {
            match c_type {
                ComponentType::Node(_) => {
                    // Nodes do not have an "after" method.
                }
                ComponentType::VirtualStorageNode(_) => {
                    // Nodes do not have an "after" method.;
                }
                ComponentType::Parameter(p_type) => {
                    match p_type {
                        ParameterType::Parameter(idx) => {
                            // Find the parameter itself
                            let p = self
                                .parameters
                                .get(*idx.deref())
                                .ok_or(PywrError::ParameterIndexNotFound(*idx))?;
                            // .. and its internal state
                            let internal_state = internal_states
                                .get_mut_value_state(*idx)
                                .ok_or(PywrError::ParameterIndexNotFound(*idx))?;

                            p.after(timestep, scenario_index, self, state, internal_state)?;
                        }
                        ParameterType::Index(idx) => {
                            let p = self
                                .index_parameters
                                .get(*idx.deref())
                                .ok_or(PywrError::IndexParameterIndexNotFound(*idx))?;

                            // .. and its internal state
                            let internal_state = internal_states
                                .get_mut_index_state(*idx)
                                .ok_or(PywrError::IndexParameterIndexNotFound(*idx))?;

                            p.after(timestep, scenario_index, self, state, internal_state)?;
                        }
                        ParameterType::Multi(idx) => {
                            let p = self
                                .multi_parameters
                                .get(*idx.deref())
                                .ok_or(PywrError::MultiValueParameterIndexNotFound(*idx))?;

                            // .. and its internal state
                            let internal_state = internal_states
                                .get_mut_multi_state(*idx)
                                .ok_or(PywrError::MultiValueParameterIndexNotFound(*idx))?;

                            p.after(timestep, scenario_index, self, state, internal_state)?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn save_recorders(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        states: &[State],
        recorder_internal_states: &mut [Option<Box<dyn Any>>],
    ) -> Result<(), PywrError> {
        for (recorder, internal_state) in self.recorders.iter().zip(recorder_internal_states) {
            recorder.save(timestep, scenario_indices, self, states, internal_state)?;
        }
        Ok(())
    }

    /// Add a `ScenarioGroup` to the model
    pub fn add_scenario_group(&mut self, name: &str, size: usize) -> Result<(), PywrError> {
        self.scenarios.add_group(name, size);
        Ok(())
    }

    /// Get a `ScenarioGroup`'s index by name
    pub fn get_scenario_group_index_by_name(&self, name: &str) -> Result<usize, PywrError> {
        self.scenarios.get_group_index_by_name(name)
    }

    /// Get a `ScenarioGroup`'s size by name
    pub fn get_scenario_group_size_by_name(&self, name: &str) -> Result<usize, PywrError> {
        self.scenarios.get_group_by_name(name).map(|g| g.size())
    }

    pub fn get_scenario_indices(&self) -> Vec<ScenarioIndex> {
        self.scenarios.scenario_indices()
    }

    /// Get a Node from a node's name
    pub fn get_node_index_by_name(&self, name: &str, sub_name: Option<&str>) -> Result<NodeIndex, PywrError> {
        Ok(self.get_node_by_name(name, sub_name)?.index())
    }

    /// Get a Node from a node's index
    pub fn get_node(&self, index: &NodeIndex) -> Result<&Node, PywrError> {
        self.nodes.get(index)
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
    pub fn get_aggregated_node(&self, index: &AggregatedNodeIndex) -> Result<&AggregatedNode, PywrError> {
        self.aggregated_nodes.get(index)
    }

    /// Get a `AggregatedNode` from a node's name
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

    pub fn set_aggregated_node_factors(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        factors: Option<Factors>,
    ) -> Result<(), PywrError> {
        let node = self.get_mut_aggregated_node_by_name(name, sub_name)?;
        node.set_factors(factors);
        Ok(())
    }

    /// Get a `&AggregatedStorageNode` from a node's name
    pub fn get_aggregated_storage_node(
        &self,
        index: &AggregatedStorageNodeIndex,
    ) -> Result<&AggregatedStorageNode, PywrError> {
        self.aggregated_storage_nodes.get(index)
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

    /// Get a `VirtualStorageNode` from a node's name
    pub fn get_virtual_storage_node(&self, index: &VirtualStorageIndex) -> Result<&VirtualStorage, PywrError> {
        self.virtual_storage_nodes.get(index)
    }

    /// Get a `VirtualStorageNode` from a node's name
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
        } else if let Ok(idx) = self.get_aggregated_storage_node_index_by_name(name, sub_name) {
            if proportional {
                Ok(Metric::AggregatedNodeProportionalVolume(idx))
            } else {
                Ok(Metric::AggregatedNodeVolume(idx))
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
            .chain(self.aggregated_nodes.iter().map(|n| {
                let metric = n.default_metric();
                let (name, sub_name) = n.full_name();
                (metric, (name.to_string(), sub_name.map(|s| s.to_string())))
            }))
            .collect()
    }

    pub fn get_parameter_metrics(&self) -> Vec<(Metric, (String, Option<String>))> {
        self.parameters
            .iter()
            .enumerate()
            .map(|(idx, p)| {
                let metric = Metric::ParameterValue(ParameterIndex::new(idx));

                (metric, (format!("param-{}", p.name()), None))
            })
            .collect()
    }

    /// Get a `Parameter` from a parameter's name
    pub fn get_parameter(&self, index: &ParameterIndex) -> Result<&dyn parameters::Parameter, PywrError> {
        match self.parameters.get(*index.deref()) {
            Some(p) => Ok(p.as_ref()),
            None => Err(PywrError::ParameterIndexNotFound(*index)),
        }
    }

    /// Get a `Parameter` from a parameter's name
    pub fn get_mut_parameter(&mut self, index: &ParameterIndex) -> Result<&mut dyn parameters::Parameter, PywrError> {
        match self.parameters.get_mut(*index.deref()) {
            Some(p) => Ok(p.as_mut()),
            None => Err(PywrError::ParameterIndexNotFound(*index)),
        }
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

    /// Get a `MultiValueParameterIndex` from a parameter's name
    pub fn get_multi_valued_parameter_index_by_name(&self, name: &str) -> Result<MultiValueParameterIndex, PywrError> {
        match self.multi_parameters.iter().position(|p| p.name() == name) {
            Some(idx) => Ok(MultiValueParameterIndex::new(idx)),
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
        // ... and add it to the resolve order.
        self.resolve_order.push(ComponentType::Node(node_index));
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
        // ... and add it to the resolve order.
        self.resolve_order.push(ComponentType::Node(node_index));
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
        // ... and add it to the resolve order.
        self.resolve_order.push(ComponentType::Node(node_index));
        Ok(node_index)
    }

    /// Add a new Node::Link to the model.
    pub fn add_storage_node(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        initial_volume: StorageInitialVolume,
        min_volume: ConstraintValue,
        max_volume: ConstraintValue,
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
        // ... and add it to the resolve order.
        self.resolve_order.push(ComponentType::Node(node_index));
        Ok(node_index)
    }

    /// Add a new `aggregated_node::AggregatedNode` to the model.
    pub fn add_aggregated_node(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        nodes: &[NodeIndex],
        factors: Option<Factors>,
    ) -> Result<AggregatedNodeIndex, PywrError> {
        if let Ok(_agg_node) = self.get_aggregated_node_by_name(name, sub_name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        let node_index = self.aggregated_nodes.push_new(name, sub_name, nodes, factors);
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
        nodes: &[NodeIndex],
        factors: Option<&[f64]>,
        initial_volume: StorageInitialVolume,
        min_volume: ConstraintValue,
        max_volume: ConstraintValue,
        reset: VirtualStorageReset,
    ) -> Result<VirtualStorageIndex, PywrError> {
        if let Ok(_agg_node) = self.get_virtual_storage_node_by_name(name, sub_name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        let node_index = self.virtual_storage_nodes.push_new(
            name,
            sub_name,
            nodes,
            factors,
            initial_volume,
            min_volume,
            max_volume,
            reset,
        );

        // Add to the resolve order.
        self.resolve_order.push(ComponentType::VirtualStorageNode(node_index));

        Ok(node_index)
    }

    /// Add a `parameters::Parameter` to the model
    pub fn add_parameter(&mut self, parameter: Box<dyn parameters::Parameter>) -> Result<ParameterIndex, PywrError> {
        if let Ok(idx) = self.get_parameter_index_by_name(&parameter.meta().name) {
            return Err(PywrError::ParameterNameAlreadyExists(
                parameter.meta().name.to_string(),
                idx,
            ));
        }

        let parameter_index = ParameterIndex::new(self.parameters.len());

        // Add the parameter ...
        self.parameters.push(parameter);
        // .. and add it to the resolve order
        self.resolve_order
            .push(ComponentType::Parameter(ParameterType::Parameter(parameter_index)));
        Ok(parameter_index)
    }

    /// Add a `parameters::IndexParameter` to the model
    pub fn add_index_parameter(
        &mut self,
        index_parameter: Box<dyn parameters::IndexParameter>,
    ) -> Result<IndexParameterIndex, PywrError> {
        if let Ok(idx) = self.get_index_parameter_index_by_name(&index_parameter.meta().name) {
            return Err(PywrError::IndexParameterNameAlreadyExists(
                index_parameter.meta().name.to_string(),
                idx,
            ));
        }

        let parameter_index = IndexParameterIndex::new(self.index_parameters.len());

        self.index_parameters.push(index_parameter);
        // .. and add it to the resolve order
        self.resolve_order
            .push(ComponentType::Parameter(ParameterType::Index(parameter_index)));
        Ok(parameter_index)
    }

    /// Add a `parameters::MultiValueParameter` to the model
    pub fn add_multi_value_parameter(
        &mut self,
        parameter: Box<dyn parameters::MultiValueParameter>,
    ) -> Result<MultiValueParameterIndex, PywrError> {
        if let Ok(idx) = self.get_parameter_index_by_name(&parameter.meta().name) {
            return Err(PywrError::ParameterNameAlreadyExists(
                parameter.meta().name.to_string(),
                idx,
            ));
        }

        let parameter_index = MultiValueParameterIndex::new(self.multi_parameters.len());

        // Add the parameter ...
        self.multi_parameters.push(parameter);
        // .. and add it to the resolve order
        self.resolve_order
            .push(ComponentType::Parameter(ParameterType::Multi(parameter_index)));
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

    #[cfg(feature = "clipm")]
    use crate::solvers::ClIpmF64Solver;
    use crate::solvers::ClpSolver;
    #[cfg(feature = "highs")]
    use crate::solvers::HighsSolver;
    use crate::test_utils::{default_timestepper, simple_model, simple_storage_model};
    use float_cmp::approx_eq;
    use ndarray::Array2;
    use std::ops::Deref;

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
            model.add_storage_node(
                "my-node",
                None,
                StorageInitialVolume::Absolute(10.0),
                ConstraintValue::Scalar(0.0),
                ConstraintValue::Scalar(10.0)
            ),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );
    }

    #[test]
    /// Test adding a constant parameter to a model.
    fn test_constant_parameter() {
        let mut model = Model::default();
        let _node_index = model.add_input_node("input", None).unwrap();

        let input_max_flow = parameters::ConstantParameter::new("my-constant", 10.0);
        let parameter = model.add_parameter(Box::new(input_max_flow)).unwrap();

        // assign the new parameter to one of the nodes.
        let node = model.get_mut_node_by_name("input", None).unwrap();
        node.set_constraint(
            ConstraintValue::Metric(Metric::ParameterValue(parameter)),
            Constraint::MaxFlow,
        )
        .unwrap();

        // Try to assign a constraint not defined for particular node type
        assert_eq!(
            node.set_constraint(ConstraintValue::Scalar(10.0), Constraint::MaxVolume),
            Err(PywrError::StorageConstraintsUndefined)
        );
    }

    #[test]
    fn test_step() {
        let model = simple_model(2);

        let timestepper = default_timestepper();

        let mut timings = RunTimings::default();
        let timesteps = timestepper.timesteps();
        let mut ts_iter = timesteps.iter();

        let ts = ts_iter.next().unwrap();
        let (scenario_indices, mut current_state, mut p_internal, _r_internal) = model.setup(&timesteps).unwrap();

        let mut solvers = model.setup_solver::<ClpSolver>().unwrap();
        assert_eq!(current_state.len(), scenario_indices.len());

        model
            .step(
                ts,
                &scenario_indices,
                &mut solvers,
                &mut current_state,
                &mut p_internal,
                &mut timings,
            )
            .unwrap();

        let output_node = model.get_node_by_name("output", None).unwrap();

        let state0 = current_state.get(0).unwrap();
        let output_inflow = state0
            .get_network_state()
            .get_node_in_flow(&output_node.index())
            .unwrap();
        assert!(approx_eq!(f64, output_inflow, 10.0));
    }

    #[test]
    /// Test running a simple model
    fn test_run() {
        let mut model = simple_model(10);
        let timestepper = default_timestepper();

        // Set-up assertion for "input" node
        let idx = model.get_node_by_name("input", None).unwrap().index();
        let expected = Array2::from_elem((366, 10), 10.0);
        let recorder = AssertionRecorder::new("input-flow", Metric::NodeOutFlow(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("link", None).unwrap().index();
        let expected = Array2::from_elem((366, 10), 10.0);
        let recorder = AssertionRecorder::new("link-flow", Metric::NodeOutFlow(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("output", None).unwrap().index();
        let expected = Array2::from_elem((366, 10), 10.0);
        let recorder = AssertionRecorder::new("output-flow", Metric::NodeInFlow(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_parameter_index_by_name("total-demand").unwrap();
        let expected = Array2::from_elem((366, 10), 12.0);
        let recorder = AssertionRecorder::new("total-demand", Metric::ParameterValue(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        model.run::<ClpSolver>(&timestepper, &RunOptions::default()).unwrap();
        #[cfg(feature = "highs")]
        model.run::<HighsSolver>(&timestepper, &RunOptions::default()).unwrap();
    }

    #[test]
    #[ignore]
    #[cfg(feature = "clipm")]
    /// Test running a simple model with the OpenCL IPM solver
    fn test_run_cl_ipm() {
        let mut model = simple_model(10);
        let timestepper = default_timestepper();

        // Set-up assertion for "input" node
        let idx = model.get_node_by_name("input", None).unwrap().index();
        let expected = Array2::from_elem((366, 10), 10.0);
        let recorder = AssertionRecorder::new("input-flow", Metric::NodeOutFlow(idx), expected, None, Some(1.0e-6));
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("link", None).unwrap().index();
        let expected = Array2::from_elem((366, 10), 10.0);
        let recorder = AssertionRecorder::new("link-flow", Metric::NodeOutFlow(idx), expected, None, Some(1.0e-6));
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("output", None).unwrap().index();
        let expected = Array2::from_elem((366, 10), 10.0);
        let recorder = AssertionRecorder::new("output-flow", Metric::NodeInFlow(idx), expected, None, Some(1.0e-6));
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_parameter_index_by_name("total-demand").unwrap();
        let expected = Array2::from_elem((366, 10), 12.0);
        let recorder = AssertionRecorder::new(
            "total-demand",
            Metric::ParameterValue(idx),
            expected,
            Some(5),
            Some(1.0e-6),
        );
        model.add_recorder(Box::new(recorder)).unwrap();

        model.run_multi_scenario::<ClIpmF64Solver>(&timestepper).unwrap();
    }

    #[test]
    fn test_run_storage() {
        let mut model = simple_storage_model();
        let timestepper = default_timestepper();

        let idx = model.get_node_by_name("output", None).unwrap().index();

        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| if i < 10 { 10.0 } else { 0.0 });

        let recorder = AssertionRecorder::new("output-flow", Metric::NodeInFlow(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("reservoir", None).unwrap().index();

        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (90.0 - 10.0 * i as f64).max(0.0));

        let recorder = AssertionRecorder::new("reservoir-volume", Metric::NodeVolume(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        model.run::<ClpSolver>(&timestepper, &RunOptions::default()).unwrap();
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
