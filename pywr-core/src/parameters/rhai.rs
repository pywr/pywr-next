use super::{GeneralParameter, Parameter, ParameterMeta, ParameterName, ParameterState, PywrError, Timestep};
use crate::metric::{MetricF64, MetricUsize};
use crate::network::Network;
use crate::parameters::downcast_internal_state_mut;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use chrono::Datelike;
use rhai::{Dynamic, Engine, Map, Scope, AST};
use std::collections::HashMap;

pub struct RhaiParameter {
    meta: ParameterMeta,
    engine: Engine,
    ast: AST,
    initial_state: Map,
    metrics: HashMap<String, MetricF64>,
    indices: HashMap<String, MetricUsize>,
}

#[derive(Clone)]
struct Internal {
    state: Dynamic,
}

impl RhaiParameter {
    pub fn new(
        name: ParameterName,
        script: &str,
        initial_state: Map,
        metrics: &HashMap<String, MetricF64>,
        indices: &HashMap<String, MetricUsize>,
    ) -> Self {
        let mut engine = Engine::new();

        // Register Timestep
        engine
            .register_type_with_name::<Timestep>("Timestep")
            .register_get("index", |ts: &mut Timestep| ts.index as i64)
            .register_get("day", |ts: &mut Timestep| ts.date.day() as i64)
            .register_get("month", |ts: &mut Timestep| ts.date.month() as i64)
            .register_get("year", |ts: &mut Timestep| ts.date.year() as i64);

        // Should this be compile already?
        let ast = engine.compile(script).expect("Failed to compile Rhai script.");

        Self {
            meta: ParameterMeta::new(name),
            engine,
            ast,
            initial_state,
            metrics: metrics.clone(),
            indices: indices.clone(),
        }
    }
}

impl Parameter for RhaiParameter {
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn ParameterState>>, PywrError> {
        let mut scope = Scope::new();

        let mut state = self.initial_state.clone().into();
        let options = rhai::CallFnOptions::new().bind_this_ptr(&mut state);

        self.engine
            .call_fn_with_options::<()>(options, &mut scope, &self.ast, "init", ())
            .expect("Failed to run Rhai init function.");

        let internal = Internal { state };

        Ok(Some(Box::new(internal)))
    }
}

impl GeneralParameter<f64> for RhaiParameter {
    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn ParameterState>>,
    ) -> Result<f64, PywrError> {
        let internal = downcast_internal_state_mut::<Internal>(internal_state);

        let metric_values = self
            .metrics
            .iter()
            .map(|(k, value)| Ok((k.into(), value.get_value(network, state)?.into())))
            .collect::<Result<rhai::Map, PywrError>>()?;

        let index_values = self
            .indices
            .iter()
            .map(|(k, value)| Ok((k.into(), (value.get_value(network, state)? as i64).into())))
            .collect::<Result<rhai::Map, PywrError>>()?;

        let args = (*timestep, scenario_index.index as i64, metric_values, index_values);

        let options = rhai::CallFnOptions::new().bind_this_ptr(&mut internal.state);
        let mut scope = Scope::new();
        let value: f64 = self
            .engine
            .call_fn_with_options(options, &mut scope, &self.ast, "compute", args)
            .expect("Failed to run Rhai compute function.");

        Ok(value)
    }

    fn as_parameter(&self) -> &dyn Parameter
    where
        Self: Sized,
    {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StateBuilder;
    use crate::test_utils::default_timestepper;
    use crate::timestep::TimeDomain;
    use float_cmp::assert_approx_eq;

    #[test]
    fn test_counter_parameter() {
        let script = r#"
        // Compute the value to return
        fn init() {
            this.counter = 0;
        }

        fn compute(ts, si, mv, iv) {
            // `counter` is added to the scope from the initial state
            this.counter += si;
            to_float(this.counter + ts.day)
        }
        "#;

        // let initial_state: HashMap<String, Dynamic> = [("counter".to_string(), 0.into())].into();
        let initial_state = rhai::Map::new();

        let param = RhaiParameter::new(
            "my-counter".into(),
            script,
            initial_state,
            &Default::default(),
            &Default::default(),
        );

        let timestepper = default_timestepper();
        let time: TimeDomain = TimeDomain::try_from(timestepper).unwrap();
        let timesteps = time.timesteps();

        let scenario_indices = [
            ScenarioIndex {
                index: 0,
                indices: vec![0],
            },
            ScenarioIndex {
                index: 1,
                indices: vec![1],
            },
        ];

        let state = StateBuilder::new(vec![], 0).build();

        let mut internal_p_states: Vec<_> = scenario_indices
            .iter()
            .map(|si| param.setup(timesteps, si).expect("Could not setup the PyParameter"))
            .collect();

        let model = Network::default();

        for ts in timesteps {
            for (si, internal) in scenario_indices.iter().zip(internal_p_states.iter_mut()) {
                let value = param.compute(ts, si, &model, &state, internal).unwrap();

                assert_approx_eq!(f64, value, ((ts.index + 1) * si.index + ts.date.day() as usize) as f64);
            }
        }
    }
}
