use crate::metric::Metric;
use crate::model::Model;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

/// Calculate the flow required to produce power using the hydropower equation
fn inverse_hydropower_calculation(
    power: f64,
    water_elevation: f64,
    turbine_elevation: f64,
    efficiency: f64,
    flow_unit_conversion: f64,
    energy_unit_conversion: f64,
    density: f64,
) -> f64 {
    let mut head = water_elevation - turbine_elevation;
    if head < 0.0 {
        head = 0.0;
    }
    power / (energy_unit_conversion * density * 9.81 * head * efficiency * flow_unit_conversion)
}

pub struct HydropowerTargetParameter {
    pub meta: ParameterMeta,
    pub target: Metric,
    pub water_elevation: Option<Metric>,
    pub turbine_elevation: f64,
    pub min_head: f64,
    pub max_flow: Option<Metric>,
    pub min_flow: Option<Metric>,
    pub efficiency: f64,
    pub water_density: f64,
    pub flow_unit_conversion: f64,
    pub energy_unit_conversion: f64,
}

impl HydropowerTargetParameter {
    pub fn new(
        name: &str,
        target: Metric,
        water_elevation: Option<Metric>,
        turbine_elevation: Option<f64>,
        min_head: Option<f64>,
        max_flow: Option<Metric>,
        min_flow: Option<Metric>,
        efficiency: Option<f64>,
        water_density: Option<f64>,
        flow_unit_conversion: Option<f64>,
        energy_unit_conversion: Option<f64>,
    ) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            target,
            water_elevation,
            turbine_elevation: turbine_elevation.unwrap_or(0.0),
            min_head: min_head.unwrap_or(0.0),
            max_flow,
            min_flow,
            efficiency: efficiency.unwrap_or(1.0),
            water_density: water_density.unwrap_or(1000.0),
            flow_unit_conversion: flow_unit_conversion.unwrap_or(1.0),
            energy_unit_conversion: energy_unit_conversion.unwrap_or(1e-6),
        }
    }
}

impl Parameter for HydropowerTargetParameter {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Model,
        state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        // Calculate the head
        let mut head = if let Some(water_elevation) = &self.water_elevation {
            water_elevation.get_value(model, state)? - self.turbine_elevation
        } else {
            self.turbine_elevation
        };

        // the head may be negative
        head = head.max(0.0);

        // apply the minimum head threshold
        if head < self.min_head {
            return Ok(0.0);
        }

        // Get the flow from the current node
        let power = self.target.get_value(model, state)?;
        let mut q = inverse_hydropower_calculation(
            power,
            head,
            0.0,
            self.efficiency,
            self.flow_unit_conversion,
            self.energy_unit_conversion,
            self.water_density,
        );

        // Bound the flow if required
        if let Some(max_flow) = &self.max_flow {
            q = q.min(max_flow.get_value(model, state)?);
        } else if let Some(min_flow) = &self.min_flow {
            q = q.max(min_flow.get_value(model, state)?);
        }

        if q < 0.0 {
            return Err(PywrError::InternalParameterError(format!(
                "The calculated flow in the hydro power parameter named {} is negative",
                self.name()
            )));
        }
        Ok(q)
    }
}
