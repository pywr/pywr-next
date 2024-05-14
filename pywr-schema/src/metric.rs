use crate::data_tables::TableDataRef;
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
            Self::Constant { value } => Ok(MetricF64::Constant(*value)),
            Self::Table(table_ref) => {
                let value = args.tables.get_scalar_f64(table_ref)?;
                Ok(MetricF64::Constant(value))
            }
            Self::Timeseries(ts_ref) => {
                let param_idx = match &ts_ref.columns {
                    TimeseriesColumns::Scenario(scenario) => {
                        args.timeseries
                            .load_df(network, ts_ref.name.as_ref(), args.domain, scenario.as_str())?
                    }
                    TimeseriesColumns::Column(col) => {
                        args.timeseries
                            .load_column(network, ts_ref.name.as_ref(), col.as_str())?
                    }
                };
                Ok(MetricF64::ParameterValue(param_idx))
            }
            Self::InlineParameter { definition } => {
                // This inline parameter could already have been loaded on a previous attempt
                // Let's see if exists first.
                // TODO this will create strange issues if there are duplicate names in the
                // parameter definitions. I.e. we will only ever load the first one and then
                // assume it is the correct one for future references to that name. This could be
                // improved by checking the parameter returned by name matches the definition here.

                match network.get_parameter_index_by_name(definition.name()) {
                    Ok(p) => {
                        // Found a parameter with the name; assume it is the right one!
                        Ok(MetricF64::ParameterValue(p))
                    }
                    Err(_) => {
                        // An error retrieving a parameter with this name; assume it needs creating.
                        match definition.add_to_model(network, args)? {
                            pywr_core::parameters::ParameterType::Parameter(idx) => Ok(MetricF64::ParameterValue(idx)),
                            pywr_core::parameters::ParameterType::Index(idx) => Ok(MetricF64::IndexParameterValue(idx)),
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
        }
    }

    fn name(&self) -> Result<&str, SchemaError> {
        match self {
            Self::Node(node_ref) => Ok(&node_ref.name),
            Self::Parameter(parameter_ref) => Ok(&parameter_ref.name),
            Self::Constant { .. } => Err(SchemaError::LiteralConstantOutputNotSupported),
            Self::Table(table_ref) => Ok(&table_ref.table),
            Self::Timeseries(ts_ref) => Ok(&ts_ref.name),
            Self::InlineParameter { definition } => Ok(definition.name()),
            Self::InterNetworkTransfer { name } => Ok(name),
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
            self.name()?,
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
                            (Some(col), None) => TimeseriesColumns::Column(col.clone()),
                            (None, Some(scenario)) => TimeseriesColumns::Scenario(scenario.clone()),
                            (Some(_), Some(_)) => {
                                return Err(ConversionError::AmbiguousColumnAndScenario(name.clone()))
                            }
                            (None, None) => return Err(ConversionError::MissingColumnOrScenario(name.clone())),
                        };

                        Self::Timeseries(TimeseriesReference::new(name, cols))
                    }
                }
            }
        };
        Ok(p)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(tag = "type", content = "name")]
pub enum TimeseriesColumns {
    Scenario(String),
    Column(String),
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
pub struct TimeseriesReference {
    name: String,
    columns: TimeseriesColumns,
}

impl TimeseriesReference {
    pub fn new(name: String, columns: TimeseriesColumns) -> Self {
        Self { name, columns }
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
pub struct NodeReference {
    /// The name of the node
    pub name: String,
    /// The attribute of the node. If this is `None` then the default attribute is used.
    pub attribute: Option<NodeAttribute>,
}

impl NodeReference {
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

impl From<String> for NodeReference {
    fn from(v: String) -> Self {
        NodeReference {
            name: v,
            attribute: None,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
pub struct ParameterReference {
    /// The name of the parameter
    pub name: String,
    /// The key of the parameter. If this is `None` then the default value is used.
    pub key: Option<String>,
}

impl ParameterReference {
    #[cfg(feature = "core")]
    pub fn load(&self, network: &mut pywr_core::network::Network) -> Result<MetricF64, SchemaError> {
        match &self.key {
            Some(key) => {
                // Key given; this should be a multi-valued parameter
                Ok(MetricF64::MultiParameterValue((
                    network.get_multi_valued_parameter_index_by_name(&self.name)?,
                    key.clone(),
                )))
            }
            None => {
                if let Ok(idx) = network.get_parameter_index_by_name(&self.name) {
                    Ok(MetricF64::ParameterValue(idx))
                } else if let Ok(idx) = network.get_index_parameter_index_by_name(&self.name) {
                    Ok(MetricF64::IndexParameterValue(idx))
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
