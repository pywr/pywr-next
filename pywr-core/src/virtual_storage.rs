use crate::NodeIndex;
use crate::aggregated_node::RelationshipBuildError;
use crate::metric::{MetricF64, MetricF64Error, MetricF64ResolutionError, SimpleMetricF64Error, UnresolvedMetricF64};
use crate::network::{Network, ResolutionMaps, VirtualStorageIndex};
use crate::node::{NodeMeta, StorageConstraints, StorageInitialVolume, UnresolvedNode, UnresolvedStorageInitialVolume};
use crate::state::{NetworkStateError, State, StateError, VirtualStorageState};
use crate::timestep::Timestep;
use chrono::{Datelike, Month, NaiveDate, NaiveDateTime};
use std::num::NonZeroUsize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VirtualStorageNodeBuilderError {
    #[error("Index not found in resolution map.")]
    IndexNotFound,
    #[error("Could not resolve f64 metric for `{attr}` attribute: {source}")]
    ResolveMetricF64Error {
        attr: String,
        #[source]
        source: MetricF64ResolutionError,
    },
    #[error("Could not simplify f64 metric for `{attr}`: {source}")]
    CouldNotSimplifyMetricF64 {
        attr: String,
        #[source]
        source: MetricF64Error,
    },
    #[error("Reference to node not found.")]
    NodeIndexNotFound { node: UnresolvedNode },
    #[error("Error building relationship: {0}")]
    RelationshipBuildError(#[from] RelationshipBuildError),
}

/// Builder for creating a [`VirtualStorageNode`] node.
#[derive(Debug)]
pub struct VirtualStorageNodeBuilder {
    name: UnresolvedNode,
    nodes: Vec<UnresolvedNode>,
    factors: Option<Vec<f64>>,
    initial_volume: UnresolvedStorageInitialVolume,
    reset: VirtualStorageReset,
    reset_volume: VirtualStorageResetVolume,
    rolling_window: Option<NonZeroUsize>,
    active_period: VirtualStorageActivePeriod,
    cost: Option<UnresolvedMetricF64>,
    max_volume: Option<UnresolvedMetricF64>,
    min_volume: Option<UnresolvedMetricF64>,
}

impl VirtualStorageNodeBuilder {
    pub fn new(name: &str, nodes: &[UnresolvedNode]) -> Self {
        let name = UnresolvedNode::new(name, None);
        Self {
            name,
            nodes: nodes.to_vec(),
            factors: None,
            initial_volume: UnresolvedStorageInitialVolume::Absolute(0.0),
            reset: VirtualStorageReset::Never,
            reset_volume: VirtualStorageResetVolume::Initial,
            rolling_window: None,
            active_period: VirtualStorageActivePeriod::Always,
            cost: None,
            max_volume: None,
            min_volume: None,
        }
    }

    pub fn name(&self) -> &UnresolvedNode {
        &self.name
    }

    /// The slice of regular node names linked to the virtual storage builder.
    pub fn nodes(&self) -> &[UnresolvedNode] {
        &self.nodes
    }

    pub fn sub_name(&mut self, sub_name: &str) -> &mut Self {
        self.name.set_sub_name(Some(sub_name));
        self
    }

    pub fn factors(&mut self, factors: &[f64]) -> &mut Self {
        self.factors = Some(factors.to_vec());
        self
    }

    pub fn initial_volume(&mut self, initial_volume: UnresolvedStorageInitialVolume) -> &mut Self {
        self.initial_volume = initial_volume;
        self
    }

    pub fn cost(&mut self, cost: UnresolvedMetricF64) -> &mut Self {
        self.cost = Some(cost);
        self
    }

    pub fn max_volume(&mut self, max_volume: UnresolvedMetricF64) -> &mut Self {
        self.max_volume = Some(max_volume);
        self
    }

    pub fn min_volume(&mut self, min_volume: UnresolvedMetricF64) -> &mut Self {
        self.min_volume = Some(min_volume);
        self
    }

    pub fn reset(&mut self, reset: VirtualStorageReset) -> &mut Self {
        self.reset = reset;
        self
    }

    pub fn reset_volume(&mut self, reset_volume: VirtualStorageResetVolume) -> &mut Self {
        self.reset_volume = reset_volume;
        self
    }

    pub fn rolling_window(&mut self, rolling_window: NonZeroUsize) -> &mut Self {
        self.rolling_window = Some(rolling_window);
        self
    }

    pub fn active_period(&mut self, active_period: VirtualStorageActivePeriod) -> &mut Self {
        self.active_period = active_period;
        self
    }

    /// Build a [`StorageConstraints`] from the builder.
    fn build_storage_constraints(
        &self,
        resolution_maps: &ResolutionMaps,
    ) -> Result<StorageConstraints, VirtualStorageNodeBuilderError> {
        let min_volume = self
            .min_volume
            .as_ref()
            .map(|min_volume| {
                min_volume
                    .resolve(resolution_maps)
                    .map_err(|source| VirtualStorageNodeBuilderError::ResolveMetricF64Error {
                        attr: "min_volume".to_string(),
                        source,
                    })?
                    .try_into()
                    .map_err(|source| VirtualStorageNodeBuilderError::CouldNotSimplifyMetricF64 {
                        attr: "max_volume".to_string(),
                        source,
                    })
            })
            .transpose()?;

        let max_volume = self
            .max_volume
            .as_ref()
            .map(|max_volume| {
                max_volume
                    .resolve(resolution_maps)
                    .map_err(|source| VirtualStorageNodeBuilderError::ResolveMetricF64Error {
                        attr: "max_volume".to_string(),
                        source,
                    })?
                    .try_into()
                    .map_err(|source| VirtualStorageNodeBuilderError::CouldNotSimplifyMetricF64 {
                        attr: "max_volume".to_string(),
                        source,
                    })
            })
            .transpose()?;

        let storage_constraints = StorageConstraints::new(min_volume, max_volume);

        Ok(storage_constraints)
    }

    fn build_storage_initial_volume(
        &self,
        resolution_maps: &ResolutionMaps,
    ) -> Result<StorageInitialVolume, VirtualStorageNodeBuilderError> {
        match &self.initial_volume {
            UnresolvedStorageInitialVolume::Absolute(iv) => Ok(StorageInitialVolume::Absolute(*iv)),
            UnresolvedStorageInitialVolume::Proportional(iv) => Ok(StorageInitialVolume::Proportional(*iv)),
            UnresolvedStorageInitialVolume::DistributedAbsolute {
                absolute,
                prior_max_volume,
            } => {
                let prior_max_volume = prior_max_volume
                    .resolve(resolution_maps)
                    .map_err(|source| VirtualStorageNodeBuilderError::ResolveMetricF64Error {
                        attr: "prior_max_volume".to_string(),
                        source,
                    })?
                    .try_into()
                    .map_err(|source| VirtualStorageNodeBuilderError::CouldNotSimplifyMetricF64 {
                        attr: "prior_max_volume".to_string(),
                        source,
                    })?;
                Ok(StorageInitialVolume::DistributedAbsolute {
                    absolute: *absolute,
                    prior_max_volume,
                })
            }
            UnresolvedStorageInitialVolume::DistributedProportional {
                proportion,
                total_volume,
                prior_max_volume,
            } => {
                let total_volume = total_volume
                    .resolve(resolution_maps)
                    .map_err(|source| VirtualStorageNodeBuilderError::ResolveMetricF64Error {
                        attr: "total_volume".to_string(),
                        source,
                    })?
                    .try_into()
                    .map_err(|source| VirtualStorageNodeBuilderError::CouldNotSimplifyMetricF64 {
                        attr: "total_volume".to_string(),
                        source,
                    })?;
                let prior_max_volume = prior_max_volume
                    .resolve(resolution_maps)
                    .map_err(|source| VirtualStorageNodeBuilderError::ResolveMetricF64Error {
                        attr: "prior_max_volume".to_string(),
                        source,
                    })?
                    .try_into()
                    .map_err(|source| VirtualStorageNodeBuilderError::CouldNotSimplifyMetricF64 {
                        attr: "prior_max_volume".to_string(),
                        source,
                    })?;

                Ok(StorageInitialVolume::DistributedProportional {
                    total_volume,
                    proportion: *proportion,
                    prior_max_volume,
                })
            }
        }
    }

    pub fn build(
        &self,
        resolution_maps: &ResolutionMaps,
    ) -> Result<VirtualStorageNode, VirtualStorageNodeBuilderError> {
        let index = resolution_maps
            .virtual_storage_node
            .get(&self.name)
            .ok_or(VirtualStorageNodeBuilderError::IndexNotFound)?;
        let meta = NodeMeta::from_unresolved_name(self.name.clone(), *index);

        // Default to unit factors if none provided
        let factors = self.factors.clone().unwrap_or_else(|| vec![1.0; self.nodes.len()]);
        let nodes = self
            .nodes
            .iter()
            .map(|unresolved| {
                resolution_maps.nodes.get(unresolved).copied().ok_or_else(|| {
                    VirtualStorageNodeBuilderError::NodeIndexNotFound {
                        node: unresolved.clone(),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let cost = self
            .cost
            .as_ref()
            .map(|cost| cost.resolve(resolution_maps))
            .transpose()
            .map_err(|source| VirtualStorageNodeBuilderError::ResolveMetricF64Error {
                attr: "cost".to_string(),
                source,
            })?;

        let vs = VirtualStorageNode {
            meta,
            nodes,
            factors,
            initial_volume: self.build_storage_initial_volume(resolution_maps)?,
            storage_constraints: self.build_storage_constraints(resolution_maps)?,
            reset: self.reset.clone(),
            reset_volume: self.reset_volume.clone(),
            rolling_window: self.rolling_window,
            active_period: self.active_period.clone(),
            cost,
        };

        Ok(vs)
    }
}

/// Defines when the virtual storage volume should be reset.
#[derive(Debug, Clone)]
pub enum VirtualStorageReset {
    Never,
    DayOfYear { day: u32, month: Month },
    NumberOfMonths { months: i32 },
}

/// When resetting the virtual storage volume, this enum defines how much volume to set.
#[derive(Debug, Clone)]
pub enum VirtualStorageResetVolume {
    Initial,
    Max,
}

/// Active periods for a virtual storage node.
#[derive(Debug, Clone)]
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
#[derive(Debug)]
pub struct VirtualStorageNode {
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

impl VirtualStorageNode {
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
        VirtualStorageState::new(0.0, 0.0, self.rolling_window)
    }

    pub fn get_cost(&self, network: &Network, state: &State) -> Result<f64, MetricF64Error> {
        match &self.cost {
            None => Ok(0.0),
            Some(m) => m.get_value(network, state),
        }
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

            state.reset_virtual_storage_node_volume(self.meta.index(), reset_volume, timestep, max_volume)?;

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
    use crate::metric::UnresolvedMetricF64;
    use crate::models::ModelBuilder;
    use crate::network::NetworkBuilder;
    use crate::node::{CostAggFunc, NodeBuilder, NodeType, UnresolvedNode, UnresolvedStorageInitialVolume};
    use crate::parameters::ControlCurveInterpolatedParameterBuilder;
    use crate::recorders::{AssertionF64RecorderBuilder, AssertionFnRecorderBuilder};
    use crate::scenario::ScenarioIndex;
    use crate::test_utils::{default_domain_builder, run_all_solvers, simple_model};
    use crate::timestep::{TimeDomainBuilder, Timestep, TimestepDuration};
    use crate::virtual_storage::{
        VirtualStorageActivePeriod, VirtualStorageNodeBuilder, VirtualStorageReset, months_since_last_reset,
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
        let mut network_builder = NetworkBuilder::default();

        let input_node = NodeBuilder::new("input", NodeType::Input);
        network_builder.node(input_node);

        let mut link_node0 = NodeBuilder::new("link", NodeType::Link);
        link_node0.sub_name("0");
        network_builder.node(link_node0);

        let mut output_node0 = NodeBuilder::new("output", NodeType::Output);
        output_node0.sub_name("0").max_flow(10.0.into()).cost((-10.0).into());
        network_builder.node(output_node0);

        network_builder.connect("input", UnresolvedNode::new("link", Some("0")));
        network_builder.connect(
            UnresolvedNode::new("link", Some("0")),
            UnresolvedNode::new("output", Some("0")),
        );

        let mut link_node1 = NodeBuilder::new("link", NodeType::Link);
        link_node1.sub_name("1");
        network_builder.node(link_node1);

        let mut output_node1 = NodeBuilder::new("output", NodeType::Output);
        output_node1.sub_name("1").max_flow(10.0.into()).cost((-10.0).into());
        network_builder.node(output_node1);

        network_builder.connect("input", UnresolvedNode::new("link", Some("1")));
        network_builder.connect(
            UnresolvedNode::new("link", Some("1")),
            UnresolvedNode::new("output", Some("1")),
        );

        // Virtual storage with contributions from link-node0 than link-node1
        let mut vs_builder = VirtualStorageNodeBuilder::new(
            "virtual-storage",
            &[
                UnresolvedNode::new("link", Some("0")),
                UnresolvedNode::new("link", Some("1")),
            ],
        );

        vs_builder
            .factors(&[2.0, 1.0])
            .initial_volume(UnresolvedStorageInitialVolume::Absolute(100.0))
            .reset(VirtualStorageReset::Never)
            .max_volume(100.0.into());

        network_builder.virtual_storage_node(vs_builder);

        // With a demand of 10 on each link node. The virtual storage will deplete at a rate of
        // 30 per day.
        let expected_vol = |ts: &Timestep, _si: &ScenarioIndex| (70.0 - ts.index as f64 * 30.0).max(0.0);
        let recorder = AssertionFnRecorderBuilder::new(
            "vs-volume",
            UnresolvedMetricF64::VirtualStorageVolume("virtual-storage".into()),
            expected_vol,
        );

        network_builder.recorder(Box::new(recorder));

        // Set-up assertion for "link" node
        let expected = |ts: &Timestep, _si: &ScenarioIndex| {
            if ts.index < 3 { 10.0 } else { 0.0 }
        };
        let recorder = AssertionFnRecorderBuilder::new(
            "link-0-flow",
            UnresolvedMetricF64::NodeOutFlow(UnresolvedNode::new("link", Some("0"))),
            expected,
        );
        network_builder.recorder(Box::new(recorder));

        // Set-up assertion for "input" node
        let expected = |ts: &Timestep, _si: &ScenarioIndex| {
            if ts.index < 4 { 10.0 } else { 0.0 }
        };
        let recorder = AssertionFnRecorderBuilder::new(
            "link-1-flow",
            UnresolvedMetricF64::NodeOutFlow(UnresolvedNode::new("link", Some("1"))),
            expected,
        );
        network_builder.recorder(Box::new(recorder));

        let domain = default_domain_builder();
        let model = ModelBuilder::new(domain, network_builder).build().unwrap();
        // Test all solvers
        run_all_solvers(&model, &["ipm-ocl", "ipm-simd"], &[], &[]);
    }

    #[test]
    /// Test virtual storage node costs
    fn test_virtual_storage_node_costs() {
        let mut model_builder = simple_model(1, None);
        let network_builder = model_builder.network_builder();

        // Make the input use any VS costs
        let name = "input".into();
        let node = network_builder.node_builder(&name).unwrap();
        node.cost_agg_func(CostAggFunc::Max);

        let nodes = vec!["input".into()];
        // Virtual storage node cost is high enough to prevent any flow

        let mut vs_builder = VirtualStorageNodeBuilder::new("vs", &nodes);
        vs_builder
            .initial_volume(UnresolvedStorageInitialVolume::Proportional(1.0))
            .reset(VirtualStorageReset::Never)
            .cost(20.0.into())
            .max_volume(100.0.into());

        network_builder.virtual_storage_node(vs_builder);

        let expected = Array::zeros((366, 1));

        let recorder = AssertionF64RecorderBuilder::new(
            "output-flow",
            UnresolvedMetricF64::NodeInFlow("output".into()),
            expected,
        );
        network_builder.recorder(Box::new(recorder));

        let model = model_builder.build().unwrap();

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

        let mut model_builder = simple_model(1, Some(TimeDomainBuilder::new(start, end, duration)));
        let network_builder = model_builder.network_builder();

        // Make the input use any VS costs
        let name = "input".into();
        let node = network_builder.node_builder(&name).unwrap();
        node.cost_agg_func(CostAggFunc::Max);

        let mut vs_builder = VirtualStorageNodeBuilder::new("vs", &["input".into()]);
        vs_builder
            .initial_volume(UnresolvedStorageInitialVolume::Proportional(1.0))
            .reset(VirtualStorageReset::NumberOfMonths { months: 1 })
            .cost(UnresolvedMetricF64::new_parameter_before("cost"))
            .max_volume(100.0.into());

        network_builder.virtual_storage_node(vs_builder);

        // Virtual storage node cost increases with decreasing volume
        let mut cost_param = ControlCurveInterpolatedParameterBuilder::new(
            "cost".into(),
            UnresolvedMetricF64::VirtualStorageProportionalVolume("vs".into()),
        );

        cost_param.value(0.0.into()).value(20.0.into());

        network_builder.parameters().f64(Box::new(cost_param));

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

        let recorder = AssertionFnRecorderBuilder::new(
            "output-flow",
            UnresolvedMetricF64::NodeInFlow("output".into()),
            expected,
        );
        network_builder.recorder(Box::new(recorder));

        let model = model_builder.build().unwrap();

        // Test all solvers
        run_all_solvers(&model, &["ipm-ocl", "ipm-simd"], &[], &[]);
    }

    #[test]
    /// Test virtual storage rolling window constraint
    fn test_virtual_storage_node_rolling_constraint() {
        let mut model_builder = simple_model(1, None);
        let network_builder = model_builder.network_builder();

        // Virtual storage with contributions from input
        // Max volume is 2.5 and is assumed to start full
        let mut vs_builder = VirtualStorageNodeBuilder::new("virtual-storage", &["input".into()]);
        vs_builder
            .factors(&[1.0])
            .initial_volume(UnresolvedStorageInitialVolume::Absolute(2.5))
            .reset(VirtualStorageReset::Never)
            .rolling_window(NonZeroUsize::new(5).unwrap())
            .max_volume(2.5.into());

        network_builder.virtual_storage_node(vs_builder);

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

        let recorder = AssertionFnRecorderBuilder::new(
            "output-flow",
            UnresolvedMetricF64::NodeInFlow("output".into()),
            expected,
        );
        network_builder.recorder(Box::new(recorder));

        let model = model_builder.build().unwrap();

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
