use crate::aggregated_storage_node::AggregatedStorageNodeIndex;
use crate::metric::Metric;
use crate::network::Network;
use crate::node::NodeIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::utils::hydropower_calculation;
use crate::virtual_storage::VirtualStorageIndex;
use crate::PywrError;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Deref;

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct DerivedMetricIndex(usize);

impl Deref for DerivedMetricIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerivedMetricIndex {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }
}

impl Display for DerivedMetricIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Turbine data to calculate the power in the `PowerFromNodeFlow` metric.
#[derive(Clone, Debug, PartialEq)]
pub struct TurbineData {
    // The turbine elevation
    pub elevation: f64,
    // The turbine relative efficiency (0-1)
    pub efficiency: f64,
    // The water elevation above the turbine
    pub water_elevation: Option<Metric>,
    // The water density
    pub water_density: f64,
    /// A factor used to transform the units of flow to be compatible with the hydropower equation
    pub flow_unit_conversion: f64,
    /// A factor used to transform the units of total energy
    pub energy_unit_conversion: f64,
}

/// Derived metrics are updated after the model is solved.
///
/// These metrics are "derived" from node states (e.g. volume, flow) and must be updated
/// after those states have been updated. This should happen after the model is solved. The values
/// are then available in this state for the next time-step.
#[derive(Clone, Debug, PartialEq)]
pub enum DerivedMetric {
    NodeInFlowDeficit(NodeIndex),
    NodeProportionalVolume(NodeIndex),
    AggregatedNodeProportionalVolume(AggregatedStorageNodeIndex),
    VirtualStorageProportionalVolume(VirtualStorageIndex),
    PowerFromNodeFlow(NodeIndex, TurbineData),
}

impl DerivedMetric {
    pub fn before(&self, timestep: &Timestep, network: &Network, state: &State) -> Result<Option<f64>, PywrError> {
        // On the first time-step set the initial value
        if timestep.is_first() {
            self.compute(network, state).map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn compute(&self, network: &Network, state: &State) -> Result<f64, PywrError> {
        match self {
            Self::NodeProportionalVolume(idx) => {
                let max_volume = network.get_node(idx)?.get_current_max_volume(network, state)?;
                Ok(state
                    .get_network_state()
                    .get_node_proportional_volume(idx, max_volume)?)
            }
            Self::VirtualStorageProportionalVolume(idx) => {
                let max_volume = network.get_virtual_storage_node(idx)?.get_max_volume(network, state)?;
                Ok(state
                    .get_network_state()
                    .get_virtual_storage_proportional_volume(*idx, max_volume)?)
            }
            Self::AggregatedNodeProportionalVolume(idx) => {
                let node = network.get_aggregated_storage_node(idx)?;
                let volume: f64 = node
                    .nodes
                    .iter()
                    .map(|idx| state.get_network_state().get_node_volume(idx))
                    .sum::<Result<_, _>>()?;

                let max_volume: f64 = node
                    .nodes
                    .iter()
                    .map(|idx| network.get_node(idx)?.get_current_max_volume(network, state))
                    .sum::<Result<_, _>>()?;
                // TODO handle divide by zero
                Ok(volume / max_volume)
            }
            Self::NodeInFlowDeficit(idx) => {
                let node = network.get_node(idx)?;
                let flow = state.get_network_state().get_node_in_flow(idx)?;
                let max_flow = node.get_current_max_flow(network, state)?;
                Ok(max_flow - flow)
            }
            Self::PowerFromNodeFlow(idx, turbine_data) => {
                let flow = state.get_network_state().get_node_in_flow(idx)?;

                // Calculate the head (the head may be negative)
                let head = if let Some(water_elevation) = &turbine_data.water_elevation {
                    water_elevation.get_value(network, state)? - turbine_data.elevation
                } else {
                    turbine_data.elevation
                }
                .max(0.0);

                Ok(hydropower_calculation(
                    flow,
                    head,
                    turbine_data.elevation,
                    turbine_data.efficiency,
                    turbine_data.flow_unit_conversion,
                    turbine_data.energy_unit_conversion,
                    turbine_data.water_density,
                ))
            }
        }
    }
}
