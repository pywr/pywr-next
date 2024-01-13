use crate::metric::Metric;
use crate::network::Network;
use crate::parameters::{Parameter, ParameterMeta};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
use std::any::Any;

pub struct DiscountFactorParameter {
    meta: ParameterMeta,
    discount_rate: Metric,
    base_year: i32,
}

impl DiscountFactorParameter {
    pub fn new(name: &str, discount_rate: Metric, base_year: i32) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            discount_rate,
            base_year,
        }
    }
}

impl Parameter for DiscountFactorParameter {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
    fn compute(
        &self,
        timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        let year = timestep.date.year() - self.base_year;
        let rate = self.discount_rate.get_value(network, state)?;

        let factor = 1.0 / (1.0 + rate).powi(year);
        Ok(factor)
    }
}

#[cfg(test)]
mod test {
    use crate::metric::Metric;
    use crate::parameters::{Array1Parameter, DiscountFactorParameter};
    use crate::test_utils::{run_and_assert_parameter, simple_model};
    use ndarray::{Array1, Array2, Axis};

    /// Basic functional test of the delay parameter.
    #[test]
    fn test_basic() {
        let mut model = simple_model(1);
        let network = model.network_mut();

        // Create an artificial volume series to use for the delay test
        let volumes = Array1::linspace(1.0, 0.0, 21);
        let volume = Array1Parameter::new("test-x", volumes.clone(), None);

        let _volume_idx = network.add_parameter(Box::new(volume)).unwrap();

        let parameter = DiscountFactorParameter::new(
            "test-parameter",
            Metric::Constant(0.03), // Interpolate with the parameter based values
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

        run_and_assert_parameter(&mut model, Box::new(parameter), expected_values, None, Some(1e-12));
    }
}
