use crate::metric::{MetricU64, UnresolvedMetricU64};
use crate::network::{Network, ResolutionMaps};
use crate::parameters::errors::{GeneralCalculationError, ParameterSetupError};
use crate::parameters::{
    BuiltParameter, GeneralParameter, MaybeBuiltParameter, Parameter, ParameterBuildError, ParameterBuilder,
    ParameterMeta, ParameterName, ParameterState, downcast_internal_state_mut,
};
use crate::resolve_metric_u64;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;

pub struct AsymmetricSwitchIndexParameter {
    meta: ParameterMeta,
    on_parameter: MetricU64,
    off_parameter: MetricU64,
}

impl Parameter for AsymmetricSwitchIndexParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, ParameterSetupError> {
        Ok(Some(Box::new(0_u64)))
    }
}

impl GeneralParameter<u64> for AsymmetricSwitchIndexParameter {
    fn before(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<u64>, GeneralCalculationError> {
        let on_value = self.on_parameter.get_value(network, state)?;

        // Downcast the internal state to the correct type
        let current_state = downcast_internal_state_mut::<u64>(internal_state);

        if *current_state > 0 {
            if on_value > 0 {
                // No change
            } else {
                let off_value = self.off_parameter.get_value(network, state)?;

                if off_value == 0 {
                    *current_state = 0;
                }
            }
        } else if on_value > 0 {
            *current_state = 1;
        }

        Ok(Some(*current_state))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

pub struct AsymmetricSwitchIndexParameterBuilder {
    meta: ParameterMeta,
    on_parameter: UnresolvedMetricU64,
    off_parameter: UnresolvedMetricU64,
}

impl AsymmetricSwitchIndexParameterBuilder {
    pub fn new(name: ParameterName, on_parameter: UnresolvedMetricU64, off_parameter: UnresolvedMetricU64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            on_parameter,
            off_parameter,
        }
    }
}

impl ParameterBuilder<u64> for AsymmetricSwitchIndexParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<u64>, ParameterBuildError> {
        let on_parameter = resolve_metric_u64!(self, self.on_parameter, resolution_maps, "on_parameter");
        let off_parameter = resolve_metric_u64!(self, self.off_parameter, resolution_maps, "off_parameter");

        let p = AsymmetricSwitchIndexParameter {
            meta: self.meta,
            on_parameter,
            off_parameter,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}
