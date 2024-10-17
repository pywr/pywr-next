use crate::data_tables::TableDataRef;
use crate::edge::Edge;
#[cfg(feature = "core")]
use crate::error::SchemaError;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::NodeAttribute;
#[cfg(feature = "core")]
use crate::nodes::NodeType;
#[cfg(feature = "core")]
use crate::parameters::ParameterType;
use crate::parameters::{Parameter, ParameterOrTimeseries, TryFromV1Parameter, TryIntoV2Parameter};
use crate::ConversionError;
#[cfg(feature = "core")]
use pywr_core::{metric::MetricF64, models::MultiNetworkTransferIndex, recorders::OutputMetric};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::ParameterValue as ParameterValueV1;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum_macros::Display;

/// Output metrics that can be recorded from a model run.
#[derive(Deserialize, Serialize, Clone, Debug, Display, JsonSchema)]
#[serde(tag = "type")]
pub enum Metric {
    Constant {
        value: f64,
    },
    Table(TableDataRef),
    /// An attribute of a node.
    Node(NodeReference),
    Edge(EdgeReference),
    Timeseries(TimeseriesReference),
    Parameter(ParameterReference),
    InlineParameter {
        definition: Box<Parameter>,
    },
    InterNetworkTransfer {
        name: String,
    },
}

impl Default for Metric {
    fn default() -> Self {
        Self::Constant { value: 0.0 }
    }
}

impl From<f64> for Metric {
    fn from(value: f64) -> Self {
        Self::Constant { value }
    }
}

#[cfg(feature = "core")]
impl Metric {
    pub fn load(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<MetricF64, SchemaError> {
        match self {
            Self::Node(node_ref) => node_ref.load(network, args),
            Self::Parameter(parameter_ref) => parameter_ref.load(network),
            Self::Constant { value } => Ok((*value).into()),
            Self::Table(table_ref) => {
                let value = args
                    .tables
                    .get_scalar_f64(table_ref)
                    .map_err(|error| SchemaError::TableRefLoad {
                        table_ref: table_ref.clone(),
                        error,
                    })?;
                Ok(value.into())
            }
            Self::Timeseries(ts_ref) => {
                let param_idx = match &ts_ref.columns {
                    Some(TimeseriesColumns::Scenario(scenario)) => {
                        args.timeseries
                            .load_df(network, ts_ref.name.as_ref(), args.domain, scenario.as_str())?
                    }
                    Some(TimeseriesColumns::Column(col)) => {
                        args.timeseries
                            .load_column(network, ts_ref.name.as_ref(), col.as_str())?
                    }
                    None => args.timeseries.load_single_column(network, ts_ref.name.as_ref())?,
                };
                Ok(param_idx.into())
            }
            Self::InlineParameter { definition } => {
                // This inline parameter could already have been loaded on a previous attempt
                // Let's see if exists first.
                // TODO this will create strange issues if there are duplicate names in the
                // parameter definitions. I.e. we will only ever load the first one and then
                // assume it is the correct one for future references to that name. This could be
                // improved by checking the parameter returned by name matches the definition here.

                match network.get_parameter_index_by_name(&definition.name().into()) {
                    Ok(p) => {
                        // Found a parameter with the name; assume it is the right one!
                        Ok(p.into())
                    }
                    Err(_) => {
                        // An error retrieving a parameter with this name; assume it needs creating.
                        match definition.add_to_model(network, args)? {
                            pywr_core::parameters::ParameterType::Parameter(idx) => Ok(idx.into()),
                            pywr_core::parameters::ParameterType::Index(idx) => Ok(idx.into()),
                            pywr_core::parameters::ParameterType::Multi(_) => Err(SchemaError::UnexpectedParameterType(format!(
                                "Found an inline definition of a multi valued parameter of type '{}' with name '{}' where a float parameter was expected. Multi valued parameters cannot be defined inline.",
                                definition.parameter_type(),
                                definition.name(),
                            ))),
                        }
                    }
                }
            }

            Self::InterNetworkTransfer { name } => {
                // Find the matching inter model transfer
                match args.inter_network_transfers.iter().position(|t| &t.name == name) {
                    Some(idx) => Ok(MetricF64::InterNetworkTransfer(MultiNetworkTransferIndex(idx))),
                    None => Err(SchemaError::InterNetworkTransferNotFound(name.to_string())),
                }
            }
            Self::Edge(edge_ref) => edge_ref.load(network, args),
        }
    }

    fn name(&self) -> Result<String, SchemaError> {
        match self {
            Self::Node(node_ref) => Ok(node_ref.name.to_string()),
            Self::Parameter(parameter_ref) => Ok(parameter_ref.name.clone()),
            Self::Constant { .. } => Err(SchemaError::LiteralConstantOutputNotSupported),
            Self::Table(table_ref) => Ok(table_ref.table.clone()),
            Self::Timeseries(ts_ref) => Ok(ts_ref.name.clone()),
            Self::InlineParameter { definition } => Ok(definition.name().to_string()),
            Self::InterNetworkTransfer { name } => Ok(name.clone()),
            Self::Edge(edge_ref) => Ok(edge_ref.edge.to_string()),
        }
    }

    fn attribute(&self, args: &LoadArgs) -> Result<String, SchemaError> {
        let attribute = match self {
            Self::Node(node_ref) => node_ref.attribute(args)?.to_string(),
            Self::Parameter(_) => "value".to_string(),
            Self::Constant { .. } => "value".to_string(),
            Self::Table(_) => "value".to_string(),
            Self::Timeseries(_) => "value".to_string(),
            Self::InlineParameter { .. } => "value".to_string(),
            Self::InterNetworkTransfer { .. } => "value".to_string(),
            Self::Edge { .. } => "Flow".to_string(),
        };

        Ok(attribute)
    }

    /// Return the subtype of the metric. This is the type of the metric that is being
    /// referenced. For example, if the metric is a node then the subtype is the type of the
    /// node.

    fn sub_type(&self, args: &LoadArgs) -> Result<Option<String>, SchemaError> {
        let sub_type = match self {
            Self::Node(node_ref) => Some(node_ref.node_type(args)?.to_string()),
            Self::Parameter(parameter_ref) => Some(parameter_ref.parameter_type(args)?.to_string()),
            Self::Constant { .. } => None,
            Self::Table(_) => None,
            Self::Timeseries(_) => None,
            Self::InlineParameter { definition } => Some(definition.parameter_type().to_string()),
            Self::InterNetworkTransfer { .. } => None,
            Self::Edge { .. } => None,
        };

        Ok(sub_type)
    }

    pub fn load_as_output(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<OutputMetric, SchemaError> {
        let metric = self.load(network, args)?;

        let ty = self.to_string();
        let sub_type = self.sub_type(args)?;

        Ok(OutputMetric::new(
            self.name()?.as_str(),
            &self.attribute(args)?,
            &ty,
            sub_type.as_deref(),
            metric,
        ))
    }
}

impl TryFromV1Parameter<ParameterValueV1> for Metric {
    type Error = ConversionError;

    fn try_from_v1_parameter(
        v1: ParameterValueV1,
        parent_node: Option<&str>,
        unnamed_count: &mut usize,
    ) -> Result<Self, Self::Error> {
        let p = match v1 {
            ParameterValueV1::Constant(value) => Self::Constant { value },
            ParameterValueV1::Reference(p_name) => Self::Parameter(ParameterReference {
                name: p_name,
                key: None,
            }),
            ParameterValueV1::Table(tbl) => Self::Table(tbl.try_into()?),
            ParameterValueV1::Inline(param) => {
                let definition: ParameterOrTimeseries = (*param).try_into_v2_parameter(parent_node, unnamed_count)?;
                match definition {
                    ParameterOrTimeseries::Parameter(p) => Self::InlineParameter {
                        definition: Box::new(p),
                    },
                    ParameterOrTimeseries::Timeseries(t) => {
                        let name = match t.name {
                            Some(n) => n,
                            None => {
                                let n = match parent_node {
                                    Some(node_name) => format!("{}-p{}.timeseries", node_name, *unnamed_count),
                                    None => format!("unnamed-timeseries-{}", *unnamed_count),
                                };
                                *unnamed_count += 1;
                                n
                            }
                        };

                        let cols = match (&t.column, &t.scenario) {
                            (Some(col), None) => Some(TimeseriesColumns::Column(col.clone())),
                            (None, Some(scenario)) => Some(TimeseriesColumns::Scenario(scenario.clone())),
                            (Some(_), Some(_)) => {
                                return Err(ConversionError::AmbiguousColumnAndScenario(name.clone()))
                            }
                            (None, None) => None,
                        };

                        Self::Timeseries(TimeseriesReference::new(name, cols))
                    }
                }
            }
        };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, strum_macros::Display)]
#[serde(tag = "type", content = "name")]
pub enum TimeseriesColumns {
    Scenario(String),
    Column(String),
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimeseriesReference {
    name: String,
    columns: Option<TimeseriesColumns>,
}

impl TimeseriesReference {
    pub fn new(name: String, columns: Option<TimeseriesColumns>) -> Self {
        Self { name, columns }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }
}

/// A reference to a node with an optional attribute.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(deny_unknown_fields)]
pub struct NodeReference {
    /// The name of the node
    pub name: String,
    /// The attribute of the node. If this is `None` then the default attribute is used.
    pub attribute: Option<NodeAttribute>,
}

impl NodeReference {
    pub fn new(name: String, attribute: Option<NodeAttribute>) -> Self {
        Self { name, attribute }
    }

    #[cfg(feature = "core")]
    pub fn load(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<MetricF64, SchemaError> {
        // This is the associated node in the schema
        let node = args
            .schema
            .get_node_by_name(&self.name)
            .ok_or_else(|| SchemaError::NodeNotFound(self.name.clone()))?;

        node.create_metric(network, self.attribute, args)
    }

    /// Return the attribute of the node. If the attribute is not specified then the default
    /// attribute of the node is returned. Note that this does not check if the attribute is
    /// valid for the node.
    #[cfg(feature = "core")]
    pub fn attribute(&self, args: &LoadArgs) -> Result<NodeAttribute, SchemaError> {
        // This is the associated node in the schema
        let node = args
            .schema
            .get_node_by_name(&self.name)
            .ok_or_else(|| SchemaError::NodeNotFound(self.name.clone()))?;

        Ok(self.attribute.unwrap_or_else(|| node.default_metric()))
    }

    #[cfg(feature = "core")]
    pub fn node_type(&self, args: &LoadArgs) -> Result<NodeType, SchemaError> {
        // This is the associated node in the schema
        let node = args
            .schema
            .get_node_by_name(&self.name)
            .ok_or_else(|| SchemaError::NodeNotFound(self.name.clone()))?;

        Ok(node.node_type())
    }
}

/// A reference to a node without an attribute.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
pub struct SimpleNodeReference {
    /// The name of the node
    pub name: String,
}

impl SimpleNodeReference {
    pub fn new(name: String) -> Self {
        Self { name }
    }

    #[cfg(feature = "core")]
    pub fn load(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<MetricF64, SchemaError> {
        // This is the associated node in the schema
        let node = args
            .schema
            .get_node_by_name(&self.name)
            .ok_or_else(|| SchemaError::NodeNotFound(self.name.clone()))?;

        node.create_metric(network, None, args)
    }

    /// Return the default attribute of the node.
    #[cfg(feature = "core")]
    pub fn attribute(&self, args: &LoadArgs) -> Result<NodeAttribute, SchemaError> {
        // This is the associated node in the schema
        let node = args
            .schema
            .get_node_by_name(&self.name)
            .ok_or_else(|| SchemaError::NodeNotFound(self.name.clone()))?;

        Ok(node.default_metric())
    }

    #[cfg(feature = "core")]
    pub fn node_type(&self, args: &LoadArgs) -> Result<NodeType, SchemaError> {
        // This is the associated node in the schema
        let node = args
            .schema
            .get_node_by_name(&self.name)
            .ok_or_else(|| SchemaError::NodeNotFound(self.name.clone()))?;

        Ok(node.node_type())
    }
}

impl From<String> for SimpleNodeReference {
    fn from(v: String) -> Self {
        SimpleNodeReference { name: v }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ParameterReference {
    /// The name of the parameter
    pub name: String,
    /// The key of the parameter. If this is `None` then the default value is used.
    pub key: Option<String>,
}

impl ParameterReference {
    pub fn new(name: String, key: Option<String>) -> Self {
        Self { name, key }
    }

    #[cfg(feature = "core")]
    pub fn load(&self, network: &mut pywr_core::network::Network) -> Result<MetricF64, SchemaError> {
        let name = self.name.as_str().into();

        match &self.key {
            Some(key) => {
                // Key given; this should be a multi-valued parameter
                Ok((network.get_multi_valued_parameter_index_by_name(&name)?, key.clone()).into())
            }
            None => {
                if let Ok(idx) = network.get_parameter_index_by_name(&name) {
                    Ok(idx.into())
                } else if let Ok(idx) = network.get_index_parameter_index_by_name(&name) {
                    Ok(idx.into())
                } else {
                    Err(SchemaError::ParameterNotFound(self.name.to_string()))
                }
            }
        }
    }

    #[cfg(feature = "core")]
    pub fn parameter_type(&self, args: &LoadArgs) -> Result<ParameterType, SchemaError> {
        let parameter = args
            .schema
            .get_parameter_by_name(&self.name)
            .ok_or_else(|| SchemaError::ParameterNotFound(self.name.clone()))?;

        Ok(parameter.parameter_type())
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EdgeReference {
    /// The edge referred to by this reference.
    pub edge: Edge,
}

#[cfg(feature = "core")]
impl EdgeReference {
    pub fn load(&self, network: &mut pywr_core::network::Network, args: &LoadArgs) -> Result<MetricF64, SchemaError> {
        // This is the associated node in the schema
        self.edge.create_metric(network, args)
    }
}
