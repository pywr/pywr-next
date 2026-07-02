#![allow(dead_code)]
use pywr_core::metric::{MetricF64, UnresolvedMetricF64};
use pywr_core::network::{Network, ResolutionMaps};
use pywr_core::parameters::{
    BuiltParameter, GeneralCalculationError, GeneralParameter, MaybeBuiltParameter, Parameter, ParameterBuildError,
    ParameterBuilder, ParameterMeta, ParameterName, ParameterState,
};
use pywr_core::resolve_metric_f64;
use pywr_core::scenario::ScenarioIndex;
use pywr_core::state::State;
use pywr_core::timestep::Timestep;

// ANCHOR: parameter
pub struct MaxParameter {
    meta: ParameterMeta,
    metric: MetricF64,
    threshold: f64,
}
// ANCHOR_END: parameter

// ANCHOR: impl-parameter
impl Parameter for MaxParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }
}

impl GeneralParameter<f64> for MaxParameter {
    fn before(
        &self,
        _timestep: &Timestep,
        _scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        _internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<Option<f64>, GeneralCalculationError> {
        // Current value
        let x = self.metric.get_value(model, state)?;
        Ok(Some(x.max(self.threshold)))
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

// ANCHOR: impl-builder
pub struct MaxParameterBuilder {
    meta: ParameterMeta,
    metric: UnresolvedMetricF64,
    threshold: f64,
}

impl MaxParameterBuilder {
    pub fn new(name: ParameterName, metric: UnresolvedMetricF64, threshold: f64) -> Self {
        Self {
            meta: ParameterMeta::new(name),
            metric,
            threshold,
        }
    }
}

impl ParameterBuilder<f64> for MaxParameterBuilder {
    fn name(&self) -> &ParameterName {
        &self.meta.name
    }

    fn build(
        self: Box<Self>,
        resolution_maps: &ResolutionMaps,
    ) -> Result<MaybeBuiltParameter<f64>, ParameterBuildError> {
        let metric = resolve_metric_f64!(self, self.metric, resolution_maps, "metric");

        let p = MaxParameter {
            meta: self.meta,
            metric,
            threshold: self.threshold,
        };

        Ok(MaybeBuiltParameter::Built(BuiltParameter::General(Box::new(p))))
    }
}
// ANCHOR_END: impl-builder

// ANCHOR_END: impl-parameter
mod schema {
    #[cfg(feature = "core")]
    use pywr_core::parameters::ParameterName;
    use pywr_schema::metric::Metric;
    use pywr_schema::parameters::ParameterMeta;
    #[cfg(feature = "core")]
    use pywr_schema::{LoadArgs, SchemaError};
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
        pub fn add_to_network(
            &self,
            network: &mut pywr_core::network::NetworkBuilder,
            args: &LoadArgs,
            parent: Option<&str>,
        ) -> Result<(), SchemaError> {
            let idx = self.parameter.load(network, args, None)?;
            let threshold = self.threshold.unwrap_or(0.0);

            let p = pywr_core::parameters::MaxParameterBuilder::new(
                ParameterName::new(&self.meta.name, parent),
                idx,
                threshold,
            );

            network.parameters().f64(Box::new(p));

            Ok(())
        }
    }
    // ANCHOR_END: schema-impl
}

fn main() {
    println!("Hello, world!");
}
