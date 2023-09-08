use super::edge::Edge;
use super::nodes::Node;
use super::parameters::Parameter;
use crate::schema::data_tables::{DataTable, LoadedTableCollection};
use crate::schema::error::ConversionError;
use crate::schema::parameters::TryIntoV2Parameter;
use crate::PywrError;
use std::path::Path;
use time::Date;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct Metadata {
    pub title: String,
    pub description: Option<String>,
    pub minimum_version: Option<String>,
}

impl TryFrom<pywr_schema::model::Metadata> for Metadata {
    type Error = ConversionError;

    fn try_from(v1: pywr_schema::model::Metadata) -> Result<Self, Self::Error> {
        Ok(Self {
            title: v1.title,
            description: v1.description,
            minimum_version: v1.minimum_version,
        })
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
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

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct Timestepper {
    pub start: Date,
    pub end: Date,
    pub timestep: Timestep,
}

impl TryFrom<pywr_schema::model::Timestepper> for Timestepper {
    type Error = ConversionError;

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

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct Scenario {
    pub name: String,
    pub size: usize,
    pub ensemble_names: Option<Vec<String>>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
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

        if let Some(scenarios) = &self.scenarios {
            for scenario in scenarios {
                model.add_scenario_group(&scenario.name, scenario.size)?;
            }
        }

        // Load all the data tables
        let tables = LoadedTableCollection::from_schema(self.tables.as_deref(), data_path)?;

        // Create all the nodes
        let mut remaining_nodes = self.nodes.clone();

        while !remaining_nodes.is_empty() {
            let mut failed_nodes: Vec<Node> = Vec::new();
            let n = remaining_nodes.len();
            for node in remaining_nodes.into_iter() {
                if let Err(e) = node.add_to_model(&mut model, &tables, data_path) {
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

            let from_slot = edge.from_slot.as_deref();

            // Connect each "from" connector to each "to" connector
            for from_connector in from_node.output_connectors(from_slot) {
                for to_connector in to_node.input_connectors() {
                    let from_node_index =
                        model.get_node_index_by_name(from_connector.0, from_connector.1.as_deref())?;
                    let to_node_index = model.get_node_index_by_name(to_connector.0, to_connector.1.as_deref())?;
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
    type Error = ConversionError;

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
    use crate::metric::Metric;
    use crate::recorders::AssertionRecorder;
    use crate::schema::parameters::{
        AggFunc, AggregatedParameter, ConstantParameter, ConstantValue, DynamicFloatValue, MetricFloatValue, Parameter,
        ParameterMeta,
    };
    use crate::solvers::{ClpSolver, ClpSolverSettings};
    use ndarray::{Array1, Array2, Axis};
    use std::path::PathBuf;

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
                        "max_flow": {
                            "type": "Parameter",
                            "name": "demand"
                        },
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
                ],
                "parameters": [{"name": "demand", "type": "Constant", "value": 10.0}]
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

        let demand1_idx = model.get_node_index_by_name("demand1", None).unwrap();

        let expected_values: Array1<f64> = [10.0; 365].to_vec().into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        let rec = AssertionRecorder::new(
            "assert-demand1",
            Metric::NodeInFlow(demand1_idx),
            expected_values,
            None,
            None,
        );
        model.add_recorder(Box::new(rec)).unwrap();

        model
            .run::<ClpSolver>(&timestepper, &ClpSolverSettings::default())
            .unwrap()
    }

    /// Test that a cycle in parameter dependencies does not load.
    #[test]
    fn test_cycle_error() {
        let data = simple1_str();
        let mut schema: PywrModel = serde_json::from_str(data).unwrap();

        // Add additional parameters for the test
        if let Some(parameters) = &mut schema.parameters {
            parameters.extend(vec![
                Parameter::Aggregated(AggregatedParameter {
                    meta: ParameterMeta {
                        name: "agg1".to_string(),
                        comment: None,
                    },
                    agg_func: AggFunc::Sum,
                    metrics: vec![
                        DynamicFloatValue::Dynamic(MetricFloatValue::Parameter {
                            name: "p1".to_string(),
                            key: None,
                        }),
                        DynamicFloatValue::Dynamic(MetricFloatValue::Parameter {
                            name: "agg2".to_string(),
                            key: None,
                        }),
                    ],
                }),
                Parameter::Constant(ConstantParameter {
                    meta: ParameterMeta {
                        name: "p1".to_string(),
                        comment: None,
                    },
                    value: ConstantValue::Literal(10.0),
                }),
                Parameter::Aggregated(AggregatedParameter {
                    meta: ParameterMeta {
                        name: "agg2".to_string(),
                        comment: None,
                    },
                    agg_func: AggFunc::Sum,
                    metrics: vec![
                        DynamicFloatValue::Dynamic(MetricFloatValue::Parameter {
                            name: "p1".to_string(),
                            key: None,
                        }),
                        DynamicFloatValue::Dynamic(MetricFloatValue::Parameter {
                            name: "agg1".to_string(),
                            key: None,
                        }),
                    ],
                }),
            ]);
        }

        // TODO this could assert a specific type of error
        assert!(schema.try_into_model(None).is_err());
    }

    /// Test that a model loads if the aggregated parameter is defined before its dependencies.
    #[test]
    fn test_ordering() {
        let data = simple1_str();
        let mut schema: PywrModel = serde_json::from_str(data).unwrap();

        if let Some(parameters) = &mut schema.parameters {
            parameters.extend(vec![
                Parameter::Aggregated(AggregatedParameter {
                    meta: ParameterMeta {
                        name: "agg1".to_string(),
                        comment: None,
                    },
                    agg_func: AggFunc::Sum,
                    metrics: vec![
                        DynamicFloatValue::Dynamic(MetricFloatValue::Parameter {
                            name: "p1".to_string(),
                            key: None,
                        }),
                        DynamicFloatValue::Dynamic(MetricFloatValue::Parameter {
                            name: "p2".to_string(),
                            key: None,
                        }),
                    ],
                }),
                Parameter::Constant(ConstantParameter {
                    meta: ParameterMeta {
                        name: "p1".to_string(),
                        comment: None,
                    },
                    value: ConstantValue::Literal(10.0),
                }),
                Parameter::Constant(ConstantParameter {
                    meta: ParameterMeta {
                        name: "p2".to_string(),
                        comment: None,
                    },
                    value: ConstantValue::Literal(10.0),
                }),
            ]);
        }
        // TODO this could assert a specific type of error
        assert!(schema.try_into_model(None).is_ok());
    }
}
