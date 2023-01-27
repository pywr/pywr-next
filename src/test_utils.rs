use crate::metric::Metric;
/// Utilities for unit tests.
/// TODO move this to its own local crate ("test-utilities") as part of a workspace.
use crate::model::Model;
use crate::node::{Constraint, ConstraintValue, StorageInitialVolume};
use crate::parameters::{AggFunc, AggregatedParameter, ConstantParameter, VectorParameter};
use crate::timestep::Timestepper;
use time::macros::date;

pub fn default_timestepper() -> Timestepper {
    Timestepper::new(date!(2020 - 01 - 01), date!(2020 - 01 - 15), 1)
}

/// Create a simple test model with three nodes.
pub fn simple_model() -> Model {
    let mut model = Model::default();
    model.add_scenario_group("test-scenario", 2).unwrap();

    let input_node = model.add_input_node("input", None).unwrap();
    let link_node = model.add_link_node("link", None).unwrap();
    let output_node = model.add_output_node("output", None).unwrap();

    model.connect_nodes(input_node, link_node).unwrap();
    model.connect_nodes(link_node, output_node).unwrap();

    let inflow = VectorParameter::new("inflow", vec![10.0; 366]);
    let inflow = model.add_parameter(Box::new(inflow)).unwrap();

    let input_node = model.get_mut_node_by_name("input", None).unwrap();
    input_node
        .set_constraint(
            ConstraintValue::Metric(Metric::ParameterValue(inflow)),
            Constraint::MaxFlow,
        )
        .unwrap();

    let base_demand = 10.0;

    let demand_factor = ConstantParameter::new("demand-factor", 1.2);
    let demand_factor = model.add_parameter(Box::new(demand_factor)).unwrap();

    let total_demand = AggregatedParameter::new(
        "total-demand",
        &[Metric::Constant(base_demand), Metric::ParameterValue(demand_factor)],
        AggFunc::Product,
    );
    let total_demand = model.add_parameter(Box::new(total_demand)).unwrap();

    let demand_cost = ConstantParameter::new("demand-cost", -10.0);
    let demand_cost = model.add_parameter(Box::new(demand_cost)).unwrap();

    let output_node = model.get_mut_node_by_name("output", None).unwrap();
    output_node
        .set_constraint(
            ConstraintValue::Metric(Metric::ParameterValue(total_demand)),
            Constraint::MaxFlow,
        )
        .unwrap();
    output_node.set_cost(ConstraintValue::Metric(Metric::ParameterValue(demand_cost)));

    model
}

/// A test model with a single storage node.
pub fn simple_storage_model() -> Model {
    let mut model = Model::default();

    let storage_node = model
        .add_storage_node(
            "reservoir",
            None,
            StorageInitialVolume::Absolute(100.0),
            0.0,
            ConstraintValue::Scalar(100.0),
        )
        .unwrap();
    let output_node = model.add_output_node("output", None).unwrap();

    model.connect_nodes(storage_node, output_node).unwrap();

    // Apply demand to the model
    // TODO convenience function for adding a constant constraint.
    let demand = ConstantParameter::new("demand", 10.0);
    let demand = model.add_parameter(Box::new(demand)).unwrap();

    let demand_cost = ConstantParameter::new("demand-cost", -10.0);
    let demand_cost = model.add_parameter(Box::new(demand_cost)).unwrap();

    let output_node = model.get_mut_node_by_name("output", None).unwrap();
    output_node
        .set_constraint(
            ConstraintValue::Metric(Metric::ParameterValue(demand)),
            Constraint::MaxFlow,
        )
        .unwrap();
    output_node.set_cost(ConstraintValue::Metric(Metric::ParameterValue(demand_cost)));

    let max_volume = 100.0;

    let storage_node = model.get_mut_node_by_name("reservoir", None).unwrap();
    storage_node
        .set_constraint(ConstraintValue::Scalar(max_volume), Constraint::MaxVolume)
        .unwrap();

    model
}
