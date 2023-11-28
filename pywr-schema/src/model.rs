use super::edge::Edge;
use super::nodes::Node;
use super::parameters::Parameter;
use crate::data_tables::{DataTable, LoadedTableCollection};
use crate::error::{ConversionError, SchemaError};
use crate::metric_sets::MetricSet;
use crate::outputs::Output;
use crate::parameters::TryIntoV2Parameter;
use pywr_core::PywrError;
use std::path::Path;
use time::Date;

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct Metadata {
    pub title: String,
    pub description: Option<String>,
    pub minimum_version: Option<String>,
}

impl TryFrom<pywr_v1_schema::model::Metadata> for Metadata {
    type Error = ConversionError;

    fn try_from(v1: pywr_v1_schema::model::Metadata) -> Result<Self, Self::Error> {
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

impl From<pywr_v1_schema::model::Timestep> for Timestep {
    fn from(v1: pywr_v1_schema::model::Timestep) -> Self {
        match v1 {
            pywr_v1_schema::model::Timestep::Days(d) => Self::Days(d as i64),
            pywr_v1_schema::model::Timestep::Frequency(f) => Self::Frequency(f),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct Timestepper {
    pub start: Date,
    pub end: Date,
    pub timestep: Timestep,
}

impl TryFrom<pywr_v1_schema::model::Timestepper> for Timestepper {
    type Error = ConversionError;

    fn try_from(v1: pywr_v1_schema::model::Timestepper) -> Result<Self, Self::Error> {
        Ok(Self {
            start: v1.start,
            end: v1.end,
            timestep: v1.timestep.into(),
        })
    }
}

impl From<Timestepper> for pywr_core::timestep::Timestepper {
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
    pub metric_sets: Option<Vec<MetricSet>>,
    pub outputs: Option<Vec<Output>>,
}

impl PywrModel {
    pub fn new(title: &str, start: &Date, end: &Date) -> Self {
        Self {
            metadata: Metadata {
                title: title.to_string(),
                description: None,
                minimum_version: None,
            },
            timestepper: Timestepper {
                start: *start,
                end: *end,
                timestep: Timestep::Days(1),
            },
            scenarios: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            parameters: None,
            tables: None,
            metric_sets: None,
            outputs: None,
        }
    }
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, SchemaError> {
        let data = std::fs::read_to_string(path).map_err(|e| SchemaError::IO(e.to_string()))?;
        Ok(serde_json::from_str(data.as_str())?)
    }

    pub fn from_str(data: &str) -> Result<Self, SchemaError> {
        Ok(serde_json::from_str(data)?)
    }

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

    pub fn build_model(
        &self,
        data_path: Option<&Path>,
        output_path: Option<&Path>,
    ) -> Result<(pywr_core::model::Model, pywr_core::timestep::Timestepper), SchemaError> {
        let mut model = pywr_core::model::Model::default();

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
                        SchemaError::PywrCore(core_err) => match core_err {
                            // And it failed because another node was not found.
                            // Let's try to load more nodes and see if this one can tried
                            // again later
                            PywrError::NodeNotFound(_) => failed_nodes.push(node),
                            _ => return Err(SchemaError::PywrCore(core_err)),
                        },
                        _ => return Err(e),
                    }
                };
            }

            if failed_nodes.len() == n {
                // Could not load any nodes; must be a circular reference
                return Err(SchemaError::CircularNodeReference);
            }

            remaining_nodes = failed_nodes;
        }

        // Create the edges
        for edge in &self.edges {
            let from_node = self
                .get_node_by_name(edge.from_node.as_str())
                .ok_or_else(|| SchemaError::NodeNotFound(edge.from_node.clone()))?;
            let to_node = self
                .get_node_by_name(edge.to_node.as_str())
                .ok_or_else(|| SchemaError::NodeNotFound(edge.to_node.clone()))?;

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
                        // Adding the parameter failed!
                        match e {
                            SchemaError::PywrCore(core_err) => match core_err {
                                // And it failed because another parameter was not found.
                                // Let's try to load more parameters and see if this one can tried
                                // again later
                                PywrError::ParameterNotFound(_) => failed_parameters.push(parameter),
                                _ => return Err(SchemaError::PywrCore(core_err)),
                            },
                            _ => return Err(e),
                        }
                    };
                }

                if failed_parameters.len() == n {
                    // Could not load any parameters; must be a circular reference
                    return Err(SchemaError::CircularParameterReference);
                }

                remaining_parameters = failed_parameters;
            }
        }

        // Apply the inline parameters & constraints to the nodes
        for node in &self.nodes {
            node.set_constraints(&mut model, &tables, data_path)?;
        }

        // Create all of the metric sets
        if let Some(metric_sets) = &self.metric_sets {
            for metric_set in metric_sets {
                metric_set.add_to_model(&mut model, self)?;
            }
        }

        // Create all of the outputs
        if let Some(outputs) = &self.outputs {
            for output in outputs {
                output.add_to_model(&mut model, output_path)?;
            }
        }

        let timestepper = self.timestepper.clone().into();

        Ok((model, timestepper))
    }
}

impl TryFrom<pywr_v1_schema::PywrModel> for PywrModel {
    type Error = ConversionError;

    fn try_from(v1: pywr_v1_schema::PywrModel) -> Result<Self, Self::Error> {
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
        let outputs = None;
        let metric_sets = None;

        Ok(Self {
            metadata,
            timestepper,
            scenarios: None,
            nodes,
            edges,
            parameters,
            tables,
            metric_sets,
            outputs,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::PywrModel;
    use crate::parameters::{
        AggFunc, AggregatedParameter, ConstantParameter, ConstantValue, DynamicFloatValue, MetricFloatValue, Parameter,
        ParameterMeta,
    };
    use ndarray::{Array1, Array2, Axis};
    use pywr_core::metric::Metric;
    use pywr_core::recorders::AssertionRecorder;
    use pywr_core::test_utils::run_all_solvers;

    fn model_str() -> &'static str {
        include_str!("./test_models/simple1.json")
    }

    #[test]
    fn test_simple1_schema() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();

        assert_eq!(schema.nodes.len(), 3);
        assert_eq!(schema.edges.len(), 2);
    }

    #[test]
    fn test_simple1_run() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let (mut model, timestepper) = schema.build_model(None, None).unwrap();

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

        // Test all solvers
        run_all_solvers(&model, &timestepper);
    }

    /// Test that a cycle in parameter dependencies does not load.
    #[test]
    fn test_cycle_error() {
        let data = model_str();
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
                    variable: None,
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
        assert!(schema.build_model(None, None).is_err());
    }

    /// Test that a model loads if the aggregated parameter is defined before its dependencies.
    #[test]
    fn test_ordering() {
        let data = model_str();
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
                    variable: None,
                }),
                Parameter::Constant(ConstantParameter {
                    meta: ParameterMeta {
                        name: "p2".to_string(),
                        comment: None,
                    },
                    value: ConstantValue::Literal(10.0),
                    variable: None,
                }),
            ]);
        }
        // TODO this could assert a specific type of error
        let build_result = schema.build_model(None, None);
        assert!(build_result.is_ok());
    }
}
