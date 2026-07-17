use crate::metric::{MetricF64, UnresolvedMetricF64};
use crate::network::{Network, ResolutionMaps};
use crate::parameters::{
    BuiltParameter, GeneralCalculationError, GeneralParameter, MaybeBuiltParameter, Parameter, ParameterBuildError,
    ParameterBuilder, ParameterMeta, ParameterName, ParameterState,
};
use crate::resolve_metric_f64;
use crate::scenario::ScenarioIndex;
use crate::state::{MultiValue, State};
use crate::timestep::Timestep;
use std::collections::HashMap;

#[derive(Debug)]
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
#[derive(Debug)]
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

impl Parameter for MuskingumParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<MultiValue> for MuskingumParameter {
    fn before(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<MultiValue>, GeneralCalculationError> {
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
                    return Ok(Some(MultiValue::new(values, HashMap::new())));
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
        Ok(Some(MultiValue::new(values, HashMap::new())))
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

#[derive(Debug)]
pub struct MuskingumParameterBuilder {
    meta: ParameterMeta,
    inflow: UnresolvedMetricF64,
    outflow: UnresolvedMetricF64,
    /// Travel time of the flood wave through routing reach / node (K)
    travel_time: UnresolvedMetricF64,
    /// Dimensionless weight (0 ≤ X ≤ 0.5).
    weight: UnresolvedMetricF64,
    /// Initial condition for the Muskingum routing.
    initial_condition: MuskingumInitialCondition,
}

impl MuskingumParameterBuilder {
    pub fn new(
        name: ParameterName,
        inflow: UnresolvedMetricF64,
        outflow: UnresolvedMetricF64,
        travel_time: UnresolvedMetricF64,
        weight: UnresolvedMetricF64,
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

impl ParameterBuilder<MultiValue> for MuskingumParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<MultiValue>, ParameterBuildError> {
        let inflow = resolve_metric_f64!(self, self.inflow, resolution_maps, "inflow");
        let outflow = resolve_metric_f64!(self, self.outflow, resolution_maps, "outflow");
        let travel_time = resolve_metric_f64!(self, self.travel_time, resolution_maps, "travel_time");
        let weight = resolve_metric_f64!(self, self.weight, resolution_maps, "weight");

        let p = MuskingumParameter {
            meta: self.meta,
            inflow,
            outflow,
            travel_time,
            weight,
            initial_condition: self.initial_condition,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}
