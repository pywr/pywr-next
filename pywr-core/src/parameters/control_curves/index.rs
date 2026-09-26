use crate::metric::{MetricConsumerPhase, MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    BuiltParameter, GeneralAfterParameter, GeneralBeforeParameter, GeneralParameter, GeneralParameterContext,
    GeneralParameterEntry, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta,
    ParameterName, ParameterState,
};
use crate::{resolve_metric_f64, resolve_metric_f64_vec};

#[derive(Debug)]
pub struct ControlCurveIndexParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    control_curves: Vec<MetricF64>,
}

impl Parameter for ControlCurveIndexParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter for ControlCurveIndexParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<u64> for ControlCurveIndexParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(ctx.network, ctx.state)?;
        let values = self
            .control_curves
            .iter()
            .map(|cc| cc.get_value(ctx.network, ctx.state));

        Ok(control_curve_index(x, values)?)
    }
}

impl GeneralAfterParameter<u64> for ControlCurveIndexParameter {
    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<u64, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(ctx.network, ctx.state)?;
        let values = self
            .control_curves
            .iter()
            .map(|cc| cc.get_value(ctx.network, ctx.state));

        Ok(control_curve_index(x, values)?)
    }
}

pub fn control_curve_index<E>(x: f64, control_curves: impl IntoIterator<Item = Result<f64, E>>) -> Result<u64, E> {
    let mut index = 0;
    for cc_value in control_curves {
        let cc_value = cc_value?;
        if x >= cc_value {
            return Ok(index);
        }
        index += 1;
    }
    Ok(index)
}

#[derive(Debug)]
pub struct ControlCurveIndexParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    control_curves: Vec<UnresolvedMetricF64>,
    phase: MetricConsumerPhase,
}

impl ControlCurveIndexParameterBuilder {
    /// Create a new builder for [`ControlCurveIndexParameter`] that is evaluated in the "before" phase.
    pub fn before(name: ParameterName, metric: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            phase: MetricConsumerPhase::Before,
        }
    }

    /// Create a new builder for [`ControlCurveIndexParameter`] that is evaluated in the "after" phase.
    pub fn after(name: ParameterName, metric: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            phase: MetricConsumerPhase::After,
        }
    }

    /// Create a new builder for [`ControlCurveIndexParameter`] that is evaluated in both "before" and "after" phases.
    pub fn both(name: ParameterName, metric: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            control_curves: Vec::new(),
            phase: MetricConsumerPhase::Both,
        }
    }

    pub fn control_curve(&mut self, cc: UnresolvedMetricF64) -> &mut Self {
        self.control_curves.push(cc);
        self
    }
}

impl ParameterBuilder<u64> for ControlCurveIndexParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<u64>, ParameterBuildError> {
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, self.phase, "metric");
        let control_curves = resolve_metric_f64_vec!(
            self,
            &self.control_curves,
            resolution_maps,
            self.phase,
            "control_curves"
        );

        let p = ControlCurveIndexParameter {
            meta: self.meta,
            metric,
            control_curves,
        };

        let built = match self.phase {
            MetricConsumerPhase::Before => BuiltParameter::General(GeneralParameterEntry::before(p)),
            MetricConsumerPhase::After => BuiltParameter::General(GeneralParameterEntry::after(p)),
            MetricConsumerPhase::Both => BuiltParameter::General(GeneralParameterEntry::both(p)),
        };

        Ok(built.into())
    }
}

#[cfg(test)]
mod tests {
    use super::control_curve_index;

    #[test]
    fn computes_index_correctly() {
        let control_curves = [0.8, 0.5, 0.2];
        let index = |x| control_curve_index(x, control_curves.iter().copied().map(Ok::<_, ()>)).unwrap();

        assert_eq!(index(0.1), 3);
        assert_eq!(index(0.2), 2);
        assert_eq!(index(0.3), 2);
        assert_eq!(index(0.5), 1);
        assert_eq!(index(0.6), 1);
        assert_eq!(index(0.8), 0);
        assert_eq!(index(0.9), 0);
    }
}
