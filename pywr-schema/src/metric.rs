use crate::data_tables::TableDataRef;
use crate::edge::Edge;
use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
#[cfg(feature = "core")]
use crate::model::LoadArgs;
use crate::nodes::NodeAttribute;
#[cfg(feature = "core")]
use crate::nodes::NodeType;
use crate::parameters::ParameterOrTimeseriesRef;
#[cfg(feature = "core")]
use crate::parameters::ParameterType;
#[cfg(feature = "core")]
use crate::timeseries::TimeseriesColumns;
use crate::timeseries::TimeseriesReference;
use crate::v1::{ConversionData, TryFromV1, TryIntoV2};
use crate::ConversionError;
#[cfg(feature = "core")]
use pywr_core::{
    metric::{MetricF64, MetricU64},
    models::MultiNetworkTransferIndex,
    parameters::ParameterName,
    recorders::OutputMetric,
};
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::ParameterValue as ParameterValueV1;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

/// A floating point value representing different model metrics.
///
/// Metrics can be used in various places in a model to create dynamic behaviour. For example,
/// parameter can use an arbitrary [`Metric`] for its calculation giving the user the ability
/// to configure the source of that value. Therefore, metrics are the primary way in which
/// dynamic behaviour is created.
///
/// See also [`IndexMetric`] for integer values.
#[derive(Deserialize, Serialize, Clone, Debug, Display, JsonSchema, PartialEq, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
// This creates a separate enum called `MetricType` that is available in this module.
#[strum_discriminants(name(MetricType))]
pub enum Metric {
    /// A constant floating point value.
    Constant { value: f64 },
    /// A reference to a constant value in a table.
    Table(TableDataRef),
    /// An attribute of a node.
    Node(NodeReference),
    /// An attribute of an edge.
    Edge(EdgeReference),
    /// A reference to a value from a timeseries.
    Timeseries(TimeseriesReference),
    /// A reference to a global parameter.
    Parameter(ParameterReference),
    /// A reference to a local parameter.
    LocalParameter(ParameterReference),
    /// A reference to an inter-network transfer by name.
    InterNetworkTransfer { name: String },
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
    pub fn load(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<MetricF64, SchemaError> {
        match self {
            Self::Node(node_ref) => node_ref.load_f64(network, args),
            // Global parameter with no parent
            Self::Parameter(parameter_ref) => parameter_ref.load_f64(network, None),
            // Local parameter loaded from parent's namespace
            Self::LocalParameter(parameter_ref) => {
                if parent.is_none() {
                    return Err(SchemaError::LocalParameterReferenceRequiresParent(
                        parameter_ref.name.clone(),
                    ));
                }

                parameter_ref.load_f64(network, parent)
            }
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
                    Some(TimeseriesColumns::Scenario { name }) => {
                        args.timeseries
                            .load_df_f64(network, ts_ref.name.as_ref(), args.domain, name.as_str())?
                    }
                    Some(TimeseriesColumns::Column { name }) => {
                        args.timeseries
                            .load_column_f64(network, ts_ref.name.as_ref(), name.as_str())?
                    }
                    None => args.timeseries.load_single_column_f64(network, ts_ref.name.as_ref())?,
                };
                Ok(param_idx.into())
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
            Self::LocalParameter(parameter_ref) => Ok(parameter_ref.name.clone()),
            Self::Constant { .. } => Err(SchemaError::LiteralConstantOutputNotSupported),
            Self::Table(table_ref) => Ok(table_ref.table.clone()),
            Self::Timeseries(ts_ref) => Ok(ts_ref.name.clone()),
            Self::InterNetworkTransfer { name } => Ok(name.clone()),
            Self::Edge(edge_ref) => Ok(edge_ref.edge.to_string()),
        }
    }

    fn attribute(&self, args: &LoadArgs) -> Result<String, SchemaError> {
        let attribute = match self {
            Self::Node(node_ref) => node_ref.attribute(args)?.to_string(),
            Self::Parameter(p_ref) => p_ref.key.clone().unwrap_or_else(|| "value".to_string()),
            Self::LocalParameter(p_ref) => p_ref.key.clone().unwrap_or_else(|| "value".to_string()),
            Self::Constant { .. } => "value".to_string(),
            Self::Table(_) => "value".to_string(),
            Self::Timeseries(_) => "value".to_string(),
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
            Self::LocalParameter(parameter_ref) => Some(parameter_ref.parameter_type(args)?.to_string()),
            Self::Constant { .. } => None,
            Self::Table(_) => None,
            Self::Timeseries(_) => None,
            Self::InterNetworkTransfer { .. } => None,
            Self::Edge { .. } => None,
        };

        Ok(sub_type)
    }

    pub fn load_as_output(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<OutputMetric, SchemaError> {
        let metric = self.load(network, args, parent)?;

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

impl TryFromV1<ParameterValueV1> for Metric {
    type Error = ConversionError;

    fn try_from_v1(
        v1: ParameterValueV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let p = match v1 {
            ParameterValueV1::Constant(value) => Self::Constant { value },
            ParameterValueV1::Reference(p_name) => Self::Parameter(ParameterReference {
                name: p_name,
                key: None,
            }),
            ParameterValueV1::Table(tbl) => Self::Table(tbl.try_into()?),
            ParameterValueV1::Inline(param) => {
                // Inline parameters are converted to either a parameter or a timeseries
                // The actual component is extracted into the conversion data leaving a reference
                // to the component in the metric.
                let definition: ParameterOrTimeseriesRef =
                    (*param)
                        .try_into_v2(parent_node, conversion_data)
                        .map_err(|e| match e {
                            ComponentConversionError::Parameter { error, .. } => error,
                            ComponentConversionError::Node { error, .. } => error,
                        })?;
                match definition {
                    ParameterOrTimeseriesRef::Parameter(p) => {
                        let reference = ParameterReference {
                            name: p.name().to_string(),
                            key: None,
                        };
                        conversion_data.parameters.push(*p);

                        Self::Parameter(reference)
                    }
                    ParameterOrTimeseriesRef::Timeseries(t) => Self::Timeseries(t.ts_ref),
                }
            }
        };
        Ok(p)
    }
}

/// A reference to a node with an optional attribute.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll, PartialEq)]
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

    /// Load a node reference into a [`MetricF64`].
    #[cfg(feature = "core")]
    pub fn load_f64(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<MetricF64, SchemaError> {
        // This is the associated node in the schema
        let node = args
            .schema
            .get_node_by_name(&self.name)
            .ok_or_else(|| SchemaError::NodeNotFound(self.name.clone()))?;

        node.create_metric(network, self.attribute, args)
    }

    /// Load a node reference into a [`MetricUsize`].
    #[cfg(feature = "core")]
    pub fn load_u64(
        &self,
        _network: &mut pywr_core::network::Network,
        args: &LoadArgs,
    ) -> Result<MetricU64, SchemaError> {
        // This is the associated node in the schema
        let _node = args
            .schema
            .get_node_by_name(&self.name)
            .ok_or_else(|| SchemaError::NodeNotFound(self.name.clone()))?;

        todo!("Support usize attributes on nodes.")
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ParameterReference {
    /// The name of the parameter
    pub name: String,
    /// The key of the parameter. If this is `None` then the default value is used.
    pub key: Option<String>,
}

impl ParameterReference {
    pub fn new(name: &str, key: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            key,
        }
    }

    /// Load a parameter reference into a [`MetricF64`] by attempting to retrieve the parameter
    /// from the `network`. If `parent` is the optional parameter name space from which to load
    /// the parameter.
    #[cfg(feature = "core")]
    pub fn load_f64(
        &self,
        network: &mut pywr_core::network::Network,
        parent: Option<&str>,
    ) -> Result<MetricF64, SchemaError> {
        let name = ParameterName::new(&self.name, parent);

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

    /// Load a parameter reference into a [`MetricUsize`] by attempting to retrieve the parameter
    /// from the `network`. If `parent` is the optional parameter name space from which to load
    /// the parameter.
    #[cfg(feature = "core")]
    pub fn load_u64(
        &self,
        network: &mut pywr_core::network::Network,
        parent: Option<&str>,
    ) -> Result<MetricU64, SchemaError> {
        let name = ParameterName::new(&self.name, parent);

        match &self.key {
            Some(key) => {
                // Key given; this should be a multi-valued parameter
                Ok((network.get_multi_valued_parameter_index_by_name(&name)?, key.clone()).into())
            }
            None => {
                if let Ok(idx) = network.get_index_parameter_index_by_name(&name) {
                    Ok(idx.into())
                } else if network.get_parameter_index_by_name(&name).is_ok() {
                    // Inform the user we found the parameter, but it was the wrong type
                    Err(SchemaError::IndexParameterExpected(self.name.to_string()))
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PartialEq)]
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

/// An unsigned integer value representing different model metrics.
///
/// This struct is the integer equivalent of [`Metric`] and is used in places where an integer
/// value is required. See [`Metric`] for more information.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, Display, PartialEq, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(IndexMetricType))]
pub enum IndexMetric {
    Constant {
        value: u64,
    },
    Table(TableDataRef),
    /// An attribute of a node.
    Node(NodeReference),
    Timeseries(TimeseriesReference),
    Parameter(ParameterReference),
    LocalParameter(ParameterReference),
    InterNetworkTransfer {
        name: String,
    },
}

impl From<usize> for IndexMetric {
    fn from(v: usize) -> Self {
        Self::Constant { value: v as u64 }
    }
}

impl From<u64> for IndexMetric {
    fn from(v: u64) -> Self {
        Self::Constant { value: v }
    }
}

#[cfg(feature = "core")]
impl IndexMetric {
    pub fn load(
        &self,
        network: &mut pywr_core::network::Network,
        args: &LoadArgs,
        parent: Option<&str>,
    ) -> Result<MetricU64, SchemaError> {
        match self {
            Self::Node(node_ref) => node_ref.load_u64(network, args),
            // Global parameter with no parent
            Self::Parameter(parameter_ref) => parameter_ref.load_u64(network, None),
            // Local parameter loaded from parent's namespace
            Self::LocalParameter(parameter_ref) => {
                if parent.is_none() {
                    return Err(SchemaError::LocalParameterReferenceRequiresParent(
                        parameter_ref.name.clone(),
                    ));
                }

                parameter_ref.load_u64(network, parent)
            }
            Self::Constant { value } => Ok((*value).into()),
            Self::Table(table_ref) => {
                let value = args
                    .tables
                    .get_scalar_u64(table_ref)
                    .map_err(|error| SchemaError::TableRefLoad {
                        table_ref: table_ref.clone(),
                        error,
                    })?;
                Ok(value.into())
            }
            Self::Timeseries(ts_ref) => {
                let param_idx = match &ts_ref.columns {
                    Some(TimeseriesColumns::Scenario { name }) => {
                        args.timeseries
                            .load_df_usize(network, ts_ref.name.as_ref(), args.domain, name.as_str())?
                    }
                    Some(TimeseriesColumns::Column { name }) => {
                        args.timeseries
                            .load_column_usize(network, ts_ref.name.as_ref(), name.as_str())?
                    }
                    None => args
                        .timeseries
                        .load_single_column_usize(network, ts_ref.name.as_ref())?,
                };
                Ok(param_idx.into())
            }
            Self::InterNetworkTransfer { name } => {
                // Find the matching inter model transfer
                match args.inter_network_transfers.iter().position(|t| &t.name == name) {
                    Some(idx) => Ok(MetricU64::InterNetworkTransfer(MultiNetworkTransferIndex(idx))),
                    None => Err(SchemaError::InterNetworkTransferNotFound(name.to_string())),
                }
            }
        }
    }
}

impl TryFromV1<ParameterValueV1> for IndexMetric {
    type Error = ConversionError;

    fn try_from_v1(
        v1: ParameterValueV1,
        parent_node: Option<&str>,
        conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        let p = match v1 {
            // There was no such thing as s constant index in Pywr v1
            // TODO this could print a warning and do a cast to usize instead.
            ParameterValueV1::Constant(value) => {
                // Check if the value is not a whole non-negative number
                if value.fract() != 0.0 || value < 0.0 {
                    return Err(ConversionError::FloatToIndex {});
                }

                Self::Constant { value: value as u64 }
            }
            ParameterValueV1::Reference(p_name) => Self::Parameter(ParameterReference {
                name: p_name,
                key: None,
            }),
            ParameterValueV1::Table(tbl) => Self::Table(tbl.try_into()?),
            ParameterValueV1::Inline(param) => {
                // Inline parameters are converted to either a parameter or a timeseries
                // The actual component is extracted into the conversion data leaving a reference
                // to the component in the metric.
                let definition: ParameterOrTimeseriesRef =
                    (*param)
                        .try_into_v2(parent_node, conversion_data)
                        .map_err(|e| match e {
                            ComponentConversionError::Parameter { error, .. } => error,
                            ComponentConversionError::Node { error, .. } => error,
                        })?;
                match definition {
                    ParameterOrTimeseriesRef::Parameter(p) => {
                        let reference = ParameterReference {
                            name: p.name().to_string(),
                            key: None,
                        };
                        conversion_data.parameters.push(*p);

                        Self::Parameter(reference)
                    }
                    ParameterOrTimeseriesRef::Timeseries(t) => Self::Timeseries(t.ts_ref),
                }
            }
        };
        Ok(p)
    }
}

#[cfg(test)]
mod test {
    use super::{ConversionError, IndexMetric, ParameterValueV1, TryFromV1};

    /// Test conversion of `ParameterValueV1::Constant` to `IndexMetric`.
    #[test]
    fn test_index_metric_try_from_v1_constant() {
        let v1 = ParameterValueV1::Constant(0.0);
        let result = IndexMetric::try_from_v1(v1, None, &mut Default::default());
        assert_eq!(result, Ok(IndexMetric::Constant { value: 0 }));

        let v1 = ParameterValueV1::Constant(1.0);
        let result = IndexMetric::try_from_v1(v1, None, &mut Default::default());
        assert_eq!(result, Ok(IndexMetric::Constant { value: 1 }));

        let v1 = ParameterValueV1::Constant(1.5);
        let result = IndexMetric::try_from_v1(v1, None, &mut Default::default());
        assert_eq!(result, Err(ConversionError::FloatToIndex {}));

        let v1 = ParameterValueV1::Constant(-1.0);
        let result = IndexMetric::try_from_v1(v1, None, &mut Default::default());
        assert_eq!(result, Err(ConversionError::FloatToIndex {}));
    }
}
