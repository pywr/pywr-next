use crate::metric::{MetricConsumerPhase, MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::control_curves::index::control_curve_index;
use crate::parameters::{
    BuiltParameter, GeneralBeforeParameter, GeneralAfterParameter, GeneralParameter, GeneralParameterContext, GeneralParameterEntry,
    MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta, ParameterName,
    ParameterState,
};
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

impl GeneralParameter for ControlCurveParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<f64> for ControlCurveParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(ctx.network, ctx.state)?;
        let control_curves = self
            .control_curves
            .iter()
            .map(|cc| cc.get_value(ctx.network, ctx.state))
            .collect::<Result<Vec<_>, _>>()?;
        let values = self
            .values
            .iter()
            .map(|v| v.get_value(ctx.network, ctx.state))
            .collect::<Result<Vec<_>, _>>()?;
        
        calculate_control_curve(x, &control_curves, &values)
    }
}


impl GeneralAfterParameter<f64> for ControlCurveParameter {
    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(ctx.network, ctx.state)?;
        let control_curves = self
            .control_curves
            .iter()
            .map(|cc| cc.get_value(ctx.network, ctx.state))
            .collect::<Result<Vec<_>, _>>()?;
        let values = self
            .values
            .iter()
            .map(|v| v.get_value(ctx.network, ctx.state))
            .collect::<Result<Vec<_>, _>>()?;

        calculate_control_curve(x, &control_curves, &values)
    }
}

fn calculate_control_curve(x: f64, control_curves: &[f64], values: &[f64]) -> Result<f64, GeneralCalculationError> {
    let idx = control_curve_index(x, control_curves) as usize;
    let value = values
        .get(idx)
        .ok_or_else(|| GeneralCalculationError::OutOfBoundsError {
            axis: 0,
            index: idx,
            length: values.len(),
        })?;
    Ok(*value)
}


#[derive(Debug)]
pub struct ControlCurveParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    control_curves: Vec<UnresolvedMetricF64>,
    values: Vec<UnresolvedMetricF64>,
    phase: MetricConsumerPhase,
}

impl ControlCurveParameterBuilder {
    /// Create a new builder for [`ControlCurveParameter`] that is evaluated in the "before" phase.
    pub fn before(name: ParameterName, metric: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            values: Vec::new(),
            phase: MetricConsumerPhase::Before,
        }
    }

    /// Create a new builder for [`ControlCurveParameter`] that is evaluated in the "after" phase.
    pub fn after(name: ParameterName, metric: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            values: Vec::new(),
            phase: MetricConsumerPhase::After,
        }
    }

    /// Create a new builder for [`ControlCurveParameter`] that is evaluated in both "before" and "after" phases.
    pub fn both(name: ParameterName, metric: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            values: Vec::new(),
            phase: MetricConsumerPhase::Both,
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
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, self.phase, "metric");

        let control_curves =
            resolve_metric_f64_vec!(self, &self.control_curves, resolution_maps, self.phase, "control_curves");

        let values = resolve_metric_f64_vec!(self, &self.values, resolution_maps, self.phase, "values");

        let p = ControlCurveParameter {
            meta: self.meta,
            metric,
            control_curves,
            values,
        };

        let built = match self.phase {
            MetricConsumerPhase::Before => {
                BuiltParameter::General(GeneralParameterEntry::before(p))
            },
            MetricConsumerPhase::After => {
                BuiltParameter::General(GeneralParameterEntry::after(p))
            },
            MetricConsumerPhase::Both => {
                BuiltParameter::General(GeneralParameterEntry::both(p))
            }
        };

        Ok(built.into())
    }
}


#[cfg(test)]
mod tests {
    use super:: calculate_control_curve;

    #[test]
    fn test_calculate_control_curve() {
        let control_curves = vec![0.8, 0.5, 0.2];
        let values = vec![10.0, 20.0, 30.0, 50.0];

        assert_eq!(calculate_control_curve(0.9, &control_curves, &values).unwrap(), 10.0);
        assert_eq!(calculate_control_curve(0.8, &control_curves, &values).unwrap(), 10.0);
        assert_eq!(calculate_control_curve(0.6, &control_curves, &values).unwrap(), 20.0);
        assert_eq!(calculate_control_curve(0.5, &control_curves, &values).unwrap(), 20.0);
        assert_eq!(calculate_control_curve(0.3, &control_curves, &values).unwrap(), 30.0);
        assert_eq!(calculate_control_curve(0.2, &control_curves, &values).unwrap(), 30.0);
        assert_eq!(calculate_control_curve(0.1, &control_curves, &values).unwrap(), 50.0);
    }
}