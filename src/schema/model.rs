use super::edge::Edge;
use super::nodes::Node;
use super::parameters::Parameter;
use crate::schema::data_tables::{DataTable, LoadedTableCollection};
use crate::schema::parameters::TryIntoV2Parameter;
use crate::PywrError;
use std::path::Path;
use time::Date;

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Metadata {
    pub title: String,
    pub description: Option<String>,
    pub minimum_version: Option<String>,
}

impl TryFrom<pywr_schema::model::Metadata> for Metadata {
    type Error = PywrError;

    fn try_from(v1: pywr_schema::model::Metadata) -> Result<Self, Self::Error> {
        Ok(Self {
            title: v1.title,
            description: v1.description,
            minimum_version: v1.minimum_version,
        })
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum Timestep {
    Days(i64),
    Frequency(String),
}

impl From<pywr_schema::model::Timestep> for Timestep {
    fn from(v1: pywr_schema::model::Timestep) -> Self {
        match v1 {
            pywr_schema::model::Timestep::Days(d) => Self::Days(d as i64),
            pywr_schema::model::Timestep::Frequency(f) => Self::Frequency(f),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Timestepper {
    pub start: Date,
    pub end: Date,
    pub timestep: Timestep,
}

impl TryFrom<pywr_schema::model::Timestepper> for Timestepper {
    type Error = PywrError;

    fn try_from(v1: pywr_schema::model::Timestepper) -> Result<Self, Self::Error> {
        Ok(Self {
            start: v1.start,
            end: v1.end,
            timestep: v1.timestep.into(),
        })
    }
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

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Scenario {
    pub name: String,
    pub size: usize,
    pub ensemble_names: Option<Vec<String>>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct PywrModel {
    pub metadata: Metadata,
    pub timestepper: Timestepper,
    pub scenarios: Option<Vec<Scenario>>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub parameters: Option<Vec<Parameter>>,
    pub tables: Option<Vec<DataTable>>,
}

impl PywrModel {
    pub fn get_node_by_name(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.name() == name)
    }

    pub fn get_node_index_by_name(&self, name: &str) -> Option<usize> {
        self.nodes
            .iter()
            .enumerate()
            .find_map(|(idx, n)| (n.name() == name).then_some(idx))
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

    pub fn try_into_model(
        self,
        data_path: Option<&Path>,
    ) -> Result<(crate::model::Model, crate::timestep::Timestepper), PywrError> {
        let mut model = crate::model::Model::default();

        // Load all the data tables
        let tables = LoadedTableCollection::from_schema(&self.tables, data_path)?;

        // Create all the nodes
        let mut remaining_nodes = self.nodes.clone();

        while !remaining_nodes.is_empty() {
            let mut failed_nodes: Vec<Node> = Vec::new();
            let n = remaining_nodes.len();
            for node in remaining_nodes.into_iter() {
                if let Err(e) = node.add_to_model(&mut model, &tables) {
                    // Adding the node failed!
                    match e {
                        PywrError::NodeNotFound(_) => failed_nodes.push(node),
                        _ => return Err(e),
                    }
                };
            }

            if failed_nodes.len() == n {
                // Could not load any nodes; must be a circular reference
                return Err(PywrError::SchemaLoad(
                    "Circular reference in node definitions.".to_string(),
                ));
            }

            remaining_nodes = failed_nodes;
        }

        // Create the edges
        for edge in &self.edges {
            let from_node = self
                .get_node_by_name(edge.from_node.as_str())
                .ok_or_else(|| PywrError::NodeNotFound(edge.from_node.clone()))?;
            let to_node = self
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

        // Create all the parameters
        if let Some(mut remaining_parameters) = self.parameters.clone() {
            while !remaining_parameters.is_empty() {
                let mut failed_parameters: Vec<Parameter> = Vec::new();
                let n = remaining_parameters.len();
                for parameter in remaining_parameters.into_iter() {
                    if let Err(e) = parameter.add_to_model(&mut model, &tables, data_path) {
                        // Adding the node failed!
                        match e {
                            PywrError::ParameterNotFound(_) => failed_parameters.push(parameter),
                            _ => return Err(e),
                        }
                    };
                }

                if failed_parameters.len() == n {
                    // Could not load any nodes; must be a circular reference
                    return Err(PywrError::SchemaLoad(
                        "Circular reference in parameter definitions.".to_string(),
                    ));
                }

                remaining_parameters = failed_parameters;
            }
        }

        // Apply the inline parameters & constraints to the nodes
        for node in self.nodes {
            node.set_constraints(&mut model, &tables, data_path)?;
        }

        let timestepper = self.timestepper.into();

        Ok((model, timestepper))
    }
}

impl TryFrom<pywr_schema::PywrModel> for PywrModel {
    type Error = PywrError;

    fn try_from(v1: pywr_schema::PywrModel) -> Result<Self, Self::Error> {
        let metadata = v1.metadata.try_into()?;
        let timestepper = v1.timestepper.try_into()?;

        let nodes = v1
            .nodes
            .into_iter()
            .map(|n| n.try_into())
            .collect::<Result<Vec<_>, _>>()?;

        let edges = v1.edges.into_iter().map(|e| e.into()).collect();

        let parameters = if let Some(v1_parameters) = v1.parameters {
            let mut unnamed_count: usize = 0;
            Some(
                v1_parameters
                    .into_iter()
                    .map(|p| p.try_into_v2_parameter(None, &mut unnamed_count))
                    .collect::<Result<Vec<_>, _>>()?,
            )
        } else {
            None
        };

        // TODO convert v1 tables!
        let tables = None;

        Ok(Self {
            metadata,
            timestepper,
            scenarios: None,
            nodes,
            edges,
            parameters,
            tables,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::PywrModel;
    use crate::scenario::ScenarioGroupCollection;
    use crate::solvers::clp::ClpSolver;
    use crate::timestep::Timestepper;
    use time::macros::date;

    fn default_timestepper() -> Timestepper {
        Timestepper::new(date!(2020 - 01 - 01), date!(2020 - 01 - 15), 1)
    }

    fn default_scenarios() -> ScenarioGroupCollection {
        let mut scenarios = ScenarioGroupCollection::default();
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
        let (mut model, timestepper): (crate::model::Model, crate::timestep::Timestepper) =
            schema.try_into_model(None).unwrap();

        assert_eq!(model.nodes.len(), 3);
        assert_eq!(model.edges.len(), 2);

        let scenarios = default_scenarios();

        model.run::<ClpSolver>(&timestepper, &scenarios).unwrap()

        // TODO assert the results!
    }
}
