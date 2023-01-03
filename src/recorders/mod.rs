pub mod hdf;
pub mod py;

use crate::assert_almost_eq;
use crate::metric::Metric;
use crate::scenario::ScenarioIndex;
use crate::timestep::Timestep;
use crate::PywrError;
use ndarray::prelude::*;
use ndarray::Array2;
use std::any::Any;
use std::fmt;
use std::fmt::{Display, Formatter};

use crate::model::Model;
use crate::state::State;
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
    pub index: Option<RecorderIndex>,
    pub name: String,
    pub comment: String,
}

impl RecorderMeta {
    fn new(name: &str) -> Self {
        Self {
            index: None,
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
    fn setup(
        &self,
        _timesteps: &[Timestep],
        _scenario_indices: &[ScenarioIndex],
    ) -> Result<Option<Box<dyn Any>>, PywrError> {
        Ok(None)
    }
    fn before(&self) {}

    fn save(
        &self,
        _timestep: &Timestep,
        _scenario_indices: &[ScenarioIndex],
        _model: &Model,
        _state: &[State],
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
    array: Option<Array2<f64>>,
    metric: Metric,
}

impl Array2Recorder {
    pub fn new(name: &str, metric: Metric) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            array: None,
            metric,
        }
    }
}

impl Recorder for Array2Recorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn setup(
        &self,
        timesteps: &[Timestep],
        scenario_indices: &[ScenarioIndex],
    ) -> Result<Option<Box<(dyn Any)>>, PywrError> {
        let array: Array2<f64> = Array::zeros((timesteps.len(), scenario_indices.len()));

        Ok(Some(Box::new(array)))
    }

    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        model: &Model,
        state: &[State],
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
}

impl AssertionRecorder {
    pub fn new(name: &str, metric: Metric, expected_values: Array2<f64>) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            expected_values,
            metric,
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
        model: &Model,
        state: &[State],
        _internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        // This panics if out-of-bounds

        for scenario_index in scenario_indices {
            let expected_value = match self.expected_values.get([timestep.index, scenario_index.index]) {
                Some(v) => *v,
                None => panic!("Simulation produced results out of range."),
            };

            assert_almost_eq!(
                self.metric.get_value(model, &state[scenario_index.index])?,
                expected_value
            );
        }

        Ok(())
    }
}

pub enum RecorderAggregation {
    Min,
    Max,
    Mean,
    Median,
    Sum,
    Quantile(f64),
    CountNonZero,
    CountAboveThreshold(f64),
}

pub enum Direction {
    Minimise,
    Maximise,
}

struct RecorderMetric {
    temporal_aggregation: RecorderAggregation,
    scenario_aggregation: RecorderAggregation,
    lower_bounds: Option<f64>,
    upper_bounds: Option<f64>,
    objective: Option<Direction>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solvers::clp::ClpSolver;
    use crate::test_utils::{default_scenarios, default_timestepper, simple_model};

    #[test]
    fn test_array2_recorder() {
        let mut model = simple_model();
        let timestepper = default_timestepper();
        let scenarios = default_scenarios();

        let node_idx = model.get_node_index_by_name("input", None).unwrap();

        let rec = Array2Recorder::new("test", Metric::NodeOutFlow(node_idx));

        let idx = model.add_recorder(Box::new(rec)).unwrap();
        model.run::<ClpSolver>(&timestepper, &scenarios).unwrap();

        // TODO fix this with respect to the trait.
        // let array = rec.data_view2().unwrap();
        // assert_almost_eq!(array[[0, 0]], 10.0);
    }
}
