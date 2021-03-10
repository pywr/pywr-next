pub(crate) mod py;
#[macro_use]
use crate::assert_almost_eq;
use crate::metric::Metric;
use crate::scenario::ScenarioIndex;
use crate::timestep::Timestep;
use crate::{NetworkState, ParameterState, PywrError};
use ndarray::prelude::*;
use ndarray::Array2;

pub type RecorderIndex = usize;

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

pub trait Recorder {
    fn meta(&self) -> &RecorderMeta;
    fn setup(&mut self) -> Result<(), PywrError> {
        Ok(())
    }
    fn before(&self) {}
    fn save(
        &mut self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<(), PywrError>;
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

    fn setup(&mut self) -> Result<(), PywrError> {
        self.array = Some(Array::zeros((10, 2)));

        Ok(())
    }

    fn save(
        &mut self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<(), PywrError> {
        // This panics if out-of-bounds

        match &mut self.array {
            Some(array) => {
                let value = self.metric.get_value(state, parameter_state)?;
                array[[timestep.index, scenario_index.index]] = value
            }
            None => return Err(PywrError::RecorderNotInitialised),
        };

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
        &mut self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<(), PywrError> {
        // This panics if out-of-bounds

        let expected_value = match self.expected_values.get([timestep.index, scenario_index.index]) {
            Some(v) => *v,
            None => panic!("Simulation produced results out of range."),
        };

        assert_almost_eq!(self.metric.get_value(state, parameter_state)?, expected_value);

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
    use crate::state::{EdgeState, NodeState};
    #[macro_use]
    use crate::assert_almost_eq;
    use super::*;

    #[test]
    fn test_array2_recorder() {
        let mut state = NetworkState::new();
        state.push_node_state(NodeState::new_flow_state());
        state.push_node_state(NodeState::new_flow_state());
        state.push_edge_state(EdgeState::new());

        let parameter_state = ParameterState::new();
        let timestep = Timestep::parse_from_str("2020-01-01", "%Y-%m-%d", 0, 1).unwrap();
        let scenario_index = ScenarioIndex::new(0, vec![0]);

        state.add_flow(0, 0, 1, &timestep, 10.0).unwrap();

        let mut rec = Array2Recorder::new("test", Metric::NodeOutFlow(0));
        rec.setup().unwrap();
        rec.save(&timestep, &scenario_index, &state, &parameter_state).unwrap();
        assert_almost_eq!(rec.array.unwrap()[[0, 0]], 10.0);
    }
}
