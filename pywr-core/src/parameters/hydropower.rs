use crate::metric::{MetricF64, UnresolvedMetricF64};
use crate::network::{Network, ResolutionMaps};
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    BuiltParameter, GeneralParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder,
    ParameterMeta, ParameterName, ParameterState,
};
use crate::resolve_optional_metric_f64;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::utils::{hydropower_calculation, inverse_hydropower_calculation};

pub struct HydropowerTargetData {
    pub actual_flow: Option<UnresolvedMetricF64>,
    pub target: Option<UnresolvedMetricF64>,
    pub elevation: Option<f64>,
    pub min_head: Option<f64>,
    pub max_flow: Option<UnresolvedMetricF64>,
    pub min_flow: Option<UnresolvedMetricF64>,
    pub efficiency: Option<f64>,
    pub water_elevation: Option<UnresolvedMetricF64>,
    pub water_density: Option<f64>,
    pub flow_unit_conversion: Option<f64>,
    pub energy_unit_conversion: Option<f64>,
}

pub struct HydropowerTargetParameter {
    meta: ParameterMeta,
    actual_flow: Option<MetricF64>,
    target: Option<MetricF64>,
    max_flow: Option<MetricF64>,
    min_flow: Option<MetricF64>,
    turbine_min_head: f64,
    turbine_elevation: f64,
    turbine_efficiency: f64,
    water_elevation: Option<MetricF64>,
    water_density: f64,
    flow_unit_conversion: f64,
    energy_unit_conversion: f64,
}

impl HydropowerTargetParameter {
    fn head(&self, model: &Network, state: &State) -> Result<f64, GeneralCalculationError> {
        let head = if let Some(water_elevation) = &self.water_elevation {
            water_elevation.get_value(model, state)? - self.turbine_elevation
        } else {
            self.turbine_elevation
        };
        Ok(head.max(0.0))
    }
}

impl Parameter for HydropowerTargetParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<f64> for HydropowerTargetParameter {
    fn before(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        let head = self.head(model, state)?;

        // apply the minimum head threshold
        if head <= self.turbine_min_head {
            return Ok(Some(0.0));
        }

        // Get the flow from the current node
        if let Some(target) = &self.target {
            let power = target.get_value(model, state)?;
            let mut q = inverse_hydropower_calculation(
                power,
                head,
                self.turbine_efficiency,
                self.flow_unit_conversion,
                self.energy_unit_conversion,
                self.water_density,
            );

            // Bound the flow if required
            if let Some(max_flow) = &self.max_flow {
                q = q.min(max_flow.get_value(model, state)?);
            }
            if let Some(min_flow) = &self.min_flow {
                q = q.max(min_flow.get_value(model, state)?);
            }

            if q < 0.0 {
                return Err(GeneralCalculationError::Internal {
                    message: "The calculated flow is negative".into(),
                });
            }
            Ok(Some(q))
        } else {
            // No target flow therefore can not calculate a flow
            Ok(None)
        }
    }

    fn after(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        if let Some(actual_flow) = &self.actual_flow {
            let flow = actual_flow.get_value(model, state)?;
            // Calculate the head (the head may be negative)
            let head = self.head(model, state)?;

            // apply the minimum head threshold
            if head <= self.turbine_min_head {
                return Ok(Some(0.0));
            }

            Ok(Some(hydropower_calculation(
                flow,
                head,
                self.turbine_efficiency,
                self.flow_unit_conversion,
                self.energy_unit_conversion,
                self.water_density,
            )))
        } else {
            // No actual flow therefore can not calculate power
            Ok(None)
        }
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

pub struct HydropowerTargetParameterBuilder {
    meta: ParameterMeta,
    actual_flow: Option<UnresolvedMetricF64>,
    target: Option<UnresolvedMetricF64>,
    max_flow: Option<UnresolvedMetricF64>,
    min_flow: Option<UnresolvedMetricF64>,
    turbine_min_head: f64,
    turbine_elevation: f64,
    turbine_efficiency: f64,
    water_elevation: Option<UnresolvedMetricF64>,
    water_density: f64,
    flow_unit_conversion: f64,
    energy_unit_conversion: f64,
}

impl HydropowerTargetParameterBuilder {
    pub fn new(name: ParameterName, turbine_data: HydropowerTargetData) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            actual_flow: turbine_data.actual_flow,
            target: turbine_data.target,
            water_elevation: turbine_data.water_elevation,
            turbine_elevation: turbine_data.elevation.unwrap_or(0.0),
            turbine_min_head: turbine_data.min_head.unwrap_or(0.0),
            turbine_efficiency: turbine_data.efficiency.unwrap_or(1.0),
            max_flow: turbine_data.max_flow,
            min_flow: turbine_data.min_flow,
            water_density: turbine_data.water_density.unwrap_or(1000.0),
            flow_unit_conversion: turbine_data.flow_unit_conversion.unwrap_or(1.0),
            energy_unit_conversion: turbine_data.energy_unit_conversion.unwrap_or(1e-6),
        }
    }
}

impl ParameterBuilder<f64> for HydropowerTargetParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let actual_flow = resolve_optional_metric_f64!(self, &self.actual_flow, resolution_maps, "actual_flow");
        let target = resolve_optional_metric_f64!(self, &self.target, resolution_maps, "target");
        let max_flow = resolve_optional_metric_f64!(self, &self.max_flow, resolution_maps, "max_flow");
        let min_flow = resolve_optional_metric_f64!(self, &self.min_flow, resolution_maps, "min_flow");
        let water_elevation =
            resolve_optional_metric_f64!(self, &self.water_elevation, resolution_maps, "water_elevation");

        let p = HydropowerTargetParameter {
            meta: self.meta,
            actual_flow,
            target,
            max_flow,
            min_flow,
            turbine_min_head: self.turbine_min_head,
            turbine_elevation: self.turbine_elevation,
            turbine_efficiency: self.turbine_efficiency,
            water_elevation,
            water_density: self.water_density,
            flow_unit_conversion: self.flow_unit_conversion,
            energy_unit_conversion: self.energy_unit_conversion,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}
