#![allow(dead_code)]
use pywr_core::metric::MetricF64;
use pywr_core::network::Network;
use pywr_core::parameters::{GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState};
use pywr_core::scenario::ScenarioIndex;
use pywr_core::state::State;
use pywr_core::timestep::Timestep;
use pywr_core::PywrError;

// ANCHOR: parameter
pub struct MaxParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    threshold: f64,
}
// ANCHOR_END: parameter
// ANCHOR: impl-new
impl MaxParameter {
    pub fn new(name: ParameterName, metric: MetricF64, threshold: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
        }
    }
}
// ANCHOR_END: impl-new
// ANCHOR: impl-parameter
impl Parameter for MaxParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<f64> for MaxParameter {
    fn compute(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        // Current value
        let x = self.metric.get_value(model, state)?;
        Ok(x.max(self.threshold))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

// ANCHOR_END: impl-parameter
mod schema {
    #[cfg(feature = "core")]
    use pywr_core::parameters::ParameterIndex;
    use pywr_schema::metric::Metric;
    use pywr_schema::parameters::ParameterMeta;
    #[cfg(feature = "core")]
    use pywr_schema::{model::LoadArgs, SchemaError};
    use schemars::JsonSchema;

    // ANCHOR: schema
    #[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
    pub struct MaxParameter {
        #[serde(flatten)]
        pub meta: ParameterMeta,
        pub parameter: Metric,
        pub threshold: Option<f64>,
    }

    // ANCHOR_END: schema
    // ANCHOR: schema-impl
    #[cfg(feature = "core")]
    impl MaxParameter {
        pub fn add_to_model(
            &self,
            network: &mut pywr_core::network::Network,
            args: &LoadArgs,
        ) -> Result<ParameterIndex<f64>, SchemaError> {
            let idx = self.parameter.load(network, args, Some(&self.meta.name))?;
            let threshold = self.threshold.unwrap_or(0.0);

            let p = pywr_core::parameters::MaxParameter::new(self.meta.name.as_str().into(), idx, threshold);
            Ok(network.add_parameter(Box::new(p))?)
        }
    }
    // ANCHOR_END: schema-impl
}

fn main() {
    println!("Hello, world!");
}
