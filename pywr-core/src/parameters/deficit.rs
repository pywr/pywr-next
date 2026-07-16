use crate::metric::{MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::{
    BuiltParameter, GeneralCalculationError, GeneralParameter, GeneralParameterContext, MaybeBuiltParameter, Parameter,
    ParameterBuildError, ParameterBuilder, ParameterMeta, ParameterName, ParameterState,
};
use crate::resolve_metric_f64;

/// A parameter representing the deficit between a flow metric and a max metric.
///
/// Typically used to represent the deficit between actual inflow and requested max flow at
/// a node.
#[derive(Debug)]
pub struct DeficitParameter {
    meta: ParameterMeta,
    flow: MetricF64,
    max_flow: MetricF64,
}

impl Parameter for DeficitParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<f64> for DeficitParameter {
    fn after(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        let actual_flow = self.flow.get_value(ctx.network, ctx.state)?;
        let max_flow = self.max_flow.get_value(ctx.network, ctx.state)?;

        let deficit = (max_flow - actual_flow).max(0.0);
        Ok(Some(deficit))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct DeficitParameterBuilder {
    meta: ParameterMeta,
    flow: UnresolvedMetricF64,
    max_flow: UnresolvedMetricF64,
}

impl DeficitParameterBuilder {
    pub fn new(name: ParameterName, flow: UnresolvedMetricF64, max_flow: UnresolvedMetricF64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            flow,
            max_flow,
        }
    }
}

impl ParameterBuilder<f64> for DeficitParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let flow = resolve_metric_f64!(self, self.flow, resolution_maps, "flow");
        let max_flow = resolve_metric_f64!(self, self.max_flow, resolution_maps, "max_flow");

        let p = DeficitParameter {
            meta: self.meta,
            flow,
            max_flow,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}
