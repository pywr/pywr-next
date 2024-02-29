use crate::aggregated_node::{AggregatedNode, AggregatedNodeIndex, AggregatedNodeVec, Factors};
use crate::aggregated_storage_node::{AggregatedStorageNode, AggregatedStorageNodeIndex, AggregatedStorageNodeVec};
use crate::derived_metric::{DerivedMetric, DerivedMetricIndex};
use crate::edge::{EdgeIndex, EdgeVec};
use crate::metric::Metric;
use crate::models::ModelDomain;
use crate::node::{ConstraintValue, Node, NodeVec, StorageInitialVolume};
use crate::parameters::{MultiValueParameterIndex, ParameterType, VariableConfig};
use crate::recorders::{MetricSet, MetricSetIndex, MetricSetState};
use crate::scenario::ScenarioIndex;
use crate::solvers::{MultiStateSolver, Solver, SolverFeatures, SolverTimings};
use crate::state::{ParameterStates, State, StateBuilder};
use crate::timestep::Timestep;
use crate::virtual_storage::{VirtualStorage, VirtualStorageIndex, VirtualStorageReset, VirtualStorageVec};
use crate::{parameters, recorders, IndexParameterIndex, NodeIndex, ParameterIndex, PywrError, RecorderIndex};
use rayon::prelude::*;
use std::any::Any;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::slice::{Iter, IterMut};
use std::time::Duration;
use std::time::Instant;
use tracing::info;

pub enum RunDuration {
    Running(Instant),
    Finished(Duration, usize),
}

pub struct RunTimings {
    pub global: RunDuration,
    pub parameter_calculation: Duration,
    pub recorder_saving: Duration,
    pub solve: SolverTimings,
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
    pub fn finish(&mut self, count: usize) {
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

    pub fn print_table(&self) {
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

#[derive(Clone)]
/// Internal states for each scenario and recorder.
pub struct NetworkState {
    // State by scenario
    states: Vec<State>,
    // Parameter state by scenario
    parameter_internal_states: Vec<ParameterStates>,
    // Metric set states by scenario
    metric_set_internal_states: Vec<Vec<MetricSetState>>,
}

impl NetworkState {
    pub fn state(&self, scenario_index: &ScenarioIndex) -> &State {
        &self.states[scenario_index.index]
    }

    pub fn state_mut(&mut self, scenario_index: &ScenarioIndex) -> &mut State {
        &mut self.states[scenario_index.index]
    }

    pub fn parameter_states(&self, scenario_index: &ScenarioIndex) -> &ParameterStates {
        &self.parameter_internal_states[scenario_index.index]
    }

    pub fn parameter_states_mut(&mut self, scenario_index: &ScenarioIndex) -> &mut ParameterStates {
        &mut self.parameter_internal_states[scenario_index.index]
    }

    /// Returns an iterator of immutable parameter states for each scenario.
    pub fn iter_parameter_states(&self) -> Iter<'_, ParameterStates> {
        self.parameter_internal_states.iter()
    }

    /// Returns an iterator that allows modifying the parameter states for each scenario.
    pub fn iter_parameter_states_mut(&mut self) -> IterMut<'_, ParameterStates> {
        self.parameter_internal_states.iter_mut()
    }

    pub fn all_metric_set_internal_states_mut(&mut self) -> &mut [Vec<MetricSetState>] {
        &mut self.metric_set_internal_states
    }
}

/// A Pywr network containing nodes, edges, parameters, metric sets, etc.
///
/// This struct is the main entry point for constructing a Pywr network and should be used
/// to represent a discrete system. A network can be simulated using a model and a solver. The
/// network is translated into a linear program using the [`Solver`] trait.
///
#[derive(Default)]
pub struct Network {
    nodes: NodeVec,
    edges: EdgeVec,
    aggregated_nodes: AggregatedNodeVec,
    aggregated_storage_nodes: AggregatedStorageNodeVec,
    virtual_storage_nodes: VirtualStorageVec,
    parameters: Vec<Box<dyn parameters::Parameter>>,
    index_parameters: Vec<Box<dyn parameters::IndexParameter>>,
    multi_parameters: Vec<Box<dyn parameters::MultiValueParameter>>,
    derived_metrics: Vec<DerivedMetric>,
    metric_sets: Vec<MetricSet>,
    resolve_order: Vec<ComponentType>,
    recorders: Vec<Box<dyn recorders::Recorder>>,
}

impl Network {
    pub fn nodes(&self) -> &NodeVec {
        &self.nodes
    }
    pub fn edges(&self) -> &EdgeVec {
        &self.edges
    }

    pub fn aggregated_nodes(&self) -> &AggregatedNodeVec {
        &self.aggregated_nodes
    }

    pub fn aggregated_storage_nodes(&self) -> &AggregatedStorageNodeVec {
        &self.aggregated_storage_nodes
    }

    pub fn virtual_storage_nodes(&self) -> &VirtualStorageVec {
        &self.virtual_storage_nodes
    }

    /// Setup the network and create the initial state for each scenario.
    pub fn setup_network(
        &self,
        timesteps: &[Timestep],
        scenario_indices: &[ScenarioIndex],
        num_inter_network_transfers: usize,
    ) -> Result<NetworkState, PywrError> {
        let mut states: Vec<State> = Vec::with_capacity(scenario_indices.len());
        let mut parameter_internal_states: Vec<ParameterStates> = Vec::with_capacity(scenario_indices.len());
        let mut metric_set_internal_states: Vec<_> = Vec::with_capacity(scenario_indices.len());

        for scenario_index in scenario_indices {
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

            let state_builder = StateBuilder::new(initial_node_states, self.edges.len())
                .with_virtual_storage_states(initial_virtual_storage_states)
                .with_value_parameters(initial_values_states.len())
                .with_index_parameters(initial_indices_states.len())
                .with_multi_parameters(initial_multi_param_states.len())
                .with_derived_metrics(self.derived_metrics.len())
                .with_inter_network_transfers(num_inter_network_transfers);

            let state = state_builder.build();

            states.push(state);

            parameter_internal_states.push(ParameterStates::new(
                initial_values_states,
                initial_indices_states,
                initial_multi_param_states,
            ));

            metric_set_internal_states.push(self.metric_sets.iter().map(|p| p.setup()).collect::<Vec<_>>());
        }

        Ok(NetworkState {
            states,
            parameter_internal_states,
            metric_set_internal_states,
        })
    }

    pub fn setup_recorders(&self, domain: &ModelDomain) -> Result<Vec<Option<Box<dyn Any>>>, PywrError> {
        // Setup recorders
        let mut recorder_internal_states = Vec::new();
        for recorder in &self.recorders {
            let initial_state = recorder.setup(domain, self)?;
            recorder_internal_states.push(initial_state);
        }

        Ok(recorder_internal_states)
    }

    /// Check whether a solver [`S`] has the required features to run this network.
    pub fn check_solver_features<S>(&self) -> bool
    where
        S: Solver,
    {
        let required_features = self.required_features();

        required_features.iter().all(|f| S::features().contains(f))
    }

    /// Check whether a solver [`S`] has the required features to run this network.
    pub fn check_multi_scenario_solver_features<S>(&self) -> bool
    where
        S: MultiStateSolver,
    {
        let required_features = self.required_features();

        required_features.iter().all(|f| S::features().contains(f))
    }

    pub fn setup_solver<S>(
        &self,
        scenario_indices: &[ScenarioIndex],
        settings: &S::Settings,
    ) -> Result<Vec<Box<S>>, PywrError>
    where
        S: Solver,
    {
        if !self.check_solver_features::<S>() {
            return Err(PywrError::MissingSolverFeatures);
        }

        let mut solvers = Vec::with_capacity(scenario_indices.len());

        for _scenario_index in scenario_indices {
            // Create a solver for each scenario
            let solver = S::setup(self, settings)?;
            solvers.push(solver);
        }

        Ok(solvers)
    }

    pub fn setup_multi_scenario_solver<S>(
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

    pub fn finalise(
        &self,
        metric_set_states: &mut [Vec<MetricSetState>],
        recorder_internal_states: &mut [Option<Box<dyn Any>>],
    ) -> Result<(), PywrError> {
        // Finally, save new data to the metric set

        for ms_states in metric_set_states.iter_mut() {
            for (metric_set, ms_state) in self.metric_sets.iter().zip(ms_states.iter_mut()) {
                metric_set.finalise(ms_state);
            }
        }

        // Setup recorders
        for (recorder, internal_state) in self.recorders.iter().zip(recorder_internal_states) {
            recorder.finalise(metric_set_states, internal_state)?;
        }

        Ok(())
    }

    /// Perform a single timestep mutating the current state.
    pub fn step<S>(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        solvers: &mut [Box<S>],
        state: &mut NetworkState,
        timings: &mut RunTimings,
    ) -> Result<(), PywrError>
    where
        S: Solver,
    {
        scenario_indices
            .iter()
            .zip(state.states.iter_mut())
            .zip(state.parameter_internal_states.iter_mut())
            .zip(state.metric_set_internal_states.iter_mut())
            .zip(solvers)
            .for_each(
                |((((scenario_index, current_state), p_internal_states), ms_internal_states), solver)| {
                    // TODO clear the current parameter values state (i.e. set them all to zero).

                    let start_p_calc = Instant::now();
                    self.compute_components(timestep, scenario_index, current_state, p_internal_states)
                        .unwrap();

                    // State now contains updated parameter values BUT original network state
                    timings.parameter_calculation += start_p_calc.elapsed();

                    // Solve determines the new network state
                    let solve_timings = solver.solve(self, timestep, current_state).unwrap();
                    // State now contains updated parameter values AND updated network state
                    timings.solve += solve_timings;

                    // Now run the "after" method on all components
                    let start_p_after = Instant::now();
                    self.after(
                        timestep,
                        scenario_index,
                        current_state,
                        p_internal_states,
                        ms_internal_states,
                    )
                    .unwrap();

                    timings.parameter_calculation += start_p_after.elapsed();
                },
            );

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
        state: &mut NetworkState,
        timings: &mut RunTimings,
    ) -> Result<(), PywrError>
    where
        S: Solver,
    {
        // Collect all the timings from each parallel solve
        let step_times: Vec<_> = scenario_indices
            .par_iter()
            .zip(&mut state.states)
            .zip(&mut state.parameter_internal_states)
            .zip(&mut state.metric_set_internal_states)
            .zip(solvers)
            .map(
                |((((scenario_index, current_state), p_internal_state), ms_internal_state), solver)| {
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
                    self.after(
                        timestep,
                        scenario_index,
                        current_state,
                        p_internal_state,
                        ms_internal_state,
                    )
                    .unwrap();

                    parameter_calculation += start_p_after.elapsed();

                    (parameter_calculation, solve_timings)
                },
            )
            .collect();

        // Add them all together
        for (parameter_calculation, solve_timings) in step_times.into_iter() {
            timings.parameter_calculation += parameter_calculation;
            timings.solve += solve_timings;
        }

        Ok(())
    }

    /// Perform a single timestep with a multi1-state solver mutating the current state.
    pub(crate) fn step_multi_scenario<S>(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        solver: &mut Box<S>,
        state: &mut NetworkState,
        timings: &mut RunTimings,
    ) -> Result<(), PywrError>
    where
        S: MultiStateSolver,
    {
        // First compute all the updated state

        let p_calc_timings: Vec<_> = scenario_indices
            .par_iter()
            .zip(&mut state.states)
            .zip(&mut state.parameter_internal_states)
            .map(|((scenario_index, current_state), p_internal_states)| {
                // TODO clear the current parameter values state (i.e. set them all to zero).

                let start_p_calc = Instant::now();
                self.compute_components(timestep, scenario_index, current_state, p_internal_states)
                    .unwrap();

                // State now contains updated parameter values BUT original network state
                start_p_calc.elapsed()
            })
            .collect();

        for t in p_calc_timings.into_iter() {
            timings.parameter_calculation += t;
        }

        // Now solve all the LPs simultaneously

        let solve_timings = solver.solve(self, timestep, &mut state.states).unwrap();
        // State now contains updated parameter values AND updated network state
        timings.solve += solve_timings;

        // Now run the "after" method on all components
        let p_after_timings: Vec<_> = scenario_indices
            .par_iter()
            .zip(&mut state.states)
            .zip(&mut state.parameter_internal_states)
            .zip(&mut state.metric_set_internal_states)
            .map(
                |(((scenario_index, current_state), p_internal_states), ms_internal_states)| {
                    let start_p_after = Instant::now();
                    self.after(
                        timestep,
                        scenario_index,
                        current_state,
                        p_internal_states,
                        ms_internal_states,
                    )
                    .unwrap();
                    start_p_after.elapsed()
                },
            )
            .collect();

        for t in p_after_timings.into_iter() {
            timings.parameter_calculation += t;
        }

        Ok(())
    }

    /// Calculate the set of [`SolverFeatures`] required to correctly run this network.
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

    /// Undertake calculations for network components before solve.
    ///
    /// This method iterates through the network components (nodes, parameters, etc) to perform
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

    /// Undertake "after" for network components after solve.
    ///
    /// This method iterates through the network components (nodes, parameters, etc) to perform
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
        metric_set_states: &mut [MetricSetState],
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

        // Finally, save new data to the metric set
        for (metric_set, ms_state) in self.metric_sets.iter().zip(metric_set_states.iter_mut()) {
            metric_set.save(timestep, scenario_index, self, state, ms_state)?;
        }

        Ok(())
    }

    pub fn save_recorders(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        state: &NetworkState,
        recorder_internal_states: &mut [Option<Box<dyn Any>>],
    ) -> Result<(), PywrError> {
        for (recorder, internal_state) in self.recorders.iter().zip(recorder_internal_states) {
            recorder.save(
                timestep,
                scenario_indices,
                self,
                &state.states,
                &state.metric_set_internal_states,
                internal_state,
            )?;
        }
        Ok(())
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

    pub fn get_recorder_index_by_name(&self, name: &str) -> Result<RecorderIndex, PywrError> {
        match self.recorders.iter().position(|r| r.name() == name) {
            Some(idx) => Ok(RecorderIndex::new(idx)),
            None => Err(PywrError::RecorderNotFound),
        }
    }

    pub fn get_aggregated_value(&self, name: &str, recorder_states: &[Option<Box<dyn Any>>]) -> Result<f64, PywrError> {
        match self.recorders.iter().enumerate().find(|(_, r)| r.name() == name) {
            Some((idx, recorder)) => recorder.aggregated_value(&recorder_states[idx]),
            None => Err(PywrError::RecorderNotFound),
        }
    }

    /// Add a new Node::Input to the network.
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

    /// Add a new Node::Link to the network.
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

    /// Add a new Node::Link to the network.
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

    /// Add a new Node::Link to the network.
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

    /// Add a new `aggregated_node::AggregatedNode` to the network.
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

    /// Add a new `aggregated_storage_node::AggregatedStorageNode` to the network.
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

    /// Add a new `VirtualStorage` to the network.
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
        rolling_window: Option<NonZeroUsize>,
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
            rolling_window,
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

    /// Add a `parameters::Parameter` to the network
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

    /// Add a `parameters::IndexParameter` to the network
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

    /// Add a `parameters::MultiValueParameter` to the network
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

    /// Add a [`MetricSet`] to the network.
    pub fn add_metric_set(&mut self, metric_set: MetricSet) -> Result<MetricSetIndex, PywrError> {
        if self.get_metric_set_by_name(metric_set.name()).is_ok() {
            return Err(PywrError::MetricSetNameAlreadyExists(metric_set.name().to_string()));
        }

        let metric_set_idx = MetricSetIndex::new(self.metric_sets.len());
        self.metric_sets.push(metric_set);
        Ok(metric_set_idx)
    }

    /// Get a [`MetricSet'] from its index.
    pub fn get_metric_set(&self, index: MetricSetIndex) -> Result<&MetricSet, PywrError> {
        self.metric_sets
            .get(*index)
            .ok_or(PywrError::MetricSetIndexNotFound(index))
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

    /// Add a `recorders::Recorder` to the network
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

        // The network can get in a bad state here if the edge is added to the `from_node`
        // successfully, but fails on the `to_node`.
        // Suggest to do a check before attempting to add.
        let from_node = self.nodes.get_mut(&from_node_index)?;
        from_node.add_outgoing_edge(edge_index)?;
        let to_node = self.nodes.get_mut(&to_node_index)?;
        to_node.add_incoming_edge(edge_index)?;

        Ok(edge_index)
    }

    /// Set the variable values on the parameter [`parameter_index`].
    ///
    /// This will update the internal state of the parameter with the new values for all scenarios.
    pub fn set_f64_parameter_variable_values(
        &self,
        parameter_index: ParameterIndex,
        values: &[f64],
        variable_config: &dyn VariableConfig,
        state: &mut NetworkState,
    ) -> Result<(), PywrError> {
        match self.parameters.get(*parameter_index.deref()) {
            Some(parameter) => match parameter.as_f64_variable() {
                Some(variable) => {
                    // Iterate over all scenarios and set the variable values
                    for parameter_states in state.iter_parameter_states_mut() {
                        let internal_state = parameter_states
                            .get_mut_value_state(parameter_index)
                            .ok_or(PywrError::ParameterStateNotFound(parameter_index))?;

                        variable.set_variables(values, variable_config, internal_state)?;
                    }

                    Ok(())
                }
                None => Err(PywrError::ParameterTypeNotVariable),
            },
            None => Err(PywrError::ParameterIndexNotFound(parameter_index)),
        }
    }

    /// Set the variable values on the parameter [`parameter_index`] and scenario [`scenario_index`].
    ///
    /// Only the internal state of the parameter for the given scenario will be updated.
    pub fn set_f64_parameter_variable_values_for_scenario(
        &self,
        parameter_index: ParameterIndex,
        scenario_index: ScenarioIndex,
        values: &[f64],
        variable_config: &dyn VariableConfig,
        state: &mut NetworkState,
    ) -> Result<(), PywrError> {
        match self.parameters.get(*parameter_index.deref()) {
            Some(parameter) => match parameter.as_f64_variable() {
                Some(variable) => {
                    let internal_state = state
                        .parameter_states_mut(&scenario_index)
                        .get_mut_value_state(parameter_index)
                        .ok_or(PywrError::ParameterStateNotFound(parameter_index))?;
                    variable.set_variables(values, variable_config, internal_state)
                }
                None => Err(PywrError::ParameterTypeNotVariable),
            },
            None => Err(PywrError::ParameterIndexNotFound(parameter_index)),
        }
    }

    /// Return a vector of the current values of active variable parameters.
    pub fn get_f64_parameter_variable_values_for_scenario(
        &self,
        parameter_index: ParameterIndex,
        scenario_index: ScenarioIndex,
        state: &NetworkState,
    ) -> Result<Option<Vec<f64>>, PywrError> {
        match self.parameters.get(*parameter_index.deref()) {
            Some(parameter) => match parameter.as_f64_variable() {
                Some(variable) => {
                    let internal_state = state
                        .parameter_states(&scenario_index)
                        .get_value_state(parameter_index)
                        .ok_or(PywrError::ParameterStateNotFound(parameter_index))?;

                    Ok(variable.get_variables(internal_state))
                }
                None => Err(PywrError::ParameterTypeNotVariable),
            },
            None => Err(PywrError::ParameterIndexNotFound(parameter_index)),
        }
    }

    pub fn get_f64_parameter_variable_values(
        &self,
        parameter_index: ParameterIndex,
        state: &NetworkState,
    ) -> Result<Vec<Option<Vec<f64>>>, PywrError> {
        match self.parameters.get(*parameter_index.deref()) {
            Some(parameter) => match parameter.as_f64_variable() {
                Some(variable) => {
                    let values = state
                        .iter_parameter_states()
                        .map(|parameter_states| {
                            let internal_state = parameter_states
                                .get_value_state(parameter_index)
                                .ok_or(PywrError::ParameterStateNotFound(parameter_index))?;

                            Ok(variable.get_variables(internal_state))
                        })
                        .collect::<Result<_, PywrError>>()?;

                    Ok(values)
                }
                None => Err(PywrError::ParameterTypeNotVariable),
            },
            None => Err(PywrError::ParameterIndexNotFound(parameter_index)),
        }
    }

    /// Set the variable values on the parameter [`parameter_index`].
    ///
    /// This will update the internal state of the parameter with the new values for scenarios.
    pub fn set_u32_parameter_variable_values(
        &self,
        parameter_index: ParameterIndex,
        values: &[u32],
        variable_config: &dyn VariableConfig,
        state: &mut NetworkState,
    ) -> Result<(), PywrError> {
        match self.parameters.get(*parameter_index.deref()) {
            Some(parameter) => match parameter.as_u32_variable() {
                Some(variable) => {
                    // Iterate over all scenarios and set the variable values
                    for parameter_states in state.iter_parameter_states_mut() {
                        let internal_state = parameter_states
                            .get_mut_value_state(parameter_index)
                            .ok_or(PywrError::ParameterStateNotFound(parameter_index))?;

                        variable.set_variables(values, variable_config, internal_state)?;
                    }

                    Ok(())
                }
                None => Err(PywrError::ParameterTypeNotVariable),
            },
            None => Err(PywrError::ParameterIndexNotFound(parameter_index)),
        }
    }

    /// Set the variable values on the parameter [`parameter_index`] and scenario [`scenario_index`].
    ///
    /// Only the internal state of the parameter for the given scenario will be updated.
    pub fn set_u32_parameter_variable_values_for_scenario(
        &self,
        parameter_index: ParameterIndex,
        scenario_index: ScenarioIndex,
        values: &[u32],
        variable_config: &dyn VariableConfig,
        state: &mut NetworkState,
    ) -> Result<(), PywrError> {
        match self.parameters.get(*parameter_index.deref()) {
            Some(parameter) => match parameter.as_u32_variable() {
                Some(variable) => {
                    let internal_state = state
                        .parameter_states_mut(&scenario_index)
                        .get_mut_value_state(parameter_index)
                        .ok_or(PywrError::ParameterIndexNotFound(parameter_index))?;
                    variable.set_variables(values, variable_config, internal_state)
                }
                None => Err(PywrError::ParameterTypeNotVariable),
            },
            None => Err(PywrError::ParameterIndexNotFound(parameter_index)),
        }
    }

    /// Return a vector of the current values of active variable parameters.
    pub fn get_u32_parameter_variable_values_for_scenario(
        &self,
        parameter_index: ParameterIndex,
        scenario_index: ScenarioIndex,
        state: &NetworkState,
    ) -> Result<Option<Vec<u32>>, PywrError> {
        match self.parameters.get(*parameter_index.deref()) {
            Some(parameter) => match parameter.as_u32_variable() {
                Some(variable) => {
                    let internal_state = state
                        .parameter_states(&scenario_index)
                        .get_value_state(parameter_index)
                        .ok_or(PywrError::ParameterStateNotFound(parameter_index))?;
                    Ok(variable.get_variables(internal_state))
                }
                None => Err(PywrError::ParameterTypeNotVariable),
            },
            None => Err(PywrError::ParameterIndexNotFound(parameter_index)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metric::Metric;
    use crate::network::Network;
    use crate::node::{Constraint, ConstraintValue};
    use crate::parameters::{ActivationFunction, ControlCurveInterpolatedParameter, Parameter};
    use crate::recorders::AssertionRecorder;
    use crate::scenario::{ScenarioDomain, ScenarioGroupCollection, ScenarioIndex};
    #[cfg(feature = "clipm")]
    use crate::solvers::{ClIpmF64Solver, SimdIpmF64Solver};
    use crate::solvers::{ClpSolver, ClpSolverSettings};
    use crate::test_utils::{run_all_solvers, simple_model, simple_storage_model};
    use float_cmp::assert_approx_eq;
    use ndarray::{Array, Array2};
    use std::default::Default;
    use std::ops::Deref;

    #[test]
    fn test_simple_network() {
        let mut network = Network::default();

        let input_node = network.add_input_node("input", None).unwrap();
        let link_node = network.add_link_node("link", None).unwrap();
        let output_node = network.add_output_node("output", None).unwrap();

        assert_eq!(*input_node.deref(), 0);
        assert_eq!(*link_node.deref(), 1);
        assert_eq!(*output_node.deref(), 2);

        let edge = network.connect_nodes(input_node, link_node).unwrap();
        assert_eq!(*edge.deref(), 0);
        let edge = network.connect_nodes(link_node, output_node).unwrap();
        assert_eq!(*edge.deref(), 1);

        // Now assert the internal structure is as expected.
        let input_node = network.get_node_by_name("input", None).unwrap();
        let link_node = network.get_node_by_name("link", None).unwrap();
        let output_node = network.get_node_by_name("output", None).unwrap();
        assert_eq!(input_node.get_outgoing_edges().unwrap().len(), 1);
        assert_eq!(link_node.get_incoming_edges().unwrap().len(), 1);
        assert_eq!(link_node.get_outgoing_edges().unwrap().len(), 1);
        assert_eq!(output_node.get_incoming_edges().unwrap().len(), 1);
    }

    #[test]
    /// Test the duplicate node names are not permitted.
    fn test_duplicate_node_name() {
        let mut network = Network::default();

        network.add_input_node("my-node", None).unwrap();
        // Second add with the same name
        assert_eq!(
            network.add_input_node("my-node", None),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );

        network.add_input_node("my-node", Some("a")).unwrap();
        // Second add with the same name
        assert_eq!(
            network.add_input_node("my-node", Some("a")),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );

        assert_eq!(
            network.add_link_node("my-node", None),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );

        assert_eq!(
            network.add_output_node("my-node", None),
            Err(PywrError::NodeNameAlreadyExists("my-node".to_string()))
        );

        assert_eq!(
            network.add_storage_node(
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
    /// Test adding a constant parameter to a network.
    fn test_constant_parameter() {
        let mut network = Network::default();
        let _node_index = network.add_input_node("input", None).unwrap();

        let input_max_flow = parameters::ConstantParameter::new("my-constant", 10.0);
        let parameter = network.add_parameter(Box::new(input_max_flow)).unwrap();

        // assign the new parameter to one of the nodes.
        let node = network.get_mut_node_by_name("input", None).unwrap();
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

        let mut timings = RunTimings::default();

        let mut state = model.setup::<ClpSolver>(&ClpSolverSettings::default()).unwrap();

        let output_node = model.network().get_node_by_name("output", None).unwrap();

        for i in 0..2 {
            model.step(&mut state, None, &mut timings).unwrap();

            for j in 0..NUM_SCENARIOS {
                let state_j = state.network_state().states.get(j).unwrap();
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

        // Set-up assertion for "input" node
        let idx = model.network().get_node_by_name("input", None).unwrap().index();
        let expected = Array::from_shape_fn((366, 10), |(i, j)| (1.0 + i as f64 + j as f64).min(12.0));

        let recorder = AssertionRecorder::new("input-flow", Metric::NodeOutFlow(idx), expected.clone(), None, None);
        model.network_mut().add_recorder(Box::new(recorder)).unwrap();

        let idx = model.network().get_node_by_name("link", None).unwrap().index();
        let recorder = AssertionRecorder::new("link-flow", Metric::NodeOutFlow(idx), expected.clone(), None, None);
        model.network_mut().add_recorder(Box::new(recorder)).unwrap();

        let idx = model.network().get_node_by_name("output", None).unwrap().index();
        let recorder = AssertionRecorder::new("output-flow", Metric::NodeInFlow(idx), expected, None, None);
        model.network_mut().add_recorder(Box::new(recorder)).unwrap();

        let idx = model.network().get_parameter_index_by_name("total-demand").unwrap();
        let expected = Array2::from_elem((366, 10), 12.0);
        let recorder = AssertionRecorder::new("total-demand", Metric::ParameterValue(idx), expected, None, None);
        model.network_mut().add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model);
    }

    #[test]
    fn test_run_storage() {
        let mut model = simple_storage_model();

        let network = model.network_mut();

        let idx = network.get_node_by_name("output", None).unwrap().index();

        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| if i < 10 { 10.0 } else { 0.0 });

        let recorder = AssertionRecorder::new("output-flow", Metric::NodeInFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        let idx = network.get_node_by_name("reservoir", None).unwrap().index();

        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (90.0 - 10.0 * i as f64).max(0.0));

        let recorder = AssertionRecorder::new("reservoir-volume", Metric::NodeVolume(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model);
    }

    /// Test proportional storage derived metric.
    ///
    /// Proportional storage is a derived metric that is updated after each solve. However, a
    /// parameter may required a value for the initial time-step based on the initial volume.
    #[test]
    fn test_storage_proportional_volume() {
        let mut model = simple_storage_model();
        let network = model.network_mut();
        let idx = network.get_node_by_name("reservoir", None).unwrap().index();
        let dm_idx = network.add_derived_metric(DerivedMetric::NodeProportionalVolume(idx));

        // These are the expected values for the proportional volume at the end of the time-step
        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (90.0 - 10.0 * i as f64).max(0.0) / 100.0);
        let recorder = AssertionRecorder::new(
            "reservoir-proportion-volume",
            Metric::DerivedMetric(dm_idx),
            expected,
            None,
            None,
        );
        network.add_recorder(Box::new(recorder)).unwrap();

        // Set-up a control curve that uses the proportional volume
        // This should be use the initial proportion (100%) on the first time-step, and then the previous day's end value
        let cc = ControlCurveInterpolatedParameter::new(
            "interp",
            Metric::DerivedMetric(dm_idx),
            vec![],
            vec![Metric::Constant(100.0), Metric::Constant(0.0)],
        );
        let p_idx = network.add_parameter(Box::new(cc)).unwrap();
        let expected = Array2::from_shape_fn((15, 10), |(i, _j)| (100.0 - 10.0 * i as f64).max(0.0));

        let recorder = AssertionRecorder::new("reservoir-cc", Metric::ParameterValue(p_idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model);
    }

    #[test]
    /// Test `ScenarioGroupCollection` iteration
    fn test_scenario_iteration() {
        let mut collection = ScenarioGroupCollection::default();
        collection.add_group("Scenarion A", 10);
        collection.add_group("Scenarion B", 2);
        collection.add_group("Scenarion C", 5);

        let domain: ScenarioDomain = collection.into();
        let mut iter = domain.indices().iter();

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
        let mut model = simple_model(1);

        let variable = ActivationFunction::Unit { min: 0.0, max: 10.0 };
        let input_max_flow = parameters::ConstantParameter::new("my-constant", 10.0);

        assert!(input_max_flow.can_be_f64_variable());

        let input_max_flow_idx = model.network_mut().add_parameter(Box::new(input_max_flow)).unwrap();

        // assign the new parameter to one of the nodes.
        let node = model.network_mut().get_mut_node_by_name("input", None).unwrap();
        node.set_constraint(
            ConstraintValue::Metric(Metric::ParameterValue(input_max_flow_idx)),
            Constraint::MaxFlow,
        )
        .unwrap();

        let mut state = model.setup::<ClpSolver>(&ClpSolverSettings::default()).unwrap();

        // Initially the variable value should be unset
        let variable_values = model
            .network_mut()
            .get_f64_parameter_variable_values(input_max_flow_idx, state.network_state())
            .unwrap();
        assert_eq!(variable_values, vec![None]);

        // Update the variable values
        model
            .network_mut()
            .set_f64_parameter_variable_values(input_max_flow_idx, &[5.0], &variable, state.network_state_mut())
            .unwrap();

        // After update the variable value should match what was set
        let variable_values = model
            .network_mut()
            .get_f64_parameter_variable_values(input_max_flow_idx, state.network_state())
            .unwrap();

        assert_eq!(variable_values, vec![Some(vec![5.0])]);
    }
}
