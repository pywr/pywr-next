use crate::NodeIndex;
use crate::metric::{MetricF64, MetricF64Error, SimpleMetricF64, SimpleMetricF64Error};
use crate::network::{Network, NetworkError};
use crate::node::{NodeMeta, StorageConstraints, StorageInitialVolume};
use crate::state::{NetworkStateError, State, StateError, VirtualStorageState};
use crate::timestep::Timestep;
use chrono::{Datelike, Month, NaiveDateTime};
use std::fmt;
use std::fmt::{Display, Formatter};
use std::num::NonZeroUsize;
use std::ops::{Deref, DerefMut};
use thiserror::Error;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug, Hash)]
pub struct VirtualStorageIndex(usize);

impl Deref for VirtualStorageIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for VirtualStorageIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Default)]
pub struct VirtualStorageVec {
    nodes: Vec<VirtualStorage>,
}

impl Deref for VirtualStorageVec {
    type Target = Vec<VirtualStorage>;

    fn deref(&self) -> &Self::Target {
        &self.nodes
    }
}

impl DerefMut for VirtualStorageVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.nodes
    }
}

impl VirtualStorageVec {
    pub fn get(&self, index: &VirtualStorageIndex) -> Option<&VirtualStorage> {
        self.nodes.get(index.0)
    }

    pub fn get_mut(&mut self, index: &VirtualStorageIndex) -> Option<&mut VirtualStorage> {
        self.nodes.get_mut(index.0)
    }

    pub fn push_new(&mut self, builder: VirtualStorageBuilder) -> Result<VirtualStorageIndex, NetworkError> {
        if self.nodes.iter().any(|n| n.name() == builder.name) {
            return Err(NetworkError::NodeAlreadyExists {
                name: builder.name.clone(),
                sub_name: builder.sub_name.clone(),
            });
        }

        let node_index = VirtualStorageIndex(self.nodes.len());
        let node = builder.build(node_index);
        self.nodes.push(node);
        Ok(node_index)
    }
}

/// Builder for creating a [`VirtualStorage`] node.
pub struct VirtualStorageBuilder {
    name: String,
    sub_name: Option<String>,
    nodes: Vec<NodeIndex>,
    factors: Option<Vec<f64>>,
    initial_volume: StorageInitialVolume,
    min_volume: Option<SimpleMetricF64>,
    max_volume: Option<SimpleMetricF64>,
    reset: VirtualStorageReset,
    rolling_window: Option<NonZeroUsize>,
    cost: Option<MetricF64>,
}

impl VirtualStorageBuilder {
    pub fn new(name: &str, nodes: &[NodeIndex]) -> Self {
        Self {
            name: name.to_string(),
            sub_name: None,
            nodes: nodes.to_vec(),
            factors: None,
            initial_volume: StorageInitialVolume::Absolute(0.0),
            min_volume: None,
            max_volume: None,
            reset: VirtualStorageReset::Never,
            rolling_window: None,
            cost: None,
        }
    }

    pub fn sub_name(mut self, sub_name: &str) -> Self {
        self.sub_name = Some(sub_name.to_string());
        self
    }

    pub fn factors(mut self, factors: &[f64]) -> Self {
        self.factors = Some(factors.to_vec());
        self
    }

    pub fn initial_volume(mut self, initial_volume: StorageInitialVolume) -> Self {
        self.initial_volume = initial_volume;
        self
    }

    pub fn min_volume(mut self, min_volume: Option<SimpleMetricF64>) -> Self {
        self.min_volume = min_volume;
        self
    }

    pub fn max_volume(mut self, max_volume: Option<SimpleMetricF64>) -> Self {
        self.max_volume = max_volume;
        self
    }

    pub fn reset(mut self, reset: VirtualStorageReset) -> Self {
        self.reset = reset;
        self
    }

    pub fn rolling_window(mut self, rolling_window: NonZeroUsize) -> Self {
        self.rolling_window = Some(rolling_window);
        self
    }

    pub fn cost(mut self, cost: Option<MetricF64>) -> Self {
        self.cost = cost;
        self
    }

    pub fn build(self, index: VirtualStorageIndex) -> VirtualStorage {
        // Default to unit factors if none provided
        let factors = self.factors.unwrap_or(vec![1.0; self.nodes.len()]);

        VirtualStorage {
            meta: NodeMeta::new(&index, &self.name, self.sub_name.as_deref()),
            nodes: self.nodes,
            factors,
            initial_volume: self.initial_volume,
            storage_constraints: StorageConstraints::new(self.min_volume, self.max_volume),
            reset: self.reset,
            rolling_window: self.rolling_window,
            cost: self.cost,
        }
    }
}

pub enum VirtualStorageReset {
    Never,
    DayOfYear { day: u32, month: Month },
    NumberOfMonths { months: i32 },
}

#[derive(Debug, Error)]
pub enum VirtualStorageError {
    #[error("Network state error: {0}")]
    NetworkStateError(#[from] NetworkStateError),
    #[error("State error: {0}")]
    StateError(#[from] StateError),
    #[error("Simple metric error: {0}")]
    SimpleMetricError(#[from] SimpleMetricF64Error),
}

/// A component that represents a virtual storage constraint.
///
/// Virtual storage are not part of the main network but can have their volume "used" by
/// association with real nodes. Flow through one or more nodes lowers the virtual storage
/// volume by a corresponding factor (default 1.0). Flow can be constrained in those nodes
/// if it were to violate the virtual storage's min or max volume limits.
///
/// Virtual storage volume can be reset at different frequencies. See [`VirtualStorageReset`]
/// for the choices. In addition, a rolling window can be provided as a number of time-steps.
/// Volume is recovered into the virtual storage after this number of time-steps once per time-step
/// with the oldest value added back to the volume.
pub struct VirtualStorage {
    meta: NodeMeta<VirtualStorageIndex>,
    nodes: Vec<NodeIndex>,
    factors: Vec<f64>,
    initial_volume: StorageInitialVolume,
    storage_constraints: StorageConstraints,
    reset: VirtualStorageReset,
    rolling_window: Option<NonZeroUsize>,
    cost: Option<MetricF64>,
}

impl VirtualStorage {
    pub fn name(&self) -> &str {
        self.meta.name()
    }

    /// Get a node's sub_name
    pub fn sub_name(&self) -> Option<&str> {
        self.meta.sub_name()
    }

    /// Get a node's full name
    pub fn full_name(&self) -> (&str, Option<&str>) {
        self.meta.full_name()
    }

    pub fn index(&self) -> VirtualStorageIndex {
        *self.meta.index()
    }

    pub fn default_state(&self) -> VirtualStorageState {
        VirtualStorageState::new(0.0, self.rolling_window)
    }

    pub fn get_cost(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        match &self.cost {
            None => Ok(0.0),
            Some(m) => m.get_value(network, state),
        }
    }

    pub fn set_cost(&mut self, cost: Option<MetricF64>) {
        self.cost = cost;
    }

    pub fn before(&self, timestep: &Timestep, state: &mut State) -> Result<(), VirtualStorageError> {
        let do_reset = if timestep.is_first() {
            // Set the initial volume if it is the first timestep.
            true
        } else {
            // Otherwise we check the reset condition
            match self.reset {
                VirtualStorageReset::Never => false,
                VirtualStorageReset::DayOfYear { day, month } => {
                    (timestep.date.day() == day) && (timestep.date.month() == month.number_from_month())
                }
                VirtualStorageReset::NumberOfMonths { months } => {
                    // Get the date when the virtual storage was last reset
                    match state
                        .get_network_state()
                        .get_virtual_storage_last_reset(&self.index())?
                    {
                        // Reset if last reset is more than `months` ago.
                        Some(last_reset) => months_since_last_reset(&timestep.date, &last_reset.date) >= months,
                        None => true,
                    }
                }
            }
        };

        if do_reset {
            let max_volume = self.get_max_volume(state)?;
            // Determine the initial volume
            let volume = match &self.initial_volume {
                StorageInitialVolume::Absolute(iv) => *iv,
                StorageInitialVolume::Proportional(ipc) => max_volume * ipc,
            };

            // Reset the volume
            state.reset_virtual_storage_node_volume(self.meta.index(), volume, timestep)?;
            // Reset the rolling history if defined
            if let Some(window) = self.rolling_window {
                // Initially the missing volume is distributed evenly across the window
                let initial_flow = (max_volume - volume) / window.get() as f64;
                state.reset_virtual_storage_history(self.meta.index(), initial_flow)?;
            }
        }
        // Recover any historical flows from a rolling window
        if self.rolling_window.is_some() {
            state.recover_virtual_storage_last_historical_flow(self.meta.index(), timestep)?;
        }

        Ok(())
    }

    pub fn nodes(&self) -> &[NodeIndex] {
        &self.nodes
    }

    pub fn iter_nodes_with_factors(&self) -> impl Iterator<Item = (&NodeIndex, f64)> + '_ {
        self.nodes.iter().zip(self.factors.iter()).map(|(n, f)| (n, *f))
    }

    pub fn get_min_volume(&self, state: &State) -> Result<f64, SimpleMetricF64Error> {
        self.storage_constraints
            .get_min_volume(&state.get_simple_parameter_values())
    }

    pub fn get_max_volume(&self, state: &State) -> Result<f64, SimpleMetricF64Error> {
        self.storage_constraints
            .get_max_volume(&state.get_simple_parameter_values())
    }

    pub fn get_available_volume_bounds(&self, state: &State) -> Result<(f64, f64), VirtualStorageError> {
        let min_vol = self.get_min_volume(state)?;
        let max_vol = self.get_max_volume(state)?;

        let current_volume = state.get_network_state().get_virtual_storage_volume(&self.index())?;

        let available = (current_volume - min_vol).max(0.0);
        let missing = (max_vol - current_volume).max(0.0);
        Ok((available, missing))
    }
}

/// Calculate the number of months between `current` [Timestep] and the `last_reset` [Timestep].
fn months_since_last_reset(current: &NaiveDateTime, last_reset: &NaiveDateTime) -> i32 {
    (current.year() - last_reset.year()) * 12 + current.month() as i32 - last_reset.month() as i32
}

#[cfg(test)]
mod tests {
    use crate::derived_metric::DerivedMetric;
    use crate::metric::MetricF64;
    use crate::models::Model;
    use crate::network::Network;
    use crate::node::StorageInitialVolume;
    use crate::parameters::ControlCurveInterpolatedParameter;
    use crate::recorders::{AssertionF64Recorder, AssertionFnRecorder};
    use crate::scenario::ScenarioIndex;
    use crate::test_utils::{default_timestepper, run_all_solvers, simple_model};
    use crate::timestep::{Timestep, TimestepDuration, Timestepper};
    use crate::virtual_storage::{VirtualStorageBuilder, VirtualStorageReset, months_since_last_reset};
    use chrono::{Datelike, NaiveDate};
    use ndarray::Array;
    use std::num::NonZeroUsize;

    /// Test the calculation of number of months since last reset
    #[test]
    fn test_months_since_last_reset() {
        let current = NaiveDate::from_ymd_opt(2022, 12, 31)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let last_reset = NaiveDate::from_ymd_opt(2022, 12, 31)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert_eq!(months_since_last_reset(&current, &last_reset), 0);

        let current = NaiveDate::from_ymd_opt(2023, 12, 31)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let last_reset = NaiveDate::from_ymd_opt(2022, 12, 31)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert_eq!(months_since_last_reset(&current, &last_reset), 12);

        let current = NaiveDate::from_ymd_opt(2023, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let last_reset = NaiveDate::from_ymd_opt(2022, 12, 31)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert_eq!(months_since_last_reset(&current, &last_reset), 1);

        let current = NaiveDate::from_ymd_opt(2022, 12, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let last_reset = NaiveDate::from_ymd_opt(2022, 12, 31)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        assert_eq!(months_since_last_reset(&current, &last_reset), 0);
    }

    /// Test the virtual storage constraints
    #[test]
    fn test_basic_virtual_storage() {
        let mut network = Network::default();

        let input_node = network.add_input_node("input", None).unwrap();
        let link_node0 = network.add_link_node("link", Some("0")).unwrap();
        let output_node0 = network.add_output_node("output", Some("0")).unwrap();

        network.connect_nodes(input_node, link_node0).unwrap();
        network.connect_nodes(link_node0, output_node0).unwrap();

        let link_node1 = network.add_link_node("link", Some("1")).unwrap();
        let output_node1 = network.add_output_node("output", Some("1")).unwrap();

        network.connect_nodes(input_node, link_node1).unwrap();
        network.connect_nodes(link_node1, output_node1).unwrap();

        // Virtual storage with contributions from link-node0 than link-node1
        let vs_builder = VirtualStorageBuilder::new("virtual-storage", &[link_node0, link_node1])
            .factors(&[2.0, 1.0])
            .initial_volume(StorageInitialVolume::Absolute(100.0))
            .min_volume(Some(0.0.into()))
            .max_volume(Some(100.0.into()))
            .reset(VirtualStorageReset::Never)
            .cost(None);

        let vs_idx = network.add_virtual_storage_node(vs_builder).unwrap();

        // Setup a demand on output-0 and output-1
        for sub_name in &["0", "1"] {
            let output_node = network.get_mut_node_by_name("output", Some(sub_name)).unwrap();
            output_node.set_max_flow_constraint(Some(10.0.into())).unwrap();
            output_node.set_cost(Some((-10.0).into()));
        }

        // With a demand of 10 on each link node. The virtual storage will deplete at a rate of
        // 30 per day.
        let expected_vol = |ts: &Timestep, _si: &ScenarioIndex| (70.0 - ts.index as f64 * 30.0).max(0.0);
        let recorder = AssertionFnRecorder::new(
            "vs-volume",
            MetricF64::VirtualStorageVolume(vs_idx),
            expected_vol,
            None,
            None,
        );
        network.add_recorder(Box::new(recorder)).unwrap();
        // Set-up assertion for "link" node
        let idx = network.get_node_by_name("link", Some("0")).unwrap().index();
        let expected = |ts: &Timestep, _si: &ScenarioIndex| {
            if ts.index < 3 { 10.0 } else { 0.0 }
        };
        let recorder = AssertionFnRecorder::new("link-0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Set-up assertion for "input" node
        let idx = network.get_node_by_name("link", Some("1")).unwrap().index();
        let expected = |ts: &Timestep, _si: &ScenarioIndex| {
            if ts.index < 4 { 10.0 } else { 0.0 }
        };
        let recorder = AssertionFnRecorder::new("link-1-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        let domain = default_timestepper().try_into().unwrap();
        let model = Model::new(domain, network);
        // Test all solvers
        run_all_solvers(&model, &["ipm-ocl", "ipm-simd"], &[], &[]);
    }

    #[test]
    /// Test virtual storage node costs
    fn test_virtual_storage_node_costs() {
        let mut model = simple_model(1, None);
        let network = model.network_mut();

        let nodes = vec![network.get_node_index_by_name("input", None).unwrap()];
        // Virtual storage node cost is high enough to prevent any flow

        let vs_builder = VirtualStorageBuilder::new("vs", &nodes)
            .initial_volume(StorageInitialVolume::Proportional(1.0))
            .min_volume(Some(0.0.into()))
            .max_volume(Some(100.0.into()))
            .reset(VirtualStorageReset::Never)
            .cost(Some(20.0.into()));

        network.add_virtual_storage_node(vs_builder).unwrap();

        let expected = Array::zeros((366, 1));

        let idx = network.get_node_by_name("output", None).unwrap().index();
        let recorder = AssertionF64Recorder::new("output-flow", MetricF64::NodeInFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model, &["ipm-ocl", "ipm-simd"], &[], &[]);
    }

    #[test]
    /// Virtual storage node resets every month. This test will check that a parameter which
    /// uses the derived proportional volume receives the correct value after each reset.
    fn test_virtual_storage_node_cost_dynamic() {
        // This test needs to be run over a period of time to see the cost change
        let start = NaiveDate::from_ymd_opt(2020, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let end = NaiveDate::from_ymd_opt(2020, 12, 31)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let duration = TimestepDuration::Days(1);

        let mut model = simple_model(1, Some(Timestepper::new(start, end, duration)));
        let network = model.network_mut();

        let nodes = vec![network.get_node_index_by_name("input", None).unwrap()];

        let vs_builder = VirtualStorageBuilder::new("vs", &nodes)
            .initial_volume(StorageInitialVolume::Proportional(1.0))
            .min_volume(Some(0.0.into()))
            .max_volume(Some(100.0.into()))
            .reset(VirtualStorageReset::NumberOfMonths { months: 1 });

        let vs_idx = network.add_virtual_storage_node(vs_builder).unwrap();
        let vs_vol_metric = network.add_derived_metric(DerivedMetric::VirtualStorageProportionalVolume(vs_idx));

        // Virtual storage node cost increases with decreasing volume
        let cost_param = ControlCurveInterpolatedParameter::new(
            "cost".into(),
            vs_vol_metric.into(),
            vec![],
            vec![0.0.into(), 20.0.into()],
        );

        let cost_param = network.add_parameter(Box::new(cost_param)).unwrap();

        network
            .set_virtual_storage_node_cost("vs", None, Some(cost_param.into()))
            .unwrap();

        let expected = |ts: &Timestep, _si: &ScenarioIndex| {
            // Calculate the current volume within each month
            // This should be the absolute volume at the start of each, which is then used
            // to calculate the cost.
            let mut volume = 100.0;
            for dom in 1..ts.date.day() {
                let demand_met = (1.0 + ts.index as f64 - (ts.date.day() - dom) as f64).min(12.0);
                volume -= demand_met;
            }

            if volume >= 50.0 {
                (1.0 + ts.index as f64).min(12.0)
            } else {
                0.0
            }
        };
        let idx = network.get_node_by_name("output", None).unwrap().index();
        let recorder = AssertionFnRecorder::new("output-flow", MetricF64::NodeInFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model, &["ipm-ocl", "ipm-simd"], &[], &[]);
    }

    #[test]
    /// Test virtual storage rolling window constraint
    fn test_virtual_storage_node_rolling_constraint() {
        let mut model = simple_model(1, None);
        let network = model.network_mut();

        let nodes = vec![network.get_node_index_by_name("input", None).unwrap()];

        // Virtual storage with contributions from input
        // Max volume is 2.5 and is assumed to start full
        let vs_builder = VirtualStorageBuilder::new("virtual-storage", &nodes)
            .factors(&[1.0])
            .initial_volume(StorageInitialVolume::Absolute(2.5))
            .min_volume(Some(0.0.into()))
            .max_volume(Some(2.5.into()))
            .reset(VirtualStorageReset::Never)
            .rolling_window(NonZeroUsize::new(5).unwrap())
            .cost(None);
        let _vs = network.add_virtual_storage_node(vs_builder);

        // Expected values will follow a pattern set by the first few time-steps
        let expected = |ts: &Timestep, _si: &ScenarioIndex| {
            match ts.index % 5 {
                //                               Vol   Abs   Recovered = New vol.
                0 => 1.0, // End of day licence: 2.5 - 1.0 + 0.0 = 1.5
                1 => 1.5, // End of day licence: 1.5 - 1.5 + 0.0 = 0.0
                2 => 0.0, // End of day licence: 0.0 - 0.0 + 0.0 = 0.0
                3 => 0.0, // End of day licence: 0.0 - 0.0 + 0.0 = 0.0
                4 => 0.0, // End of day licence: 0.0 - 0.0 + 0.0 = 0.0
                _ => panic!("Unexpected timestep index"),
            }
        };
        let idx = network.get_node_by_name("output", None).unwrap().index();
        let recorder = AssertionFnRecorder::new("output-flow", MetricF64::NodeInFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model, &["ipm-ocl", "ipm-simd"], &[], &[]);
    }
}
