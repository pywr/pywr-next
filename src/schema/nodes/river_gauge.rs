use crate::schema::nodes::NodeMeta;
use crate::schema::parameters::{DynamicFloatValue, ParameterFloatValue};
use crate::PywrError;

#[doc = svgbobdoc::transform!(
/// This is used to represent a minimum residual flow (MRF) at a gauging station.
///
///
/// ```svgbob
///            <node>.mrf
///          .------>L -----.
///      U  |                |     D
///     -*--|                |--->*- - -
///         |                |
///          '------>L -----'
///            <node>.bypass
/// ```
///
)]
#[derive(serde::Deserialize, serde::Serialize)]
pub struct RiverGaugeNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub mrf: Option<DynamicFloatValue>,
    pub mrf_cost: Option<DynamicFloatValue>,
}

impl RiverGaugeNode {
    fn mrf_sub_name() -> Option<&'static str> {
        Some("mrf")
    }

    fn bypass_sub_name() -> Option<&'static str> {
        Some("bypass")
    }

    pub fn add_to_model(&self, model: &mut crate::model::Model) -> Result<(), PywrError> {
        model.add_link_node(self.meta.name.as_str(), Self::mrf_sub_name())?;
        model.add_link_node(self.meta.name.as_str(), Self::bypass_sub_name())?;

        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![
            (self.meta.name.as_str(), Self::mrf_sub_name()),
            (self.meta.name.as_str(), Self::bypass_sub_name()),
        ]
    }

    pub fn output_connectors(&self) -> Vec<(&str, Option<&str>)> {
        vec![
            (self.meta.name.as_str(), Self::mrf_sub_name()),
            (self.meta.name.as_str(), Self::bypass_sub_name()),
        ]
    }
}

#[cfg(test)]
mod tests {

    use crate::scenario::ScenarioGroupCollection;
    use crate::schema::model::PywrModel;
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

    fn model_str() -> &'static str {
        r#"
            {
                "metadata": {
                    "title": "Simple 1",
                    "description": "A very simple example.",
                    "minimum_version": "0.1"
                },
                "timestepper": {
                    "start": "2015-01-01",
                    "end": "2015-12-31",
                    "timestep": 1
                },
                "nodes": [
                    {
                        "name": "catchment1",
                        "type": "Catchment",
                        "flow": 15
                    },
                    {
                        "name": "gauge1",
                        "type": "RiverGauge",
                        "mrf": 5.0,
                        "mrf_cost": -20.0
                    },
                    {
                        "name": "term1",
                        "type": "Output"
                    },
                    {
                        "name": "demand1",
                        "type": "Output",
                        "max_flow": 15.0,
                        "cost": -10
                    }
                ],
                "edges": [
                    {
                        "from_node": "catchment1",
                        "to_node": "gauge1"
                    },
                    {
                        "from_node": "gauge1",
                        "to_node": "term1"
                    },
                    {
                        "from_node": "gauge1",
                        "to_node": "demand1"
                    }
                ]
            }
            "#
    }

    #[test]
    fn test_model_schema() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();

        assert_eq!(schema.nodes.len(), 4);
        assert_eq!(schema.edges.len(), 3);
    }

    #[test]
    fn test_model_run() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let mut model: crate::model::Model = schema.try_into().unwrap();

        assert_eq!(model.nodes.len(), 5);
        assert_eq!(model.edges.len(), 6);

        let timestepper = default_timestepper();
        let scenarios = default_scenarios();
        let mut solver: Box<dyn Solver> = Box::new(ClpSolver::new());

        model.run(timestepper, scenarios, &mut solver).unwrap()

        // TODO assert the results!
    }
}
