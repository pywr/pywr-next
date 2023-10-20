use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::nodes::NodeMeta;
use crate::parameters::{DynamicFloatValue, TryIntoV2Parameter};
use pywr_core::metric::Metric;
use pywr_v1_schema::nodes::PiecewiseLinkNode as PiecewiseLinkNodeV1;
use std::path::Path;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct PiecewiseLinkStep {
    pub max_flow: Option<DynamicFloatValue>,
    pub min_flow: Option<DynamicFloatValue>,
    pub cost: Option<DynamicFloatValue>,
}

#[doc = svgbobdoc::transform!(
/// This node is used to create a sequence of link nodes with separate costs and constraints.
///
/// Typically this node is used to model an non-linear cost by providing increasing cost
/// values at different flows limits.
///
/// ```svgbob
///
///            <node>.00    D
///          .------>L ---.
///      U  |             |         D
///     -*--|             |-------->*-
///         |  <node>.01  |
///          '------>L --'
///         :             :
///         :             :
///         :  <node>.n   :
///          '~~~~~~>L ~~'
///
/// ```
///
)]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default)]
pub struct PiecewiseLinkNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub steps: Vec<PiecewiseLinkStep>,
}

impl PiecewiseLinkNode {
    fn step_sub_name(i: usize) -> Option<String> {
        Some(format!("step-{i:02}"))
    }

    pub fn add_to_model(&self, model: &mut pywr_core::model::Model) -> Result<(), SchemaError> {
        // create a link node for each step
        for (i, _) in self.steps.iter().enumerate() {
            model.add_link_node(self.meta.name.as_str(), Self::step_sub_name(i).as_deref())?;
        }
        Ok(())
    }

    pub fn set_constraints(
        &self,
        model: &mut pywr_core::model::Model,
        tables: &LoadedTableCollection,
        data_path: Option<&Path>,
    ) -> Result<(), SchemaError> {
        for (i, step) in self.steps.iter().enumerate() {
            let sub_name = Self::step_sub_name(i);

            if let Some(cost) = &step.cost {
                let value = cost.load(model, tables, data_path)?;
                model.set_node_cost(self.meta.name.as_str(), sub_name.as_deref(), value.into())?;
            }

            if let Some(max_flow) = &step.max_flow {
                let value = max_flow.load(model, tables, data_path)?;
                model.set_node_max_flow(self.meta.name.as_str(), sub_name.as_deref(), value.into())?;
            }

            if let Some(min_flow) = &step.min_flow {
                let value = min_flow.load(model, tables, data_path)?;
                model.set_node_min_flow(self.meta.name.as_str(), sub_name.as_deref(), value.into())?;
            }
        }

        Ok(())
    }

    pub fn input_connectors(&self) -> Vec<(&str, Option<String>)> {
        self.steps
            .iter()
            .enumerate()
            .map(|(i, _)| (self.meta.name.as_str(), Self::step_sub_name(i)))
            .collect()
    }
    pub fn output_connectors(&self) -> Vec<(&str, Option<String>)> {
        self.steps
            .iter()
            .enumerate()
            .map(|(i, _)| (self.meta.name.as_str(), Self::step_sub_name(i)))
            .collect()
    }

    pub fn default_metric(&self, model: &pywr_core::model::Model) -> Result<Metric, SchemaError> {
        let indices = self
            .steps
            .iter()
            .enumerate()
            .map(|(i, _)| model.get_node_index_by_name(self.meta.name.as_str(), Self::step_sub_name(i).as_deref()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Metric::MultiNodeInFlow {
            indices,
            name: self.meta.name.to_string(),
            sub_name: Some("total".to_string()),
        })
    }
}

impl TryFrom<PiecewiseLinkNodeV1> for PiecewiseLinkNode {
    type Error = ConversionError;

    fn try_from(v1: PiecewiseLinkNodeV1) -> Result<Self, Self::Error> {
        let meta: NodeMeta = v1.meta.into();
        let mut unnamed_count = 0;

        let costs = match v1.costs {
            None => vec![None; v1.nsteps],
            Some(v1_costs) => v1_costs
                .into_iter()
                .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count).map(Some))
                .collect::<Result<Vec<_>, _>>()?,
        };

        let max_flows = match v1.max_flows {
            None => vec![None; v1.nsteps],
            Some(v1_max_flows) => v1_max_flows
                .into_iter()
                .map(|v| v.try_into_v2_parameter(Some(&meta.name), &mut unnamed_count).map(Some))
                .collect::<Result<Vec<_>, _>>()?,
        };

        let steps = costs
            .into_iter()
            .zip(max_flows.into_iter())
            .map(|(cost, max_flow)| PiecewiseLinkStep {
                max_flow,
                min_flow: None,
                cost,
            })
            .collect::<Vec<_>>();

        let n = Self { meta, steps };
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use crate::model::PywrModel;
    use ndarray::Array2;
    use pywr_core::metric::Metric;
    use pywr_core::recorders::AssertionRecorder;
    use pywr_core::test_utils::run_all_solvers;
    use pywr_core::timestep::Timestepper;

    fn model_str() -> &'static str {
        include_str!("../test_models/piecewise_link1.json")
    }

    #[test]
    fn test_model_run() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let (mut model, timestepper): (pywr_core::model::Model, Timestepper) = schema.build_model(None, None).unwrap();

        assert_eq!(model.nodes.len(), 5);
        assert_eq!(model.edges.len(), 6);

        // TODO put this assertion data in the test model file.
        let idx = model.get_node_by_name("link1", Some("step-00")).unwrap().index();
        let expected = Array2::from_elem((366, 1), 1.0);
        let recorder = AssertionRecorder::new("link1-s0-flow", Metric::NodeOutFlow(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("link1", Some("step-01")).unwrap().index();
        let expected = Array2::from_elem((366, 1), 3.0);
        let recorder = AssertionRecorder::new("link1-s0-flow", Metric::NodeOutFlow(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        let idx = model.get_node_by_name("link1", Some("step-02")).unwrap().index();
        let expected = Array2::from_elem((366, 1), 0.0);
        let recorder = AssertionRecorder::new("link1-s0-flow", Metric::NodeOutFlow(idx), expected, None, None);
        model.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model, &timestepper);
    }
}