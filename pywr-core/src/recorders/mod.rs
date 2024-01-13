mod aggregator;
mod csv;
mod hdf;
mod metric_set;
mod py;

pub use self::csv::CSVRecorder;
use crate::metric::{IndexMetric, Metric};
use crate::models::ModelDomain;
use crate::network::Network;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
use crate::PywrError;
pub use aggregator::{AggregationFrequency, AggregationFunction, Aggregator};
use float_cmp::{approx_eq, ApproxEq, F64Margin};
pub use hdf::HDF5Recorder;
pub use metric_set::{MetricSet, MetricSetIndex, MetricSetState};
use ndarray::prelude::*;
use ndarray::Array2;
use std::any::Any;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Deref;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct RecorderIndex(usize);

impl RecorderIndex {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }
}

impl Deref for RecorderIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for RecorderIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Meta data common to all parameters.
#[derive(Clone, Debug)]
pub struct RecorderMeta {
    pub name: String,
    pub comment: String,
}

impl RecorderMeta {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            comment: "".to_string(),
        }
    }
}

pub trait Recorder: Send + Sync {
    fn meta(&self) -> &RecorderMeta;
    fn name(&self) -> &str {
        self.meta().name.as_str()
    }
    fn setup(&self, _domain: &ModelDomain, _model: &Network) -> Result<Option<Box<dyn Any>>, PywrError> {
        Ok(None)
    }
    fn before(&self) {}

    fn save(
        &self,
        _timestep: &Timestep,
        _scenario_indices: &[ScenarioIndex],
        _model: &Network,
        _state: &[State],
        _metric_set_states: &[Vec<MetricSetState>],
        _internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        Ok(())
    }
    fn finalise(&self, _internal_state: &mut Option<Box<dyn Any>>) -> Result<(), PywrError> {
        Ok(())
    }
}

pub struct Array2Recorder {
    meta: RecorderMeta,
    metric: Metric,
}

impl Array2Recorder {
    pub fn new(name: &str, metric: Metric) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            metric,
        }
    }
}

impl Recorder for Array2Recorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn setup(&self, domain: &ModelDomain, _model: &Network) -> Result<Option<Box<(dyn Any)>>, PywrError> {
        let array: Array2<f64> = Array::zeros((domain.time().len(), domain.scenarios().len()));

        Ok(Some(Box::new(array)))
    }

    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        model: &Network,
        state: &[State],
        _metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        // Downcast the internal state to the correct type
        let array = match internal_state {
            Some(internal) => match internal.downcast_mut::<Array2<f64>>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        // This panics if out-of-bounds
        for scenario_index in scenario_indices {
            let value = self.metric.get_value(model, &state[scenario_index.index])?;
            array[[timestep.index, scenario_index.index]] = value
        }

        Ok(())
    }
}

pub struct AssertionRecorder {
    meta: RecorderMeta,
    expected_values: Array2<f64>,
    metric: Metric,
    ulps: i64,
    epsilon: f64,
}

impl AssertionRecorder {
    pub fn new(
        name: &str,
        metric: Metric,
        expected_values: Array2<f64>,
        ulps: Option<i64>,
        epsilon: Option<f64>,
    ) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            expected_values,
            metric,
            ulps: ulps.unwrap_or(5),
            epsilon: epsilon.unwrap_or(1e-6),
        }
    }
}

impl Recorder for AssertionRecorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        model: &Network,
        state: &[State],
        _metric_set_states: &[Vec<MetricSetState>],
        _internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        // This panics if out-of-bounds

        for scenario_index in scenario_indices {
            let expected_value = match self.expected_values.get([timestep.index, scenario_index.index]) {
                Some(v) => *v,
                None => panic!("Simulation produced results out of range."),
            };

            let actual_value = self.metric.get_value(model, &state[scenario_index.index])?;

            if !actual_value.approx_eq(
                expected_value,
                F64Margin {
                    epsilon: self.epsilon,
                    ulps: self.ulps,
                },
            ) {
                panic!(
                    r#"assertion failed: (actual approx_eq expected)
recorder: `{}`
timestep: `{:?}` ({})
scenario: `{:?}`
actual: `{:?}`
expected: `{:?}`"#,
                    self.meta.name, timestep.date, timestep.index, scenario_index.index, actual_value, expected_value
                )
            }
        }

        Ok(())
    }
}

pub struct AssertionFnRecorder<F> {
    meta: RecorderMeta,
    expected_func: F,
    metric: Metric,
    ulps: i64,
    epsilon: f64,
}

impl<F> AssertionFnRecorder<F>
where
    F: Fn(&Timestep, &ScenarioIndex) -> f64,
{
    pub fn new(name: &str, metric: Metric, expected_func: F, ulps: Option<i64>, epsilon: Option<f64>) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            expected_func,
            metric,
            ulps: ulps.unwrap_or(2),
            epsilon: epsilon.unwrap_or(f64::EPSILON * 2.0),
        }
    }
}

impl<F> Recorder for AssertionFnRecorder<F>
where
    F: Send + Sync + Fn(&Timestep, &ScenarioIndex) -> f64,
{
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        model: &Network,
        state: &[State],
        _metric_set_states: &[Vec<MetricSetState>],
        _internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        // This panics if out-of-bounds

        for scenario_index in scenario_indices {
            let expected_value = (self.expected_func)(timestep, scenario_index);
            let actual_value = self.metric.get_value(model, &state[scenario_index.index])?;

            if !approx_eq!(
                f64,
                actual_value,
                expected_value,
                epsilon = self.epsilon,
                ulps = self.ulps
            ) {
                panic!(
                    r#"assertion failed at timestep {timestep:?} in scenario {scenario_index:?}: `(actual approx_eq expected)`
   actual: `{actual_value:?}`,
 expected: `{expected_value:?}`"#,
                )
            }
        }

        Ok(())
    }
}

pub struct IndexAssertionRecorder {
    meta: RecorderMeta,
    expected_values: Array2<usize>,
    metric: IndexMetric,
}

impl IndexAssertionRecorder {
    pub fn new(name: &str, metric: IndexMetric, expected_values: Array2<usize>) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            expected_values,
            metric,
        }
    }
}

impl Recorder for IndexAssertionRecorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        network: &Network,
        state: &[State],
        _metric_set_states: &[Vec<MetricSetState>],
        _internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        // This panics if out-of-bounds

        for scenario_index in scenario_indices {
            let expected_value = match self.expected_values.get([timestep.index, scenario_index.index]) {
                Some(v) => *v,
                None => panic!("Simulation produced results out of range."),
            };

            let actual_value = self.metric.get_value(network, &state[scenario_index.index])?;

            if actual_value != expected_value {
                panic!(
                    r#"assertion failed: (actual eq expected)
recorder: `{}`
timestep: `{:?}` ({})
scenario: `{:?}`
actual: `{:?}`
expected: `{:?}`"#,
                    self.meta.name, timestep.date, timestep.index, scenario_index.index, actual_value, expected_value
                )
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{run_all_solvers, simple_model};

    #[test]
    fn test_array2_recorder() {
        let mut model = simple_model(2);

        let node_idx = model.network().get_node_index_by_name("input", None).unwrap();

        let rec = Array2Recorder::new("test", Metric::NodeOutFlow(node_idx));

        let _idx = model.network_mut().add_recorder(Box::new(rec)).unwrap();
        // Test all solvers
        run_all_solvers(&model);

        // TODO fix this with respect to the trait.
        // let array = rec.data_view2().unwrap();
        // assert_almost_eq!(array[[0, 0]], 10.0);
    }
}
