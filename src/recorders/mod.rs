pub mod hdf;
pub mod py;

use crate::assert_almost_eq;
use crate::metric::Metric;
use crate::model::Model;
use crate::scenario::ScenarioIndex;
use crate::state::ParameterState;
use crate::timestep::Timestep;
use crate::{NetworkState, PywrError};
use ndarray::prelude::*;
use ndarray::Array2;
use std::cell::RefCell;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::rc::Rc;

pub type RecorderIndex = usize;
pub type RecorderRef = Rc<RefCell<Box<dyn _Recorder>>>;

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

pub trait _Recorder {
    fn meta(&self) -> &RecorderMeta;
    fn setup(
        &mut self,
        _model: &Model,
        _timesteps: &Vec<Timestep>,
        _scenario_indices: &Vec<ScenarioIndex>,
    ) -> Result<(), PywrError> {
        Ok(())
    }
    fn before(&self) {}
    fn save(
        &mut self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Model,
        network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<(), PywrError>;
    fn after_save(&mut self, _timestep: &Timestep) -> Result<(), PywrError> {
        Ok(())
    }
    fn finalise(&mut self) -> Result<(), PywrError> {
        Ok(())
    }

    // Data access
    fn data_view2(&self) -> Result<Array2<f64>, PywrError> {
        Err(PywrError::NotSupportedByRecorder)
    }
}

#[derive(Clone)]
pub struct Recorder(RecorderRef, RecorderIndex);

impl PartialEq for Recorder {
    fn eq(&self, other: &Recorder) -> bool {
        // TODO which
        self.1 == other.1
    }
}

impl fmt::Debug for Recorder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Recorder").field(&self.name()).field(&self.1).finish()
    }
}

impl Recorder {
    pub fn new(parameter: Box<dyn _Recorder>, index: RecorderIndex) -> Self {
        Self(Rc::new(RefCell::new(parameter)), index)
    }

    pub fn index(&self) -> RecorderIndex {
        self.1
    }

    pub fn name(&self) -> String {
        self.0.borrow().deref().meta().name.to_string()
    }

    pub fn setup(
        &self,
        model: &Model,
        timesteps: &Vec<Timestep>,
        scenario_indices: &Vec<ScenarioIndex>,
    ) -> Result<(), PywrError> {
        self.0
            .borrow_mut()
            .deref_mut()
            .setup(model, timesteps, scenario_indices)
    }

    pub fn save(
        &self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Model,
        network_state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<(), PywrError> {
        self.0
            .borrow_mut()
            .deref_mut()
            .save(timestep, scenario_index, model, network_state, parameter_state)
    }

    pub fn after_save(&self, timestep: &Timestep) -> Result<(), PywrError> {
        self.0.borrow_mut().deref_mut().after_save(timestep)
    }

    pub fn finalise(&self) -> Result<(), PywrError> {
        self.0.borrow_mut().deref_mut().finalise()
    }

    fn data_view2(&self) -> Result<Array2<f64>, PywrError> {
        match self.0.borrow().deref().data_view2() {
            Ok(av) => Ok(av),
            Err(e) => Err(e),
        }
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

impl _Recorder for Array2Recorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn setup(
        &mut self,
        _model: &Model,
        _timesteps: &Vec<Timestep>,
        _scenario_indices: &Vec<ScenarioIndex>,
    ) -> Result<(), PywrError> {
        // TODO set this up properly.
        self.array = Some(Array::zeros((365, 10)));

        Ok(())
    }

    fn save(
        &mut self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Model,
        state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<(), PywrError> {
        // This panics if out-of-bounds

        match &mut self.array {
            Some(array) => {
                let value = self.metric.get_value(model, state, parameter_state)?;
                array[[timestep.index, scenario_index.index]] = value
            }
            None => return Err(PywrError::RecorderNotInitialised),
        };

        Ok(())
    }

    fn data_view2(&self) -> Result<Array2<f64>, PywrError> {
        match &self.array {
            Some(a) => Ok(a.clone()),
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

impl _Recorder for AssertionRecorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn save(
        &mut self,
        timestep: &Timestep,
        scenario_index: &ScenarioIndex,
        model: &Model,
        state: &NetworkState,
        parameter_state: &ParameterState,
    ) -> Result<(), PywrError> {
        // This panics if out-of-bounds

        let expected_value = match self.expected_values.get([timestep.index, scenario_index.index]) {
            Some(v) => *v,
            None => panic!("Simulation produced results out of range."),
        };

        assert_almost_eq!(self.metric.get_value(model, state, parameter_state)?, expected_value);

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
    use crate::node::{Constraint, ConstraintValue};
    use crate::parameters;
    use crate::scenario::ScenarioGroupCollection;
    use crate::solvers::clp::ClpSolver;
    use crate::solvers::Solver;
    use crate::timestep::Timestepper;
    use time::macros::date;

    fn default_timestepper() -> Timestepper {
        Timestepper::new(date!(2020 - 01 - 01), date!(2020 - 01 - 15), 1)
    }

    fn default_scenarios() -> ScenarioGroupCollection {
        let mut scenarios = ScenarioGroupCollection::new();
        scenarios.add_group("test-scenario", 10);
        scenarios
    }

    /// Create a simple test model with three nodes.
    fn simple_model() -> Model {
        let mut model = Model::new();

        let input_node = model.add_input_node("input", None).unwrap();
        let link_node = model.add_link_node("link", None).unwrap();
        let output_node = model.add_output_node("output", None).unwrap();

        model.connect_nodes(input_node, link_node).unwrap();
        model.connect_nodes(link_node, output_node).unwrap();

        let inflow = parameters::VectorParameter::new("inflow", vec![10.0; 366]);
        let inflow_idx = model.add_parameter(Box::new(inflow)).unwrap();

        let input_node = model.get_mut_node_by_name("input", None).unwrap();
        input_node
            .set_constraint(ConstraintValue::Parameter(inflow_idx), Constraint::MaxFlow)
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

        let demand_cost = parameters::ConstantParameter::new("demand-cost", -10.0);
        let demand_cost_idx = model.add_parameter(Box::new(demand_cost)).unwrap();

        let output_node = model.get_mut_node_by_name("output", None).unwrap();
        output_node
            .set_constraint(ConstraintValue::Parameter(total_demand_idx), Constraint::MaxFlow)
            .unwrap();

        output_node.set_cost(ConstraintValue::Parameter(demand_cost_idx));

        model
    }

    #[test]
    fn test_array2_recorder() {
        let mut model = simple_model();
        let timestepper = default_timestepper();
        let scenarios = default_scenarios();
        let mut solver: Box<dyn Solver> = Box::new(ClpSolver::new());

        let node_idx = model.get_node_index_by_name("input", None).unwrap();

        let rec = Array2Recorder::new("test", Metric::NodeOutFlow(node_idx));

        let rec = model.add_recorder(Box::new(rec)).unwrap();
        model.run(timestepper, scenarios, &mut solver).unwrap();

        let array = rec.data_view2().unwrap();

        assert_almost_eq!(array[[0, 0]], 10.0);
    }
}
