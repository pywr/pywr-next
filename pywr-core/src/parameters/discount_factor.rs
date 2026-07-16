use crate::metric::{MetricF64, UnresolvedMetricF64};
use crate::network::ResolutionMaps;
use crate::parameters::errors::GeneralCalculationError;
use crate::parameters::{
    BuiltParameter, GeneralParameter, GeneralParameterContext, MaybeBuiltParameter, Parameter, ParameterBuildError,
    ParameterBuilder, ParameterMeta, ParameterName, ParameterState,
};
use crate::resolve_metric_f64;
use chrono::Datelike;

#[derive(Debug)]
pub struct DiscountFactorParameter {
    meta: ParameterMeta,
    discount_rate: MetricF64,
    base_year: i32,
}

impl Parameter for DiscountFactorParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}
impl GeneralParameter<f64> for DiscountFactorParameter {
    fn before(
        &self,
        ctx: GeneralParameterContext<'_>,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        let year = ctx.timestep.date.year() - self.base_year;
        let rate = self.discount_rate.get_value(ctx.network, ctx.state)?;

        let factor = 1.0 / (1.0 + rate).powi(year);
        Ok(Some(factor))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[derive(Debug)]
pub struct DiscountFactorParameterBuilder {
    meta: ParameterMeta,
    discount_rate: UnresolvedMetricF64,
    base_year: i32,
}

impl DiscountFactorParameterBuilder {
    pub fn new(name: ParameterName, discount_rate: UnresolvedMetricF64, base_year: i32) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            discount_rate,
            base_year,
        }
    }
}

impl ParameterBuilder<f64> for DiscountFactorParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }
    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let discount_rate = resolve_metric_f64!(self, self.discount_rate, resolution_maps, "discount_rate");

        let p = DiscountFactorParameter {
            meta: self.meta,
            discount_rate,
            base_year: self.base_year,
        };

        let bp = BuiltParameter::General(Box::new(p));
        Ok(MaybeBuiltParameter::Built(bp))
    }
}

#[cfg(test)]
mod test {
    use crate::parameters::Array1ParameterBuilder;
    use crate::parameters::discount_factor::DiscountFactorParameterBuilder;
    use crate::test_utils::{run_and_assert_parameter, simple_model};
    use ndarray::{Array1, Array2, Axis};

    /// Basic functional test of the delay parameter.
    #[test]
    fn test_basic() {
        let mut model_builder = simple_model(1, None);

        // Create an artificial volume series to use for the delay test
        let volumes = Array1::linspace(1.0, 0.0, 21);
        let volume = Array1ParameterBuilder::new("test-x".into(), volumes.clone());

        model_builder.network_builder().parameters().f64(Box::new(volume));

        let parameter = DiscountFactorParameterBuilder::new(
            "test-parameter".into(),
            0.03.into(), // Interpolate with the parameter based values
            2020,
        );

        // We should have DELAY number of initial values to start with, and then follow the
        // values in the `volumes` array.
        let expected_values: Array1<f64> = [
            1.0; 21 // initial values
        ]
            .to_vec()
            .into();

        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        run_and_assert_parameter(model_builder, Box::new(parameter), expected_values, None, Some(1e-12));
    }
}
