use crate::error::ConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::{NodeAttribute, NodeMeta};
use crate::parameters::TryIntoV2Parameter;
#[cfg(feature = "core")]
use pywr_core::metric::MetricF64;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::nodes::PiecewiseLinkNode as PiecewiseLinkNodeV1;
use schemars::JsonSchema;

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, PywrVisitAll)]
pub struct PiecewiseLinkStep {
    pub max_flow: Option<Metric>,
    pub min_flow: Option<Metric>,
    pub cost: Option<Metric>,
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
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, Debug, JsonSchema, PywrVisitAll)]
pub struct PiecewiseLinkNode {
    #[serde(flatten)]
    pub meta: NodeMeta,
    pub steps: Vec<PiecewiseLinkStep>,
}

impl PiecewiseLinkNode {
    const DEFAULT_ATTRIBUTE: NodeAttribute = NodeAttribute::Outflow;

    fn step_sub_name(i: usize) -> Option<String> {
        Some(format!("step-{i:02}"))
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

    pub fn default_metric(&self) -> NodeAttribute {
        Self::DEFAULT_ATTRIBUTE
    }
}

#[cfg(feature = "core")]
impl PiecewiseLinkNode {
    pub fn add_to_model(&self, network: &mut pywr_core::network::Network) -> Result<(), SchemaError> {
        // create a link node for each step
        for (i, _) in self.steps.iter().enumerate() {
            network.add_link_node(self.meta.name.as_str(), Self::step_sub_name(i).as_deref())?;
        }
        Ok(())
    }
    pub fn set_constraints(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<(), SchemaError> {
        for (i, step) in self.steps.iter().enumerate() {
            let sub_name = Self::step_sub_name(i);

            if let Some(cost) = &step.cost {
                let value = cost.load(network, args)?;
                network.set_node_cost(self.meta.name.as_str(), sub_name.as_deref(), value.into())?;
            }

            if let Some(max_flow) = &step.max_flow {
                let value = max_flow.load(network, args)?;
                network.set_node_max_flow(self.meta.name.as_str(), sub_name.as_deref(), value.into())?;
            }

            if let Some(min_flow) = &step.min_flow {
                let value = min_flow.load(network, args)?;
                network.set_node_min_flow(self.meta.name.as_str(), sub_name.as_deref(), value.into())?;
            }
        }

        Ok(())
    }
    pub fn create_metric(
        &self,
        network: &pywr_core::network::Network,
        attribute: Option<NodeAttribute>,
    ) -> Result<MetricF64, SchemaError> {
        // Use the default attribute if none is specified
        let attr = attribute.unwrap_or(Self::DEFAULT_ATTRIBUTE);

        let indices = self
            .steps
            .iter()
            .enumerate()
            .map(|(i, _)| network.get_node_index_by_name(self.meta.name.as_str(), Self::step_sub_name(i).as_deref()))
            .collect::<Result<Vec<_>, _>>()?;

        let metric = match attr {
            NodeAttribute::Inflow => MetricF64::MultiNodeInFlow {
                indices,
                name: self.meta.name.to_string(),
            },
            NodeAttribute::Outflow => MetricF64::MultiNodeOutFlow {
                indices,
                name: self.meta.name.to_string(),
            },
            _ => {
                return Err(SchemaError::NodeAttributeNotSupported {
                    ty: "PiecewiseLinkNode".to_string(),
                    name: self.meta.name.clone(),
                    attr,
                })
            }
        };

        Ok(metric)
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
            .zip(max_flows)
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
#[cfg(feature = "core")]
mod tests {
    use crate::model::PywrModel;
    use ndarray::Array2;
    use pywr_core::{metric::MetricF64, recorders::AssertionRecorder, test_utils::run_all_solvers};

    fn model_str() -> &'static str {
        include_str!("../test_models/piecewise_link1.json")
    }

    #[test]
    fn test_model_run() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let mut model = schema.build_model(None, None).unwrap();

        let network = model.network_mut();
        assert_eq!(network.nodes().len(), 5);
        assert_eq!(network.edges().len(), 6);

        // TODO put this assertion data in the test model file.
        let idx = network.get_node_by_name("link1", Some("step-00")).unwrap().index();
        let expected = Array2::from_elem((366, 1), 1.0);
        let recorder = AssertionRecorder::new("link1-s0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        let idx = network.get_node_by_name("link1", Some("step-01")).unwrap().index();
        let expected = Array2::from_elem((366, 1), 3.0);
        let recorder = AssertionRecorder::new("link1-s0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        let idx = network.get_node_by_name("link1", Some("step-02")).unwrap().index();
        let expected = Array2::from_elem((366, 1), 0.0);
        let recorder = AssertionRecorder::new("link1-s0-flow", MetricF64::NodeOutFlow(idx), expected, None, None);
        network.add_recorder(Box::new(recorder)).unwrap();

        // Test all solvers
        run_all_solvers(&model);
    }
}
