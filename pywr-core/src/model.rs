use crate::aggregated_node::{AggregatedNode, AggregatedNodeIndex, AggregatedNodeVec, Factors};
use crate::aggregated_storage_node::{AggregatedStorageNode, AggregatedStorageNodeIndex, AggregatedStorageNodeVec};
use crate::derived_metric::{DerivedMetric, DerivedMetricIndex};
use crate::edge::{EdgeIndex, EdgeVec};
use crate::metric::Metric;
use crate::node::{ConstraintValue, Node, NodeVec, StorageInitialVolume};
use crate::parameters::{MultiValueParameterIndex, ParameterType};
use crate::recorders::{MetricSet, MetricSetIndex};
use crate::scenario::{ScenarioGroupCollection, ScenarioIndex};
use crate::solvers::{MultiStateSolver, Solver, SolverFeatures, SolverSettings, SolverTimings};
use crate::state::{ParameterStates, State};
use crate::timestep::{Timestep, Timestepper};
use crate::virtual_storage::{VirtualStorage, VirtualStorageIndex, VirtualStorageReset, VirtualStorageVec};
use crate::{parameters, recorders, IndexParameterIndex, NodeIndex, ParameterIndex, PywrError, RecorderIndex};
use rayon::prelude::*;
use std::any::Any;
use std::collections::HashSet;
use std::ops::Deref;
use std::time::Duration;
use std::time::Instant;
use tracing::{debug, info};

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
    DerivedMetric(DerivedMetricIndex),
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
    derived_metrics: Vec<DerivedMetric>,
    metric_sets: Vec<MetricSet>,
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
                self.derived_metrics.len(),
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

    /// Check whether a solver [`S`] has the required features to run this model.
    pub fn check_solver_features<S>(&self) -> bool
    where
        S: Solver,
    {
        let required_features = self.required_features();

        required_features.iter().all(|f| S::features().contains(f))
    }

    /// Check whether a solver [`S`] has the required features to run this model.
    pub fn check_multi_scenario_solver_features<S>(&self) -> bool
    where
        S: MultiStateSolver,
    {
        let required_features = self.required_features();

        required_features.iter().all(|f| S::features().contains(f))
    }

    pub fn setup_solver<S>(&self, settings: &S::Settings) -> Result<Vec<Box<S>>, PywrError>
    where
        S: Solver,
    {
        if !self.check_solver_features::<S>() {
            return Err(PywrError::MissingSolverFeatures);
        }

        let scenario_indices = self.scenarios.scenario_indices();

        let mut solvers = Vec::with_capacity(scenario_indices.len());

        for _scenario_index in scenario_indices {
            // Create a solver for each scenario
            let solver = S::setup(self, settings)?;
            solvers.push(solver);
        }

        Ok(solvers)
    }

    pub fn setup_multi_scenario<S>(
        &self,
        scenario_indices: &[ScenarioIndex],
        settings: &S::Settings,
    ) -> Result<Box<S>, PywrError>
    where
        S: MultiStateSolver,
    {
        if !self.check_multi_scenario_solver_features::<S>() {
            return Err(PywrError::MissingSolverFeatures);
        }
        S::setup(self, scenario_indices.len(), settings)
    }

    fn finalise(&self, recorder_internal_states: &mut [Option<Box<dyn Any>>]) -> Result<(), PywrError> {
        // Setup recorders
        for (recorder, internal_state) in self.recorders.iter().zip(recorder_internal_states) {
            recorder.finalise(internal_state)?;
        }

        Ok(())
    }

    /// Run a model through the given time-steps.
    ///
    /// This method will setup state and solvers, and then run the model through the time-steps.
    pub fn run<S>(&self, timestepper: &Timestepper, settings: &S::Settings) -> Result<(), PywrError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        let timesteps = timestepper.timesteps();

        // Setup the model and create the initial state
        let (scenario_indices, mut states, mut parameter_internal_states, mut recorder_internal_states) =
            self.setup(&timesteps)?;

        // Setup the solver
        let mut solvers = self.setup_solver::<S>(settings)?;

        self.run_with_state(
            timestepper,
            settings,
            &scenario_indices,
            &mut states,
            &mut parameter_internal_states,
            &mut recorder_internal_states,
            &mut solvers,
        )
    }

    /// Run the model with the provided states and solvers.
    pub fn run_with_state<S>(
        &self,
        timestepper: &Timestepper,
        settings: &S::Settings,
        scenario_indices: &[ScenarioIndex],
        states: &mut [State],
        parameter_internal_states: &mut [ParameterStates],
        recorder_internal_states: &mut [Option<Box<dyn Any>>],
        solvers: &mut [Box<S>],
    ) -> Result<(), PywrError>
    where
        S: Solver,
        <S as Solver>::Settings: SolverSettings,
    {
        let mut timings = RunTimings::default();
        let mut count = 0;

        let timesteps = timestepper.timesteps();

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

        // Step a timestep
        for timestep in timesteps.iter() {
            debug!("Starting timestep {:?}", timestep);

            if let Some(pool) = &pool {
                // State is mutated in-place
                pool.install(|| {
                    self.step_par(
                        timestep,
                        &scenario_indices,
                        solvers,
                        states,
                        parameter_internal_states,
                        &mut timings,
                    )
                })?;
            } else {
                // State is mutated in-place
                self.step(
                    timestep,
                    &scenario_indices,
                    solvers,
                    states,
                    parameter_internal_states,
                    &mut timings,
                )?;
            }

            let start_r_save = Instant::now();
            self.save_recorders(timestep, &scenario_indices, &states, recorder_internal_states)?;
            timings.recorder_saving += start_r_save.elapsed();

            count += scenario_indices.len();
        }

        self.finalise(recorder_internal_states)?;
        // End the global timer and print the run statistics
        timings.finish(count);
        timings.print_table();

        Ok(())
    }

    /// Run a model through the given time-steps with [`MultiStateSolver`].
    ///
    /// This method will setup state and the solver, and then run the model through the time-steps.
    pub fn run_multi_scenario<S>(&self, timestepper: &Timestepper, settings: &S::Settings) -> Result<(), PywrError>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings,
    {
        let timesteps = timestepper.timesteps();

        // Setup the model and create the initial state
        let (scenario_indices, mut states, mut parameter_internal_states, mut recorder_internal_states) =
            self.setup(&timesteps)?;

        // Setup the solver
        let mut solver = self.setup_multi_scenario::<S>(&scenario_indices, settings)?;

        self.run_multi_scenario_with_state(
            &timestepper,
            settings,
            &scenario_indices,
            &mut states,
            &mut parameter_internal_states,
            &mut recorder_internal_states,
            &mut solver,
        )
    }

    /// Run the model with the provided states and [`MultiStateSolver`] solver.
    pub fn run_multi_scenario_with_state<S>(
        &self,
        timestepper: &Timestepper,
        settings: &S::Settings,
        scenario_indices: &[ScenarioIndex],
        states: &mut [State],
        parameter_internal_states: &mut [ParameterStates],
        recorder_internal_states: &mut [Option<Box<dyn Any>>],
        solver: &mut Box<S>,
    ) -> Result<(), PywrError>
    where
        S: MultiStateSolver,
        <S as MultiStateSolver>::Settings: SolverSettings,
    {
        let mut timings = RunTimings::default();
        let mut count = 0;

        let timesteps = timestepper.timesteps();
        let num_threads = if settings.parallel() { settings.threads() } else { 1 };

        // Setup thread pool
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .unwrap();

        // Step a timestep
        for timestep in timesteps.iter() {
            debug!("Starting timestep {:?}", timestep);

            pool.install(|| {
                // State is mutated in-place
                self.step_multi_scenario(
                    timestep,
                    &scenario_indices,
                    solver,
                    states,
                    parameter_internal_states,
                    &mut timings,
                )
            })?;

            let start_r_save = Instant::now();
            self.save_recorders(timestep, &scenario_indices, &states, recorder_internal_states)?;
            timings.recorder_saving += start_r_save.elapsed();

            count += scenario_indices.len();
        }

        self.finalise(recorder_internal_states)?;

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

    /// Calculate the set of [`SolverFeatures`] required to correctly run this model.
    fn required_features(&self) -> HashSet<SolverFeatures> {
        let mut features = HashSet::new();

        // Aggregated node feature required if there are any aggregated nodes
        if self.aggregated_nodes.len() > 0 {
            features.insert(SolverFeatures::AggregatedNode);
        }

        // Aggregated node factors required if any aggregated node has factors defined.
        if self.aggregated_nodes.iter().any(|n| n.get_factors().is_some()) {
            features.insert(SolverFeatures::AggregatedNodeFactors);
        }

        // The presence of any virtual storage node requires the VirtualStorage feature.
        if self.virtual_storage_nodes.len() > 0 {
            features.insert(SolverFeatures::VirtualStorage);
        }

        features
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
                ComponentType::DerivedMetric(idx) => {
                    // Compute derived metrics in before
                    let m = self
                        .derived_metrics
                        .get(*idx.deref())
                        .ok_or(PywrError::DerivedMetricIndexNotFound(*idx))?;
                    if let Some(value) = m.before(timestep, self, state)? {
                        state.set_derived_metric_value(*idx, value)?;
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
                ComponentType::DerivedMetric(idx) => {
                    // Compute derived metrics in "after"
                    let m = self
                        .derived_metrics
                        .get(*idx.deref())
                        .ok_or(PywrError::DerivedMetricIndexNotFound(*idx))?;
                    let value = m.compute(self, state)?;
                    state.set_derived_metric_value(*idx, value)?;
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

    /// Get a `AggregatedNodeIndex` from a node's name
    pub fn get_aggregated_node_index_by_name(
        &self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Result<AggregatedNodeIndex, PywrError> {
        match self
            .aggregated_nodes
            .iter()
            .find(|&n| n.full_name() == (name, sub_name))
        {
            Some(node) => Ok(node.index()),
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

    /// Get a `VirtualStorageNode` from a node's name
    pub fn get_virtual_storage_node_index_by_name(
        &self,
        name: &str,
        sub_name: Option<&str>,
    ) -> Result<VirtualStorageIndex, PywrError> {
        let node = self.get_virtual_storage_node_by_name(name, sub_name)?;
        Ok(node.index())
    }

    pub fn get_storage_node_metric(
        &mut self,
        name: &str,
        sub_name: Option<&str>,
        proportional: bool,
    ) -> Result<Metric, PywrError> {
        if let Ok(idx) = self.get_node_index_by_name(name, sub_name) {
            // A regular node
            if proportional {
                // Proportional is a derived metric
                let dm_idx = self.add_derived_metric(DerivedMetric::NodeProportionalVolume(idx));
                Ok(Metric::DerivedMetric(dm_idx))
            } else {
                Ok(Metric::NodeVolume(idx))
            }
        } else if let Ok(idx) = self.get_aggregated_storage_node_index_by_name(name, sub_name) {
            if proportional {
                // Proportional is a derived metric
                let dm_idx = self.add_derived_metric(DerivedMetric::AggregatedNodeProportionalVolume(idx));
                Ok(Metric::DerivedMetric(dm_idx))
            } else {
                Ok(Metric::AggregatedNodeVolume(idx))
            }
        } else if let Ok(node) = self.get_virtual_storage_node_by_name(name, sub_name) {
            if proportional {
                // Proportional is a derived metric
                let dm_idx = self.add_derived_metric(DerivedMetric::VirtualStorageProportionalVolume(node.index()));
                Ok(Metric::DerivedMetric(dm_idx))
            } else {
                Ok(Metric::VirtualStorageVolume(node.index()))
            }
        } else {
            Err(PywrError::NodeNotFound(name.to_string()))
        }
    }

    /// Get a [`DerivedMetricIndex`] for the given derived metric
    pub fn get_derived_metric_index(&self, derived_metric: &DerivedMetric) -> Result<DerivedMetricIndex, PywrError> {
        let idx = self
            .derived_metrics
            .iter()
            .position(|dm| dm == derived_metric)
            .ok_or(PywrError::DerivedMetricNotFound)?;

        Ok(DerivedMetricIndex::new(idx))
    }

    /// Get a [`DerivedMetricIndex`] for the given derived metric
    pub fn get_derived_metric(&self, index: &DerivedMetricIndex) -> Result<&DerivedMetric, PywrError> {
        self.derived_metrics
            .get(*index.deref())
            .ok_or(PywrError::DerivedMetricNotFound)
    }

    pub fn add_derived_metric(&mut self, derived_metric: DerivedMetric) -> DerivedMetricIndex {
        match self.get_derived_metric_index(&derived_metric) {
            Ok(idx) => idx,
            Err(_) => {
                self.derived_metrics.push(derived_metric);
                let idx = DerivedMetricIndex::new(self.derived_metrics.len() - 1);
                self.resolve_order.push(ComponentType::DerivedMetric(idx));
                idx
            }
        }
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
        cost: ConstraintValue,
    ) -> Result<VirtualStorageIndex, PywrError> {
        if let Ok(_agg_node) = self.get_virtual_storage_node_by_name(name, sub_name) {
            return Err(PywrError::NodeNameAlreadyExists(name.to_string()));
        }

        let vs_node_index = self.virtual_storage_nodes.push_new(
            name,
            sub_name,
            nodes,
            factors,
            initial_volume,
            min_volume,
            max_volume,
            reset,
            cost,
        );

        // Link the virtual storage node to the nodes it is including
        for node_idx in nodes {
            let node = self.nodes.get_mut(node_idx)?;
            node.add_virtual_storage(vs_node_index)?;
        }

        // Add to the resolve order.
        self.resolve_order
            .push(ComponentType::VirtualStorageNode(vs_node_index));

        Ok(vs_node_index)
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

    /// Add a [`MetricSet`] to the model.
    pub fn add_metric_set(&mut self, metric_set: MetricSet) -> Result<MetricSetIndex, PywrError> {
        if let Ok(_) = self.get_metric_set_by_name(&metric_set.name()) {
            return Err(PywrError::MetricSetNameAlreadyExists(metric_set.name().to_string()));
        }

        let metric_set_idx = MetricSetIndex::new(self.metric_sets.len());
        self.metric_sets.push(metric_set);
        Ok(metric_set_idx)
    }

    /// Get a [`MetricSet'] from its index.
    pub fn get_metric_set(&self, index: MetricSetIndex) -> Result<&MetricSet, PywrError> {
        self.metric_sets.get(*index).ok_or(PywrError::MetricSetIndexNotFound)
    }

    /// Get a ['MetricSet'] by its name.
    pub fn get_metric_set_by_name(&self, name: &str) -> Result<&MetricSet, PywrError> {
        self.metric_sets
            .iter()
            .find(|&m| m.name() == name)
            .ok_or(PywrError::MetricSetNotFound(name.to_string()))
    }

    /// Get a ['MetricSetIndex'] by its name.
    pub fn get_metric_set_index_by_name(&self, name: &str) -> Result<MetricSetIndex, PywrError> {
        match self.metric_sets.iter().position(|m| m.name() == name) {
            Some(idx) => Ok(MetricSetIndex::new(idx)),
            None => Err(PywrError::MetricSetNotFound(name.to_string())),
        }
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

    /// Set the variable values on the parameter a index `['idx'].
    pub fn set_f64_parameter_variable_values(&mut self, idx: ParameterIndex, values: &[f64]) -> Result<(), PywrError> {
        match self.parameters.get_mut(*idx.deref()) {
            Some(parameter) => match parameter.as_f64_variable_mut() {
                Some(variable) => variable.set_variables(values),
                None => Err(PywrError::ParameterTypeNotVariable),
            },
            None => Err(PywrError::ParameterIndexNotFound(idx)),
        }
    }

    /// Return a vector of the current values of active variable parameters.
    pub fn get_f64_parameter_variable_values(&self) -> Vec<f64> {
        self.parameters
            .iter()
            .filter_map(|p| p.as_f64_variable().filter(|v| v.is_active()).map(|v| v.get_variables()))
            .flatten()
            .collect()
    }

    /// Set the variable values on the parameter a index `['idx'].
    pub fn set_u32_parameter_variable_values(&mut self, idx: ParameterIndex, values: &[u32]) -> Result<(), PywrError> {
        match self.parameters.get_mut(*idx.deref()) {
            Some(parameter) => match parameter.as_u32_variable_mut() {
                Some(variable) => variable.set_variables(values),
                None => Err(PywrError::ParameterTypeNotVariable),
            },
            None => Err(PywrError::ParameterIndexNotFound(idx)),
        }
    }

    /// Return a vector of the current values of active variable parameters.
    pub fn get_u32_parameter_variable_values(&self) -> Vec<u32> {
        self.parameters
            .iter()
            .filter_map(|p| p.as_u32_variable().filter(|v| v.is_active()).map(|v| v.get_variables()))
            .flatten()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::Metric;
    use crate::model::Model;
    use crate::node::{Constraint, ConstraintValue};
    use crate::parameters::{ActivationFunction, ControlCurveInterpolatedParameter, Parameter, VariableParameter};
    use crate::recorders::AssertionRecorder;
    use crate::scenario::{ScenarioGroupCollection, ScenarioIndex};
    #[cfg(feature = "clipm")]
    use crate::solvers::{ClIpmF64Solver, SimdIpmF64Solver};
    use crate::solvers::{ClpSolver, ClpSolverSettings};
    use crate::test_utils::{default_timestepper, run_all_solvers, simple_model, simple_storage_model};
    use float_cmp::{approx_eq, assert_approx_eq};
    use ndarray::{Array, Array2};
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

        let input_max_flow = parameters::ConstantParameter::new("my-constant", 10.0, None);
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
        const NUM_SCENARIOS: usize = 2;
        let model = simple_model(NUM_SCENARIOS);

        let timestepper = default_timestepper();

        let mut timings = RunTimings::default();
        let timesteps = timestepper.timesteps();
        let mut ts_iter = timesteps.iter();

        let (scenario_indices, mut current_state, mut p_internal, _r_internal) = model.setup(&timesteps).unwrap();

        let mut solvers = model.setup_solver::<ClpSolver>(&ClpSolverSettings::default()).unwrap();
        assert_eq!(current_state.len(), scenario_indices.len());

        let output_node = model.get_node_by_name("output", None).unwrap();

        for i in 0..2 {
            let ts = ts_iter.next().unwrap();
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

            for j in 0..NUM_SCENARIOS {
                let state_j = current_state.get(j).unwrap();
                let output_inflow = state_j
                    .get_network_state()
                    .get_node_in_flow(&output_node.index())
                    .unwrap();
                assert_approx_eq!(f64, output_inflow, (1.0 + i as f64 + j as f64).min(12.0));
            }
        }
    }

    #[test]
    /// Test running a simple model
    fn test_run() {
        let mut model = simple_model(10);
        let timestepper = default_timestepper();

        // Set-up assertion for "input" node
        let idx = model.get_node_by_name("input", None).unwrap().index();
        let expected = Array::from_shape_fn((366, 10), |(i, j)| (1.0 + i as f64 + j as f64).min(12.0));

        let recorder = AssertionRecorder::new("input-flow", Metric::NodeOutFlow(idx), expected.clone(), None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("link", None).unwrap().index();
        let recorder = AssertionRecorder::new("link-flow", Metric::NodeOutFlow(idx), expected.clone(), None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("output", None).unwrap().index();
        let recorder = AssertionRecorder::new("output-flow", Metric::NodeInFlow(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_parameter_index_by_name("total-demand").unwrap();
        let expected = Array2::from_elem((366, 10), 12.0);
        let recorder = AssertionRecorder::new("total-demand", Metric::ParameterValue(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model, &timestepper);
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

        // Test all solvers
        run_all_solvers(&model, &timestepper);
    }

    /// Test proportional storage derived metric.
    ///
    /// Proportional storage is a derived metric that is updated after each solve. However, a
    /// parameter may required a value for the initial time-step based on the initial volume.
    #[test]
    fn test_storage_proportional_volume() {
        let mut model = simple_storage_model();
        let timestepper = default_timestepper();

        let idx = model.get_node_by_name("reservoir", None).unwrap().index();
        let dm_idx = model.add_derived_metric(DerivedMetric::NodeProportionalVolume(idx));

        // These are the expected values for the proportional volume at the end of the time-step
        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (90.0 - 10.0 * i as f64).max(0.0) / 100.0);
        let recorder = AssertionRecorder::new(
            "reservoir-proportion-volume",
            Metric::DerivedMetric(dm_idx),
            expected,
            None,
            None,
        );
        model.add_recorder(Box::new(recorder)).unwrap();

        // Set-up a control curve that uses the proportional volume
        // This should be use the initial proportion (100%) on the first time-step, and then the previous day's end value
        let cc = ControlCurveInterpolatedParameter::new(
            "interp",
            Metric::DerivedMetric(dm_idx),
            vec![],
            vec![Metric::Constant(100.0), Metric::Constant(0.0)],
        );
        let p_idx = model.add_parameter(Box::new(cc)).unwrap();
        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (100.0 - 10.0 * i as f64).max(0.0));

        let recorder = AssertionRecorder::new("reservoir-cc", Metric::ParameterValue(p_idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model, &timestepper);
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

    #[test]
    /// Test the variable API
    fn test_variable_api() {
        let mut model = Model::default();
        let _node_index = model.add_input_node("input", None).unwrap();

        let variable = ActivationFunction::Unit { min: 0.0, max: 10.0 };
        let input_max_flow = parameters::ConstantParameter::new("my-constant", 10.0, Some(variable));

        assert!(input_max_flow.can_be_f64_variable());
        assert!(input_max_flow.is_f64_variable_active());
        assert!(input_max_flow.is_active());

        let input_max_flow_idx = model.add_parameter(Box::new(input_max_flow)).unwrap();

        // assign the new parameter to one of the nodes.
        let node = model.get_mut_node_by_name("input", None).unwrap();
        node.set_constraint(
            ConstraintValue::Metric(Metric::ParameterValue(input_max_flow_idx)),
            Constraint::MaxFlow,
        )
        .unwrap();

        let variable_values = model.get_f64_parameter_variable_values();
        assert_eq!(variable_values, vec![10.0]);

        // Update the variable values
        model
            .set_f64_parameter_variable_values(input_max_flow_idx, &[5.0])
            .unwrap();

        let variable_values = model.get_f64_parameter_variable_values();
        assert_eq!(variable_values, vec![5.0]);
    }
}
