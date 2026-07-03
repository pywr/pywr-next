use crate::metric::{MetricF64, UnresolvedMetricF64};
use crate::network::{Network, ResolutionMaps};
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    BuiltParameter, GeneralParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder,
    ParameterMeta, ParameterName, ParameterState,
};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::{resolve_metric_f64, resolve_metric_f64_vec};

#[derive(Debug)]
pub struct ControlCurveParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    control_curves: Vec<MetricF64>,
    values: Vec<MetricF64>,
}

impl Parameter for ControlCurveParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<f64> for ControlCurveParameter {
    fn before(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(model, state)?;

        for (idx, control_curve) in self.control_curves.iter().enumerate() {
            let cc_value = control_curve.get_value(model, state)?;
            if x >= cc_value {
                let value = self
                    .values
                    .get(idx)
                    .ok_or_else(|| GeneralCalculationError::OutOfBoundsError {
                        axis: 0,
                        index: idx,
                        length: self.values.len(),
                    })?;
                return Ok(Some(value.get_value(model, state)?));
            }
        }

        let value = self
            .values
            .last()
            .ok_or_else(|| GeneralCalculationError::OutOfBoundsError {
                axis: 0,
                index: 0,
                length: self.values.len(),
            })?;

        Ok(Some(value.get_value(model, state)?))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct ControlCurveParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    control_curves: Vec<UnresolvedMetricF64>,
    values: Vec<UnresolvedMetricF64>,
}

impl ControlCurveParameterBuilder {
    pub fn new(name: ParameterName, metric: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            values: Vec::new(),
        }
    }

    pub fn control_curve(&mut self, cc: UnresolvedMetricF64) -> &mut Self {
        self.control_curves.push(cc);
        self
    }

    pub fn value(&mut self, value: UnresolvedMetricF64) -> &mut Self {
        self.values.push(value);
        self
    }
}

impl ParameterBuilder<f64> for ControlCurveParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, "metric");

        let control_curves = resolve_metric_f64_vec!(self, &self.control_curves, resolution_maps, "control_curves");

        let values = resolve_metric_f64_vec!(self, &self.values, resolution_maps, "values");

        let p = ControlCurveParameter {
            meta: self.meta,
            metric,
            control_curves,
            values,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}
