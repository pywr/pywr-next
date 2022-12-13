use crate::schema::data_tables::LoadedTableCollection;
use crate::schema::nodes::NodeMeta;
use crate::schema::parameters::{DynamicFloatValue, TryIntoV2Parameter};
use crate::PywrError;
use pywr_schema::nodes::RiverSplitWithGaugeNode as RiverSplitWithGaugeNodeV1;
use std::path::Path;

#[doc = svgbobdoc::transform!(
/// This is used to represent a proportional split above a minimum residual flow (MRF) at a gauging station.
///
///
/// ```svgbob
///           <node>.mrf
///          .------>L -----.
///      U  | <node>.bypass  |     D[slot_name_0]
///     -*--|------->L ------|--->*- - -
///         | <node>.split_1 |
///          '------>L -----'
///                  |             D[slot_names_1]
///                   '---------->*- - -
///
///         |                |
///         | <node>.split_i |
///          '------>L -----'
///                  |             D[slot_names_i]
///                   '---------->*- - -
/// ```
///
)]
#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct RiverSplitWithGaugeNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub mrf: Option<DynamicFloatValue>,
    pub mrf_cost: Option<DynamicFloatValue>,
    pub cost: Option<DynamicFloatValue>,
    pub factors: Vec<DynamicFloatValue>,
    pub slot_names: Vec<String>,
}

impl RiverSplitWithGaugeNode {
    fn mrf_sub_name() -> Option<&'static str> {
        Some("mrf")
    }

    fn bypass_sub_name() -> Option<&'static str> {
        Some("bypass")
    }

    fn split_sub_name(i: usize) -> Option<String> {
        Some(format!("split-{}", i))
    }

    pub fn add_to_model(&self, model: &mut crate::model::Model) -> Result<(), PywrError> {
        // TODO do this properly
        model.add_link_node(self.meta.name.as_str(), Self::mrf_sub_name())?;
        model.add_link_node(self.meta.name.as_str(), Self::bypass_sub_name())?;

        Ok(())
    }

    pub fn set_constraints(
        &self,
        model: &mut crate::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<(), PywrError> {
        // MRF applies as a maximum on the MRF node.
        if let Some(cost) = &self.mrf_cost {
            let value = cost.load(model, tables, data_path)?;
            model.set_node_cost(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

        if let Some(mrf) = &self.mrf {
            let value = mrf.load(model, tables, data_path)?;
            model.set_node_max_flow(self.meta.name.as_str(), Self::mrf_sub_name(), value.into())?;
        }

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

impl TryFrom<RiverSplitWithGaugeNodeV1> for RiverSplitWithGaugeNode {
    type Error = PywrError;

    fn try_from(v1: RiverSplitWithGaugeNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let mrf = v1
            .mrf
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let mrf_cost = v1
            .mrf_cost
            .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count))
            .transpose()?;

        let n = Self {
            meta,
            mrf,
            mrf_cost,
            cost: None,
            factors: vec![],
            slot_names: vec![],
        };
        Ok(n)
    }
}

#[cfg(test)]
mod tests {

    use crate::scenario::ScenarioGroupCollection;
    use crate::schema::model::PywrModel;
    use crate::solvers::clp::{ClpSimplex, ClpSolver};
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
        let (mut model, timestepper): (crate::model::Model, Timestepper) = schema.try_into_model(None).unwrap();

        assert_eq!(model.nodes.len(), 5);
        assert_eq!(model.edges.len(), 6);

        let timestepper = default_timestepper();
        let scenarios = default_scenarios();
        let mut solver: Box<dyn Solver> = Box::new(ClpSolver::<ClpSimplex>::new());

        model.run(timestepper, scenarios, &mut solver).unwrap()

        // TODO assert the results!
    }
}
