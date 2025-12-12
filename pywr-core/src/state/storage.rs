use crate::state::FlowState;
use crate::timestep::Timestep;
use num::Zero;

/// The state of a storage node in the network.
///
/// This includes the current volume, max volume and flow state. The max volume is retained here
/// separately from any parameter value so that it is explicitly updated when the storage volume
/// changes. This means that the proportional volume can be calculated correctly even if the max volume
/// is controlled by a parameter. I.e. the proportional volume is always volume / max_volume at the
/// end of the time-step, and not affected by parameter changes during "before" part of the time-step.
///
#[derive(Clone, Copy, Debug, Default)]
pub struct StorageState {
    // The current volume of the storage.
    volume: f64,
    // The current max volume.
    max_volume: f64,
    flows: FlowState,
}

impl StorageState {
    pub fn new(initial_volume: f64, max_volume: f64) -> Self {
        Self {
            volume: initial_volume,
            max_volume,
            flows: FlowState::new(),
        }
    }

    pub fn volume(&self) -> f64 {
        self.volume
    }

    pub fn max_volume(&self) -> f64 {
        self.max_volume
    }

    pub fn proportional_volume(&self) -> f64 {
        // If None, max volume is zero, so return 1.0 (matches v1.x behaviour)
        if self.max_volume.is_zero() {
            1.0
        } else {
            self.volume / self.max_volume
        }
    }

    pub fn flow_state(&self) -> &FlowState {
        &self.flows
    }

    pub fn reset(&mut self) {
        self.flows.reset();
        // Volume remains unchanged
    }

    /// Add an inflow and update the volume accordingly.
    pub fn add_in_flow(&mut self, flow: f64, timestep: &Timestep) {
        self.flows.add_in_flow(flow);
        self.volume += flow * timestep.days();
    }

    /// Add an outflow and update the volume accordingly.
    pub fn add_out_flow(&mut self, flow: f64, timestep: &Timestep) {
        self.flows.add_out_flow(flow);
        self.volume -= flow * timestep.days();
    }

    /// Set the volume directly and update the proportional volume accordingly.
    pub fn set_volume(&mut self, volume: f64, max_volume: f64) {
        self.volume = volume;
        self.max_volume = max_volume;
    }

    /// Finalise the storage state at the end of the time-step.
    ///
    /// This will clamp the volume to the min and max volume range and update the proportional volume.
    pub fn finalise(&mut self, min_volume: f64, max_volume: f64) {
        self.clamp(min_volume, max_volume);
        self.max_volume = max_volume;
    }

    /// Ensure the volume is within the min and max volume range (inclusive). If the volume
    /// is more than 1E6 outside the min or max volume then this function will panic,
    /// reporting a mass-balance message.
    fn clamp(&mut self, min_volume: f64, max_volume: f64) {
        if (self.volume - min_volume) < -1e-6 {
            panic!(
                "Mass-balance error detected. Volume ({}) is smaller than minimum volume ({}).",
                self.volume, min_volume
            );
        }
        if (self.volume - max_volume) > 1e-6 {
            panic!(
                "Mass-balance error detected. Volume ({}) is greater than maximum volume ({}).",
                self.volume, max_volume,
            );
        }
        self.volume = self.volume.clamp(min_volume, max_volume);
    }
}
