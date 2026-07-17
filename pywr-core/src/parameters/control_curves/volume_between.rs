use crate::metric::{SimpleMetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::SimpleCalculationError;
use crate::parameters::{
    BuiltParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta,
    ParameterName, ParameterState, SimpleParameter,
};
use crate::resolve_metric_f64;
use crate::scenario::ScenarioIndex;
use crate::state::SimpleParameterValues;
use crate::timestep::Timestep;
use std::fmt::Debug;

/// A parameter that returns the volume that is the proportion between two control curves
#[derive(Debug)]
pub struct VolumeBetweenControlCurvesParameter<M> {
    meta: ParameterMeta,
    total: M,
    upper: Option<M>,
    lower: Option<M>,
}

impl<M> Parameter for VolumeBetweenControlCurvesParameter<M>
where
    M: Send + Sync + Debug,
{
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl SimpleParameter<f64> for VolumeBetweenControlCurvesParameter<SimpleMetricF64> {
    fn before(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        values: &SimpleParameterValues,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, SimpleCalculationError> {
        let total = self.total.get_value(values)?;

        let lower = self.lower.as_ref().map_or(Ok(0.0), |metric| metric.get_value(values))?;
        let upper = self.upper.as_ref().map_or(Ok(1.0), |metric| metric.get_value(values))?;

        Ok(Some(total * (upper - lower)))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct VolumeBetweenControlCurvesParameterBuilder<M> {
    meta: ParameterMeta,
    total: M,
    upper: Option<M>,
    lower: Option<M>,
}

impl<M> VolumeBetweenControlCurvesParameterBuilder<M> {
    pub fn new(name: ParameterName, total: M) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            total,
            upper: None,
            lower: None,
        }
    }

    pub fn lower(&mut self, lower: M) -> &mut Self {
        self.lower = Some(lower);
        self
    }

    pub fn upper(&mut self, upper: M) -> &mut Self {
        self.upper = Some(upper);
        self
    }
}

impl ParameterBuilder<f64> for VolumeBetweenControlCurvesParameterBuilder<UnresolvedMetricF64> {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let total = resolve_metric_f64!(self, self.total, resolution_maps, "total");
        let total: SimpleMetricF64 =
            total
                .try_into()
                .map_err(|source| ParameterBuildError::CouldNotSimplifyMetricF64 {
                    attr: "total".to_string(),
                    source,
                })?;

        let upper: Option<SimpleMetricF64> = match &self.upper {
            Some(upper) => {
                let upper = resolve_metric_f64!(self, upper, resolution_maps, "upper");
                let upper: SimpleMetricF64 =
                    upper
                        .try_into()
                        .map_err(|source| ParameterBuildError::CouldNotSimplifyMetricF64 {
                            attr: "upper".to_string(),
                            source,
                        })?;

                Some(upper)
            }
            None => None,
        };

        let lower: Option<SimpleMetricF64> = match &self.lower {
            Some(lower) => {
                let lower = resolve_metric_f64!(self, lower, resolution_maps, "lower");
                let lower = lower
                    .try_into()
                    .map_err(|source| ParameterBuildError::CouldNotSimplifyMetricF64 {
                        attr: "lower".to_string(),
                        source,
                    })?;
                Some(lower)
            }
            None => None,
        };

        let p = VolumeBetweenControlCurvesParameter {
            meta: self.meta,
            total,
            upper,
            lower,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::Simple(Box::new(p))))
    }
}
