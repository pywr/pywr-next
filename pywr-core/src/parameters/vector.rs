use crate::network::{Network, ResolutionMaps};
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    BuiltParameter, GeneralParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder,
    ParameterMeta, ParameterName, ParameterState,
};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

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

impl GeneralParameter<f64> for VectorParameter {
    fn before(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        _model: &Network,
        _state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        match self.values.get(timestep.index) {
            Some(v) => Ok(Some(*v)),
            None => Err(GeneralCalculationError::OutOfBoundsError {
                index: timestep.index,
                length: self.values.len(),
                axis: 0,
            }),
        }
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
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

        let bp = BuiltParameter::General(Box::new(p));
        Ok(bp.into())
    }
}
