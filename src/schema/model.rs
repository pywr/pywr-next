use super::edge::Edge;
use super::nodes::Node;
use super::parameters::Parameter;
use crate::{NodeIndex, PywrError};
use std::collections::HashMap;
use time::Date;

#[derive(serde::Deserialize)]
pub struct Metadata {
    pub title: String,
    pub description: Option<String>,
    pub minimum_version: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
pub enum Timestep {
    Days(i64),
    Frequency(String),
}

#[derive(serde::Deserialize)]
pub struct Timestepper {
    pub start: Date,
    pub end: Date,
    pub timestep: Timestep,
}

impl From<Timestepper> for crate::timestep::Timestepper {
    fn from(ts: Timestepper) -> Self {
        let timestep = match ts.timestep {
            Timestep::Days(d) => d,
            _ => todo!(),
        };

        Self::new(ts.start, ts.end, timestep)
    }
}

#[derive(serde::Deserialize)]
pub struct Scenario {
    pub name: String,
    pub size: usize,
    pub ensemble_names: Option<Vec<String>>,
}

#[derive(serde::Deserialize)]
pub struct PywrModel {
    pub metadata: Metadata,
    pub timestepper: Timestepper,
    pub scenarios: Option<Vec<Scenario>>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub parameters: Option<Vec<Parameter>>,
}

impl PywrModel {
    pub fn get_node_by_name(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.name() == name)
    }

    pub fn get_node_index_by_name(&self, name: &str) -> Option<usize> {
        self.nodes
            .iter()
            .enumerate()
            .find_map(|(idx, n)| (n.name() == name).then(|| idx))
    }

    pub fn get_node(&self, idx: usize) -> Option<&Node> {
        self.nodes.get(idx)
    }

    pub fn get_parameter_by_name(&self, name: &str) -> Option<&Parameter> {
        match &self.parameters {
            Some(parameters) => parameters.iter().find(|p| p.name() == name),
            None => None,
        }
    }
}

/// Construct a model from its schema
impl TryFrom<PywrModel> for crate::model::Model {
    type Error = PywrError;

    fn try_from(schema: PywrModel) -> Result<Self, Self::Error> {
        let mut model = crate::model::Model::new();

        // Create all the nodes
        for node in &schema.nodes {
            let _ = node.add_to_model(&mut model)?;
        }

        // Create the edges
        for edge in &schema.edges {
            let from_node = schema
                .get_node_by_name(edge.from_node.as_str())
                .ok_or_else(|| PywrError::NodeNotFound(edge.from_node.clone()))?;
            let to_node = schema
                .get_node_by_name(edge.to_node.as_str())
                .ok_or_else(|| PywrError::NodeNotFound(edge.to_node.clone()))?;

            // Connect each "from" connector to each "to" connector
            for from_connector in from_node.output_connectors() {
                for to_connector in to_node.input_connectors() {
                    let from_node_index = model.get_node_index_by_name(from_connector.0, from_connector.1)?;
                    let to_node_index = model.get_node_index_by_name(to_connector.0, to_connector.1)?;
                    model.connect_nodes(from_node_index, to_node_index)?;
                }
            }
        }

        // Build the parameters
        // if let Some(remaining_parameters) = schema.parameters {
        //     while remaining_parameters.len() > 0 {
        //         let mut failed_parameters: Vec<Parameter> = Vec::new();
        //
        //         for parameter in remaining_parameters {
        //             match parameter.add_to_model(model) {}
        //         }
        //     }
        // }

        Ok(model)
    }
}

#[cfg(test)]
mod tests {
    use super::PywrModel;
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

    fn simple1_str() -> &'static str {
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
                        "name": "supply1",
                        "type": "Input",
                        "max_flow": 15
                    },
                    {
                        "name": "link1",
                        "type": "Link"
                    },
                    {
                        "name": "demand1",
                        "type": "Output",
                        "max_flow": 10,
                        "cost": -10
                    }
                ],
                "edges": [
                    {
                        "from_node": "supply1",
                        "to_node": "link1"
                    },
                    {
                        "from_node": "link1",
                        "to_node": "demand1"
                    }
                ]
            }
            "#
    }

    #[test]
    fn test_simple1_schema() {
        let data = simple1_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();

        assert_eq!(schema.nodes.len(), 3);
        assert_eq!(schema.edges.len(), 2);
    }

    #[test]
    fn test_simple1_run() {
        let data = simple1_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let mut model: crate::model::Model = schema.try_into().unwrap();

        assert_eq!(model.nodes.len(), 3);
        assert_eq!(model.edges.len(), 2);

        let timestepper = default_timestepper();
        let scenarios = default_scenarios();
        let mut solver: Box<dyn Solver> = Box::new(ClpSolver::new());

        model.run(timestepper, scenarios, &mut solver).unwrap()

        // TODO assert the results!
    }
}
