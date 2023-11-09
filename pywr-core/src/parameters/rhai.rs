use super::{IndexValue, Parameter, ParameterMeta, PywrError, Timestep};
use crate::metric::Metric;
use crate::network::Network;
use crate::parameters::downcast_internal_state;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use rhai::{Dynamic, Engine, Map, Scope, AST};
use std::any::Any;
use std::collections::HashMap;

pub struct RhaiParameter {
    meta: ParameterMeta,
    engine: Engine,
    ast: AST,
    initial_state: Map,
    metrics: HashMap<String, Metric>,
    indices: HashMap<String, IndexValue>,
}

struct Internal {
    state: Dynamic,
}

impl RhaiParameter {
    pub fn new(
        name: &str,
        script: &str,
        initial_state: Map,
        metrics: &HashMap<String, Metric>,
        indices: &HashMap<String, IndexValue>,
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
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn meta(&self) -> &ParameterMeta {
        &self.meta
    }

    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_index: &ScenarioIndex,
    ) -> Result<Option<Box<dyn Any + Send>>, PywrError> {
        let mut scope = Scope::new();

        let mut state = self.initial_state.clone().into();
        let options = rhai::CallFnOptions::new().bind_this_ptr(&mut state);

        self.engine
            .call_fn_with_options::<()>(options, &mut scope, &self.ast, "init", ())
            .expect("Failed to run Rhai init function.");

        let internal = Internal { state };

        Ok(Some(Box::new(internal)))
    }

    fn compute(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Network,
        state: &State,
        internal_state: &mut Option<Box<dyn Any + Send>>,
    ) -> Result<f64, PywrError> {
        let internal = downcast_internal_state::<Internal>(internal_state);

        let metric_values = self
            .metrics
            .iter()
            .map(|(k, value)| Ok((k.into(), value.get_value(model, state)?.into())))
            .collect::<Result<rhai::Map, PywrError>>()?;

        let index_values = self
            .indices
            .iter()
            .map(|(k, value)| Ok((k.into(), (value.get_index(state)? as i64).into())))
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
}

#[cfg(test)]
mod tests {
    use super::*;
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
            "my-counter",
            script,
            initial_state,
            &Default::default(),
            &Default::default(),
        );

        let timestepper = default_timestepper();
        let time: TimeDomain = timestepper.into();
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

        let state = State::new(vec![], 0, vec![], 1, 0, 0, 0);

        let mut internal_p_states: Vec<_> = scenario_indices
            .iter()
            .map(|si| param.setup(&timesteps, si).expect("Could not setup the PyParameter"))
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
