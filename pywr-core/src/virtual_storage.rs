use crate::NodeIndex;
use crate::metric::{MetricF64, MetricF64Error, SimpleMetricF64, SimpleMetricF64Error};
use crate::network::{Network, NetworkError};
use crate::node::{NodeMeta, StorageConstraints, StorageInitialVolume};
use crate::state::{NetworkStateError, State, StateError, VirtualStorageState};
use crate::timestep::Timestep;
use chrono::{Datelike, Month, NaiveDate, NaiveDateTime};
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
    reset: VirtualStorageReset,
    reset_volume: VirtualStorageResetVolume,
    rolling_window: Option<NonZeroUsize>,
    active_period: VirtualStorageActivePeriod,
}

impl VirtualStorageBuilder {
    pub fn new(name: &str, nodes: &[NodeIndex]) -> Self {
        Self {
            name: name.to_string(),
            sub_name: None,
            nodes: nodes.to_vec(),
            factors: None,
            initial_volume: StorageInitialVolume::Absolute(0.0),
            reset: VirtualStorageReset::Never,
            reset_volume: VirtualStorageResetVolume::Initial,
            rolling_window: None,
            active_period: VirtualStorageActivePeriod::Always,
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

    pub fn reset(mut self, reset: VirtualStorageReset) -> Self {
        self.reset = reset;
        self
    }

    pub fn reset_volume(mut self, reset_volume: VirtualStorageResetVolume) -> Self {
        self.reset_volume = reset_volume;
        self
    }

    pub fn rolling_window(mut self, rolling_window: NonZeroUsize) -> Self {
        self.rolling_window = Some(rolling_window);
        self
    }

    pub fn active_period(mut self, active_period: VirtualStorageActivePeriod) -> Self {
        self.active_period = active_period;
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
            storage_constraints: StorageConstraints::new(None, None),
            reset: self.reset,
            reset_volume: self.reset_volume,
            rolling_window: self.rolling_window,
            active_period: self.active_period,
            cost: None,
        }
    }
}

/// Defines when the virtual storage volume should be reset.
pub enum VirtualStorageReset {
    Never,
    DayOfYear { day: u32, month: Month },
    NumberOfMonths { months: i32 },
}

/// When resetting the virtual storage volume, this enum defines how much volume to set.
pub enum VirtualStorageResetVolume {
    Initial,
    Max,
}

/// Active periods for a virtual storage node.
pub enum VirtualStorageActivePeriod {
    Always,
    Period {
        start_day: u32,
        start_month: Month,
        end_day: u32,
        end_month: Month,
    },
}

impl VirtualStorageActivePeriod {
    fn is_active(&self, timestep: &NaiveDate) -> bool {
        match self {
            Self::Always => true,
            Self::Period {
                start_day,
                start_month,
                end_day,
                end_month,
            } => {
                let start_month_num = start_month.number_from_month();
                let end_month_num = end_month.number_from_month();
                let current_month = timestep.month();
                let current_day = timestep.day();

                if start_month_num < end_month_num || (start_month_num == end_month_num && start_day <= end_day) {
                    // Period does not wrap around the year end
                    (current_month > start_month_num || (current_month == start_month_num && current_day >= *start_day))
                        && (current_month < end_month_num
                            || (current_month == end_month_num && current_day <= *end_day))
                } else {
                    // Period wraps around the year end
                    (current_month > start_month_num || (current_month == start_month_num && current_day >= *start_day))
                        || (current_month < end_month_num
                            || (current_month == end_month_num && current_day <= *end_day))
                }
            }
        }
    }
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
    reset_volume: VirtualStorageResetVolume,
    rolling_window: Option<NonZeroUsize>,
    active_period: VirtualStorageActivePeriod,
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

    pub fn set_min_volume_constraint(&mut self, min_volume: Option<SimpleMetricF64>) {
        self.storage_constraints.min_volume = min_volume;
    }

    pub fn set_max_volume_constraint(&mut self, max_volume: Option<SimpleMetricF64>) {
        self.storage_constraints.max_volume = max_volume;
    }

    pub fn before(&self, timestep: &Timestep, state: &mut State) -> Result<(), VirtualStorageError> {
        let do_reset = if timestep.is_first() {
            // Set the initial volume if it is the first timestep.
            true
        } else if !self.is_active(timestep) {
            // Make sure volume is reset outside the active period
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
            let initial_volume = self.initial_volume.get_absolute_initial_volume(max_volume, state)?;

            let reset_volume = match &self.reset_volume {
                VirtualStorageResetVolume::Max => max_volume,
                VirtualStorageResetVolume::Initial => initial_volume,
            };

            state.reset_virtual_storage_node_volume(self.meta.index(), reset_volume, timestep)?;

            // Reset the rolling history if defined
            if let Some(window) = self.rolling_window {
                // Initially the missing volume is distributed evenly across the window
                let initial_flow = (max_volume - initial_volume) / window.get() as f64;
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

    /// Return the available and missing volume as a tuple (available, missing).
    pub fn get_available_volume_bounds(&self, state: &State) -> Result<(f64, f64), VirtualStorageError> {
        let min_vol = self.get_min_volume(state)?;
        let max_vol = self.get_max_volume(state)?;

        let current_volume = state.get_network_state().get_virtual_storage_volume(&self.index())?;

        let available = (current_volume - min_vol).max(0.0);
        let missing = (max_vol - current_volume).max(0.0);
        Ok((available, missing))
    }

    /// Returns true if the virtual storage is active (i.e. it has no period where it is inactive) .
    pub fn is_active(&self, timestep: &Timestep) -> bool {
        self.active_period.is_active(&timestep.date.date())
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
    use crate::node::{CostAggFunc, StorageInitialVolume};
    use crate::parameters::ControlCurveInterpolatedParameter;
    use crate::recorders::{AssertionF64Recorder, AssertionFnRecorder};
    use crate::scenario::ScenarioIndex;
    use crate::test_utils::{default_timestepper, run_all_solvers, simple_model};
    use crate::timestep::{Timestep, TimestepDuration, Timestepper};
    use crate::virtual_storage::{
        VirtualStorageActivePeriod, VirtualStorageBuilder, VirtualStorageReset, months_since_last_reset,
    };
    use chrono::{Datelike, Month, NaiveDate};
    use ndarray::Array;
    use std::num::{NonZeroU64, NonZeroUsize};

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
            .reset(VirtualStorageReset::Never);

        let vs_idx = network.add_virtual_storage_node(vs_builder).unwrap();
        network
            .set_virtual_storage_max_volume("virtual-storage", None, Some(100.0.into()))
            .unwrap();

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

        // Make the input use any VS costs
        let node = network.get_mut_node_by_name("input", None).unwrap();
        node.set_cost_agg_func(Some(CostAggFunc::Max)).unwrap();

        let nodes = vec![network.get_node_index_by_name("input", None).unwrap()];
        // Virtual storage node cost is high enough to prevent any flow

        let vs_builder = VirtualStorageBuilder::new("vs", &nodes)
            .initial_volume(StorageInitialVolume::Proportional(1.0))
            .reset(VirtualStorageReset::Never);

        network.add_virtual_storage_node(vs_builder).unwrap();
        network.set_virtual_storage_cost("vs", None, Some(20.0.into())).unwrap();
        network
            .set_virtual_storage_max_volume("vs", None, Some(100.0.into()))
            .unwrap();

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
        let duration = TimestepDuration::Days(NonZeroU64::new(1).unwrap());

        let mut model = simple_model(1, Some(Timestepper::new(start, end, duration)));
        let network = model.network_mut();

        // Make the input use any VS costs
        let node = network.get_mut_node_by_name("input", None).unwrap();
        node.set_cost_agg_func(Some(CostAggFunc::Max)).unwrap();

        let nodes = vec![network.get_node_index_by_name("input", None).unwrap()];

        let vs_builder = VirtualStorageBuilder::new("vs", &nodes)
            .initial_volume(StorageInitialVolume::Proportional(1.0))
            .reset(VirtualStorageReset::NumberOfMonths { months: 1 });

        let vs_idx = network.add_virtual_storage_node(vs_builder).unwrap();
        let vs_vol_metric = network.add_derived_metric(DerivedMetric::VirtualStorageProportionalVolume(vs_idx));
        network
            .set_virtual_storage_max_volume("vs", None, Some(100.0.into()))
            .unwrap();

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
            .reset(VirtualStorageReset::Never)
            .rolling_window(NonZeroUsize::new(5).unwrap());
        let _vs = network.add_virtual_storage_node(vs_builder);
        network
            .set_virtual_storage_max_volume("virtual-storage", None, Some(2.5.into()))
            .unwrap();

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

    #[test]
    /// Test virtual storage active period
    fn test_virtual_storage_node_active_period() {
        let period = VirtualStorageActivePeriod::Period {
            start_day: 15,
            start_month: Month::March,
            end_day: 15,
            end_month: Month::September,
        };

        // Dates inside the period
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 3, 15).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 7, 8).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 9, 15).unwrap()));

        // Dates outside the period
        assert!(!period.is_active(&NaiveDate::from_ymd_opt(2016, 3, 14).unwrap()));
        assert!(!period.is_active(&NaiveDate::from_ymd_opt(2016, 9, 16).unwrap()));
        assert!(!period.is_active(&NaiveDate::from_ymd_opt(2016, 12, 31).unwrap()));
    }

    #[test]
    fn test_virtual_storage_node_active_period_wrap() {
        let period = VirtualStorageActivePeriod::Period {
            start_day: 15,
            start_month: Month::September,
            end_day: 15,
            end_month: Month::March,
        };

        // Dates inside the period
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 9, 15).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 12, 31).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2017, 1, 1).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2017, 3, 15).unwrap()));

        // Dates outside the period
        assert!(!period.is_active(&NaiveDate::from_ymd_opt(2016, 9, 14).unwrap()));
        assert!(!period.is_active(&NaiveDate::from_ymd_opt(2016, 3, 16).unwrap()));
        assert!(!period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 1).unwrap()));
    }

    #[test]
    fn test_virtual_storage_node_active_period_same_month() {
        let period = VirtualStorageActivePeriod::Period {
            start_day: 10,
            start_month: Month::June,
            end_day: 20,
            end_month: Month::June,
        };

        // Dates inside the period
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 10).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 15).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 20).unwrap()));

        // Dates outside the period
        assert!(!period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 9).unwrap()));
        assert!(!period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 21).unwrap()));
        assert!(!period.is_active(&NaiveDate::from_ymd_opt(2016, 5, 31).unwrap()));
        assert!(!period.is_active(&NaiveDate::from_ymd_opt(2016, 7, 1).unwrap()));
    }

    #[test]
    fn test_virtual_storage_node_active_period_same_month_wrap() {
        let period = VirtualStorageActivePeriod::Period {
            start_day: 20,
            start_month: Month::June,
            end_day: 10,
            end_month: Month::June,
        };

        // Dates inside the period
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 20).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 25).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 30).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 1).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 5).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 10).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 5, 31).unwrap()));
        assert!(period.is_active(&NaiveDate::from_ymd_opt(2016, 7, 1).unwrap()));

        // Dates outside the period
        assert!(!period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 19).unwrap()));
        assert!(!period.is_active(&NaiveDate::from_ymd_opt(2016, 6, 11).unwrap()));
    }
}
