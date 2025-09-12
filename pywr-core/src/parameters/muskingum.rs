use crate::metric::MetricF64;
use crate::network::Network;
use crate::parameters::{
    GeneralParameter, Parameter, ParameterCalculationError, ParameterMeta, ParameterName, ParameterState,
};
use crate::scenario::ScenarioIndex;
use crate::state::{MultiValue, State};
use crate::timestep::Timestep;
use std::collections::HashMap;

pub enum MuskingumInitialCondition {
    SteadyState,
    Specified { inflow: f64, outflow: f64 },
}

/// A parameter that computes the Muskingum routing coefficients.
///
/// This parameter computes two coefficients used in the Muskingum routing method:
/// - `inflow_factor`: The coefficient applied to the current inflow.
/// - `rhs`: The right-hand side of the Muskingum equation, which combines the previous
///   inflow and outflow.
///
/// These coefficients are intended for use in an [`AggregatedNode`] that relates the inflow
/// and outflow using an equality constraint. This ensures the outflow is computed based on
/// the Muskingum routing method.
///
/// # Initial conditions
///
/// The initial condition for the Muskingum routing can be specified in two ways:
/// - `SteadyState`: Assumes that the inflow and outflow are equal at the first time-step.
/// - `Specified`: Allows the user to specify the initial inflow and outflow values.
///
pub struct MuskingumParameter {
    meta: ParameterMeta,
    inflow: MetricF64,
    outflow: MetricF64,
    /// Travel time of the flood wave through routing reach / node (K)
    travel_time: MetricF64,
    /// Dimensionless weight (0 ≤ X ≤ 0.5).
    weight: MetricF64,
    /// Initial condition for the Muskingum routing.
    initial_condition: MuskingumInitialCondition,
}

impl MuskingumParameter {
    pub fn new(
        name: ParameterName,
        inflow: MetricF64,
        outflow: MetricF64,
        travel_time: MetricF64,
        weight: MetricF64,
        initial_condition: MuskingumInitialCondition,
    ) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            inflow,
            outflow,
            travel_time,
            weight,
            initial_condition,
        }
    }
}

impl Parameter for MuskingumParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<MultiValue> for MuskingumParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<MultiValue, ParameterCalculationError> {
        let weight = self.weight.get_value(model, state)?;
        let travel_time = self.travel_time.get_value(model, state)?;

        // The inflow and outflow metrics from the previous time-step
        let (inflow, outflow) = if timestep.is_first() {
            match &self.initial_condition {
                MuskingumInitialCondition::SteadyState => {
                    // For steady-state the inflow and outflow are equal
                    // This means we can simplify the equations for the first time-step
                    let inflow_factor = steady_state_inflow_factor(weight, travel_time);
                    let outflow_factor = steady_state_outflow_factor(weight, travel_time);
                    let mut values = HashMap::new();
                    values.insert("inflow_factor".to_string(), -inflow_factor / outflow_factor);
                    values.insert("rhs".to_string(), 0.0);
                    return Ok(MultiValue::new(values, HashMap::new()));
                }
                MuskingumInitialCondition::Specified { inflow, outflow } => (*inflow, *outflow),
            }
        } else {
            (
                self.inflow.get_value(model, state)?,
                self.outflow.get_value(model, state)?,
            )
        };

        let mut values = HashMap::new();
        values.insert("inflow_factor".to_string(), -inflow_factor(weight, travel_time));
        values.insert("rhs".to_string(), rhs(inflow, outflow, weight, travel_time));
        Ok(MultiValue::new(values, HashMap::new()))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

/// Compute the inflow factor for the Muskingum routing equation.
fn inflow_factor(weight: f64, travel_time: f64) -> f64 {
    (1.0 - 2.0 * weight * travel_time) / (2.0 * travel_time * (1.0 - weight) + 1.0)
}

/// Compute the right-hand side of the Muskingum routing equation.
fn rhs(inflow: f64, outflow: f64, weight: f64, travel_time: f64) -> f64 {
    let i = inflow * (1.0 + 2.0 * weight * travel_time) / (2.0 * travel_time * (1.0 - weight) + 1.0);

    let o = outflow * (2.0 * travel_time * (1.0 - weight) - 1.0) / (2.0 * travel_time * (1.0 - weight) + 1.0);

    i + o
}

/// Compute the inflow factor for the steady-state initial condition.
fn steady_state_inflow_factor(weight: f64, travel_time: f64) -> f64 {
    2.0 / (2.0 * travel_time * (1.0 - weight) + 1.0)
}

/// Compute the outflow factor for the steady-state initial condition.
fn steady_state_outflow_factor(weight: f64, travel_time: f64) -> f64 {
    1.0 - (2.0 * travel_time * (1.0 - weight) - 1.0) / (2.0 * travel_time * (1.0 - weight) + 1.0)
}
