use pywr_core::metric::MetricF64;
use pywr_core::network::Network;
use pywr_core::parameters::{Parameter, ParameterMeta};
use pywr_core::scenario::ScenarioIndex;
use pywr_core::state::{ParameterState, State};
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
    pub fn new(name: &str, metric: MetricF64, threshold: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
        }
    }
}
// ANCHOR_END: impl-new
// ANCHOR: impl-parameter
impl Parameter<f64> for MaxParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
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
}
// ANCHOR_END: impl-parameter
mod schema {
    use pywr_schema::metric::Metric;
    use pywr_schema::parameters::ParameterMeta;
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
            let idx = self.parameter.load(network, args)?;
            let threshold = self.threshold.unwrap_or(0.0);

            let p = pywr_core::parameters::MaxParameter::new(&self.meta.name, idx, threshold);
            Ok(network.add_parameter(Box::new(p))?)
        }
    }
    // ANCHOR_END: schema-impl
}

fn main() {
    println!("Hello, world!");
}
