pub(crate) mod py;

use crate::assert_almost_eq;
use crate::metric::Metric;
use crate::scenario::ScenarioIndex;
use crate::timestep::Timestep;
use crate::{NetworkState, PywrError};
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
        parameter_state: &[f64],
    ) -> Result<(), PywrError>;
    fn after_save(&mut self, timestep: &Timestep) -> Result<(), PywrError> {
        Ok(())
    }

    // Data access
    fn data_view2(&self) -> Result<ArrayView2<f64>, PywrError> {
        Err(PywrError::NotSupportedByRecorder)
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

    fn setup(&mut self) -> Result<(), PywrError> {
        // TODO set this up properly.
        self.array = Some(Array::zeros((365, 10)));

        Ok(())
    }

    fn save(
        &mut self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        state: &NetworkState,
        parameter_state: &[f64],
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

    fn data_view2(&self) -> Result<ArrayView2<f64>, PywrError> {
        match &self.array {
            Some(a) => Ok(a.view()),
            None => Err(PywrError::RecorderNotInitialised),
        }
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
        parameter_state: &[f64],
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
    use super::*;
    use crate::assert_almost_eq;
    use crate::model::Model;
    use crate::node::Constraint;
    use crate::parameters;
    use crate::scenario::ScenarioGroupCollection;
    use crate::solvers::clp::ClpSolver;
    use crate::solvers::Solver;
    use crate::state::{EdgeState, NodeState, ParameterState};
    use crate::timestep::Timestepper;
    use tempdir::TempDir;

    fn default_timestepper() -> Timestepper {
        Timestepper::new("2020-01-01", "2020-01-15", "%Y-%m-%d", 1).unwrap()
    }

    fn default_scenarios() -> ScenarioGroupCollection {
        let mut scenarios = ScenarioGroupCollection::new();
        scenarios.add_group("test-scenario", 10);
        scenarios
    }

    /// Create a simple test model with three nodes.
    fn simple_model() -> Model {
        let mut model = Model::new();

        let input_node_idx = model.add_input_node("input").unwrap();
        let link_node_idx = model.add_link_node("link").unwrap();
        let output_node_idx = model.add_output_node("output").unwrap();

        model.connect_nodes(input_node_idx, link_node_idx).unwrap();
        model.connect_nodes(link_node_idx, output_node_idx).unwrap();

        let inflow = parameters::VectorParameter::new("inflow", vec![10.0; 366]);
        let inflow_idx = model.add_parameter(Box::new(inflow)).unwrap();
        model
            .set_node_constraint(input_node_idx, Some(inflow_idx), Constraint::MaxFlow)
            .unwrap();

        let base_demand = parameters::ConstantParameter::new("base-demand", 10.0);
        let base_demand_idx = model.add_parameter(Box::new(base_demand)).unwrap();

        let demand_factor = parameters::ConstantParameter::new("demand-factor", 1.2);
        let demand_factor_idx = model.add_parameter(Box::new(demand_factor)).unwrap();

        let total_demand = parameters::AggregatedParameter::new(
            "total-demand",
            vec![base_demand_idx, demand_factor_idx],
            parameters::AggFunc::Product,
        );
        let total_demand_idx = model.add_parameter(Box::new(total_demand)).unwrap();

        model
            .set_node_constraint(output_node_idx, Some(total_demand_idx), Constraint::MaxFlow)
            .unwrap();

        let demand_cost = parameters::ConstantParameter::new("demand-cost", -10.0);
        let demand_cost_idx = model.add_parameter(Box::new(demand_cost)).unwrap();
        model.set_node_cost(output_node_idx, Some(demand_cost_idx)).unwrap();

        model
    }

    #[test]
    fn test_array2_recorder() {
        let mut model = simple_model();
        let timestepper = default_timestepper();
        let scenarios = default_scenarios();
        let mut solver: Box<dyn Solver> = Box::new(ClpSolver::new());

        let rec = Array2Recorder::new("test", Metric::NodeOutFlow(0));

        let rec_idx = model.add_recorder(Box::new(rec)).unwrap();
        model.run(timestepper, scenarios, &mut solver).unwrap();

        let array = model.get_recorder_view2(rec_idx).unwrap();

        assert_almost_eq!(array[[0, 0]], 10.0);
    }
}
