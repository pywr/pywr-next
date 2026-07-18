use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    BuiltParameter, GeneralBeforeParameter, GeneralParameter, GeneralParameterContext, GeneralParameterEntry,
    MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder, ParameterMeta, ParameterName,
    ParameterState,
};

#[derive(Debug)]
pub struct VectorParameter {
    meta: ParameterMeta,
    values: Vec<f64>,
}

impl Parameter for VectorParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter for VectorParameter {
    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

impl GeneralBeforeParameter<f64> for VectorParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, GeneralCalculationError> {
        match self.values.get(ctx.timestep.index) {
            Some(v) => Ok(*v),
            None => Err(GeneralCalculationError::OutOfBoundsError {
                index: ctx.timestep.index,
                length: self.values.len(),
                axis: 0,
            }),
        }
    }
}

#[derive(Debug)]
pub struct VectorParameterBuilder {
    meta: ParameterMeta,
    values: Vec<f64>,
}

impl VectorParameterBuilder {
    pub fn new(name: ParameterName, values: &[f64]) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            values: values.to_vec(),
        }
    }
}

impl ParameterBuilder<f64> for VectorParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        _resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let p = VectorParameter {
            meta: self.meta,
            values: self.values,
        };

        let bp = BuiltParameter::General(GeneralParameterEntry::before(p));
        Ok(bp.into())
    }
}
