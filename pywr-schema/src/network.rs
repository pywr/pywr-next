use super::edge::Edge;
use super::nodes::{Node, NodeOrVirtualNode, VirtualNode};
use super::parameters::{Parameter, ParameterOrTimeseriesRef};
use crate::ConversionError;
use crate::data_tables::DataTable;
#[cfg(feature = "core")]
use crate::data_tables::{LoadedTableCollection, TableCollectionLoadError};
use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
use crate::metric_sets::MetricSet;
#[cfg(feature = "core")]
use crate::model::MultiNetworkTransfer;
use crate::outputs::Output;
use crate::timeseries::Timeseries;
#[cfg(feature = "core")]
use crate::timeseries::{LoadTimeseriesError, LoadedTimeseriesCollection};
use crate::v1::{ConversionData, TryIntoV2};
use crate::visit::{VisitMetrics, VisitPaths};
#[cfg(all(feature = "core", feature = "pyo3"))]
use pyo3::PyErr;
#[cfg(feature = "pyo3")]
use pyo3::pyclass;
#[cfg(feature = "core")]
use pywr_core::models::ModelDomain;
use pywr_schema_macros::skip_serializing_none;
use pywr_v1_schema::nodes::{CoreNode as CoreNodeV1, Node as NodeV1};
use schemars::JsonSchema;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
use thiserror::Error;

/// Error type for reading a [`NetworkSchema`] network from a file or string.
#[derive(Error, Debug)]
pub enum NetworkSchemaReadError {
    #[error("IO error on path `{path}`: {error}")]
    IO { path: PathBuf, error: std::io::Error },
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Error type for building a `pywr_core::PywrNetwork` network from a schema ([`NetworkSchema`]).
#[cfg(feature = "core")]
#[derive(Error, Debug)]
pub enum NetworkSchemaBuildError {
    #[error("Circular node reference(s) found.")]
    CircularNodeReference,
    #[error("Circular parameters reference(s) found. Unable to load the following parameters: {0:?}")]
    CircularParameterReference(Vec<String>),
    #[error("Failed to add node `{name}` to the model: {source}")]
    AddNodeError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to add virtual node `{name}` to the model: {source}")]
    AddVirtualNodeError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to set constraints for node `{name}`: {source}")]
    SetNodeConstraintsError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to set constraints for virtual node `{name}`: {source}")]
    SetVirtualNodeConstraintsError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to add edge from `{from_node}` to `{to_node}`: {source}")]
    AddEdgeError {
        from_node: String,
        to_node: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to add parameter `{name}` to the model: {source}")]
    AddParameterError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to add local parameter from node `{parent}` with `{name}` to the model: {source}")]
    AddLocalParameterError {
        name: String,
        parent: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to add metric set with name `{name}` to the model: {source}")]
    AddMetricSetError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to add output with name `{name}` to the model: {source}")]
    AddOutputError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("{0}")]
    TableLoadError(#[from] TableCollectionLoadError),
    #[error("{0}")]
    LoadTimeseriesError(#[from] LoadTimeseriesError),
}

#[cfg(all(feature = "core", feature = "pyo3"))]
impl TryFrom<NetworkSchemaBuildError> for PyErr {
    type Error = ();
    fn try_from(err: NetworkSchemaBuildError) -> Result<PyErr, Self::Error> {
        match err {
            NetworkSchemaBuildError::AddNodeError { source, .. } => (*source).try_into(),
            NetworkSchemaBuildError::SetNodeConstraintsError { source, .. } => (*source).try_into(),
            NetworkSchemaBuildError::AddEdgeError { source, .. } => (*source).try_into(),
            NetworkSchemaBuildError::AddParameterError { source, .. } => (*source).try_into(),
            NetworkSchemaBuildError::AddLocalParameterError { source, .. } => (*source).try_into(),
            NetworkSchemaBuildError::AddMetricSetError { source, .. } => (*source).try_into(),
            NetworkSchemaBuildError::AddOutputError { source, .. } => (*source).try_into(),
            NetworkSchemaBuildError::LoadTimeseriesError(e) => e.try_into(),
            _ => Err(()),
        }
    }
}

#[cfg(feature = "core")]
#[derive(Clone)]
pub struct LoadArgs<'a> {
    pub schema: &'a NetworkSchema,
    pub domain: &'a ModelDomain,
    pub tables: &'a LoadedTableCollection,
    pub timeseries: &'a LoadedTimeseriesCollection,
    pub data_path: Option<&'a Path>,
    pub inter_network_transfers: &'a [MultiNetworkTransfer],
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, Default, JsonSchema)]
#[cfg_attr(feature = "pyo3", pyclass)]
#[serde(deny_unknown_fields)]
pub struct NetworkSchema {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub virtual_nodes: Option<Vec<VirtualNode>>,
    pub parameters: Option<Vec<Parameter>>,
    pub tables: Option<Vec<DataTable>>,
    pub timeseries: Option<Vec<Timeseries>>,
    pub metric_sets: Option<Vec<MetricSet>>,
    pub outputs: Option<Vec<Output>>,
}

impl FromStr for NetworkSchema {
    type Err = NetworkSchemaReadError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(serde_json::from_str(s)?)
    }
}

impl VisitPaths for NetworkSchema {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        for node in &self.nodes {
            node.visit_paths(visitor);
        }

        for parameter in self.parameters.as_deref().into_iter().flatten() {
            parameter.visit_paths(visitor);
        }

        for timeseries in self.timeseries.as_deref().into_iter().flatten() {
            timeseries.visit_paths(visitor);
        }

        for outputs in self.outputs.as_deref().into_iter().flatten() {
            outputs.visit_paths(visitor);
        }
    }
    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        for node in self.nodes.iter_mut() {
            node.visit_paths_mut(visitor);
        }

        for parameter in self.parameters.as_deref_mut().into_iter().flatten() {
            parameter.visit_paths_mut(visitor);
        }

        for timeseries in self.timeseries.as_deref_mut().into_iter().flatten() {
            timeseries.visit_paths_mut(visitor);
        }

        for outputs in self.outputs.as_deref_mut().into_iter().flatten() {
            outputs.visit_paths_mut(visitor);
        }
    }
}

impl VisitMetrics for NetworkSchema {
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        for node in &self.nodes {
            node.visit_metrics(visitor);
        }

        for parameter in self.parameters.as_deref().into_iter().flatten() {
            parameter.visit_metrics(visitor);
        }

        if let Some(metric_sets) = &self.metric_sets {
            for metric_set in metric_sets {
                if let Some(metrics) = &metric_set.metrics {
                    for metric in metrics {
                        visitor(metric);
                    }
                }
            }
        }
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        for node in self.nodes.iter_mut() {
            node.visit_metrics_mut(visitor);
        }

        for parameter in self.parameters.as_deref_mut().into_iter().flatten() {
            parameter.visit_metrics_mut(visitor);
        }

        if let Some(metric_sets) = &mut self.metric_sets {
            for metric_set in metric_sets {
                if let Some(metrics) = &mut metric_set.metrics {
                    for metric in metrics {
                        visitor(metric);
                    }
                }
            }
        }
    }
}

impl NetworkSchema {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, NetworkSchemaReadError> {
        let data = std::fs::read_to_string(&path).map_err(|error| NetworkSchemaReadError::IO {
            path: path.as_ref().to_path_buf(),
            error,
        })?;
        Ok(serde_json::from_str(data.as_str())?)
    }

    /// Convert a v1 network to a v2 network.
    ///
    /// This function is used to convert a v1 model to a v2 model. The conversion is not always
    /// possible and may result in errors. The errors are returned as a vector of [`ComponentConversionError`]s.
    /// alongside the (partially) converted model. This may result in a model that will not
    /// function as expected. The user should check the errors and the converted model to ensure
    /// that the conversion has been successful.
    pub fn from_v1(v1: pywr_v1_schema::PywrNetwork) -> (Self, Vec<ComponentConversionError>) {
        let mut errors = Vec::new();
        // We will use this to store any timeseries or parameters that are extracted from the v1 nodes
        let mut conversion_data = ConversionData::default();

        let mut nodes = Vec::with_capacity(v1.nodes.as_ref().map(|n| n.len()).unwrap_or_default());
        let mut virtual_nodes = Vec::with_capacity(v1.nodes.as_ref().map(|n| n.len()).unwrap_or_default());
        let mut parameters = Vec::new();
        let mut timeseries = Vec::new();

        // Extract nodes and any timeseries data from the v1 nodes
        if let Some(v1_nodes) = v1.nodes {
            // First find any virtual nodes so these can be used to determine metric conversion types
            for node in v1_nodes.iter() {
                match node {
                    NodeV1::Core(n) => match n.as_ref() {
                        CoreNodeV1::Aggregated(_)
                        | CoreNodeV1::AggregatedStorage(_)
                        | CoreNodeV1::VirtualStorage(_)
                        | CoreNodeV1::AnnualVirtualStorage(_)
                        | CoreNodeV1::MonthlyVirtualStorage(_)
                        | CoreNodeV1::SeasonalVirtualStorage(_)
                        | CoreNodeV1::RollingVirtualStorage(_) => {
                            conversion_data.virtual_nodes.push(n.name().to_string());
                        }
                        _ => continue,
                    },
                    _ => continue,
                }
            }

            for v1_node in v1_nodes.into_iter() {
                // Reset the unnamed count for each node because they are named by the parent node.
                conversion_data.reset_count();
                let result: Result<NodeOrVirtualNode, _> = v1_node.try_into_v2(None, &mut conversion_data);
                match result {
                    Ok(node) => match node {
                        NodeOrVirtualNode::Node(n) => nodes.push(*n),
                        NodeOrVirtualNode::Virtual(vn) => virtual_nodes.push(*vn),
                    },
                    Err(e) => {
                        errors.push(e);
                    }
                }
            }
        }

        let edges = match v1.edges {
            Some(v1_edges) => {
                let mut edges = Vec::with_capacity(v1_edges.len());
                for v1_edge in v1_edges.into_iter() {
                    match v1_edge.clone().try_into() {
                        Ok(e) => edges.push(e),
                        Err(error) => {
                            errors.push(ComponentConversionError::Edge {
                                from_node: v1_edge.from_node,
                                to_node: v1_edge.to_node,
                                error,
                            });
                        }
                    }
                }

                edges
            }
            None => Vec::new(),
        };

        // Collect any parameters that have been replaced by timeseries
        // These references will be referred to by ParameterReferences elsewhere in the schema
        // We will update these references to TimeseriesReferences later
        let mut timeseries_refs = Vec::new();
        if let Some(params) = v1.parameters {
            // Reset the unnamed count for global parameters
            conversion_data.reset_count();
            for p in params {
                let result: Result<ParameterOrTimeseriesRef, _> = p.try_into_v2(None, &mut conversion_data);
                match result {
                    Ok(p_or_t) => match p_or_t {
                        ParameterOrTimeseriesRef::Parameter(p) => parameters.push(*p),
                        ParameterOrTimeseriesRef::Timeseries(t) => timeseries_refs.push(t),
                    },
                    Err(e) => errors.push(e),
                }
            }
        }

        // Finally add any extracted timeseries data to the timeseries list
        timeseries.extend(conversion_data.timeseries);
        parameters.extend(conversion_data.parameters);

        // Closure to update a parameter ref with a timeseries ref when names match.
        // We match on the original parameter name because the parameter name may have been changed
        let update_to_ts_ref = &mut |m: &mut Metric| {
            if let Metric::Parameter(p) = m {
                if let Some(converted_ts_ref) = timeseries_refs.iter().find(|ts| ts.original_parameter_name == p.name) {
                    *m = Metric::Timeseries(converted_ts_ref.ts_ref.clone());
                }
            }
        };

        nodes.visit_metrics_mut(update_to_ts_ref);
        parameters.visit_metrics_mut(update_to_ts_ref);

        for table in v1.tables.into_iter().flatten() {
            let json_string = serde_json::to_string(&table).ok();
            errors.push(ComponentConversionError::Table {
                name: table.name.clone(),
                url: table.url,
                json: json_string,
                error: ConversionError::TableConversionNotSupported { name: table.name },
            });
        }

        // TODO convert v1 tables!
        let tables = None;
        let outputs = None;
        let metric_sets = None;
        let virtual_nodes = if !virtual_nodes.is_empty() {
            Some(virtual_nodes)
        } else {
            None
        };
        let parameters = if !parameters.is_empty() { Some(parameters) } else { None };
        let timeseries = if !timeseries.is_empty() { Some(timeseries) } else { None };

        (
            Self {
                nodes,
                edges,
                virtual_nodes,
                parameters,
                tables,
                timeseries,
                metric_sets,
                outputs,
            },
            errors,
        )
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

    pub fn get_virtual_node_by_name(&self, name: &str) -> Option<&VirtualNode> {
        match &self.virtual_nodes {
            Some(virtual_nodes) => virtual_nodes.iter().find(|n| n.name() == name),
            None => None,
        }
    }

    pub fn get_virtual_node_index_by_name(&self, name: &str) -> Option<usize> {
        match &self.virtual_nodes {
            Some(virtual_nodes) => virtual_nodes
                .iter()
                .enumerate()
                .find_map(|(idx, n)| (n.name() == name).then_some(idx)),
            None => None,
        }
    }

    pub fn get_virtual_node(&self, idx: usize) -> Option<&VirtualNode> {
        match &self.virtual_nodes {
            Some(virtual_nodes) => virtual_nodes.get(idx),
            None => None,
        }
    }

    pub fn get_parameter_by_name(&self, name: &str) -> Option<&Parameter> {
        match &self.parameters {
            Some(parameters) => parameters.iter().find(|p| p.name() == name),
            None => None,
        }
    }

    #[cfg(feature = "core")]
    pub fn build_network(
        &self,
        domain: &ModelDomain,
        data_path: Option<&Path>,
        output_path: Option<&Path>,
        inter_network_transfers: &[MultiNetworkTransfer],
    ) -> Result<
        (
            pywr_core::network::Network,
            LoadedTableCollection,
            LoadedTimeseriesCollection,
        ),
        NetworkSchemaBuildError,
    > {
        let mut network = pywr_core::network::Network::default();

        let tables = LoadedTableCollection::from_schema(self.tables.as_deref(), data_path)?;
        let timeseries = LoadedTimeseriesCollection::from_schema(self.timeseries.as_deref(), domain, data_path)?;

        let args = LoadArgs {
            schema: self,
            domain,
            tables: &tables,
            timeseries: &timeseries,
            data_path,
            inter_network_transfers,
        };

        // Create a combined list of nodes and virtual nodes
        let mut remaining_nodes: Vec<NodeOrVirtualNode> = self
            .nodes
            .iter()
            .map(|n| n.clone().into())
            .chain(
                self.virtual_nodes
                    .clone()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|vn| vn.into()),
            )
            .collect();

        while !remaining_nodes.is_empty() {
            let mut failed_nodes: Vec<NodeOrVirtualNode> = Vec::new();
            let n = remaining_nodes.len();
            for node in remaining_nodes.into_iter() {
                if let Err(e) = node.add_to_model(&mut network, &args) {
                    // Adding the node failed!
                    match e {
                        // And it failed because another node was not found.
                        // Let's try to load more nodes and see if this one can be added later
                        SchemaError::CoreNodeNotFound { .. } => failed_nodes.push(node),
                        _ => {
                            return match node {
                                NodeOrVirtualNode::Node(n) => Err(NetworkSchemaBuildError::AddNodeError {
                                    name: n.name().to_string(),
                                    source: Box::new(e),
                                }),
                                NodeOrVirtualNode::Virtual(vn) => Err(NetworkSchemaBuildError::AddVirtualNodeError {
                                    name: vn.name().to_string(),
                                    source: Box::new(e),
                                }),
                            };
                        }
                    }
                };
            }

            if failed_nodes.len() == n {
                // Could not load any nodes; must be a circular reference
                return Err(NetworkSchemaBuildError::CircularNodeReference);
            }

            remaining_nodes = failed_nodes;
        }

        // Create the edges
        for edge in &self.edges {
            edge.add_to_model(&mut network, &args)
                .map_err(|source| NetworkSchemaBuildError::AddEdgeError {
                    from_node: edge.from_node.clone(),
                    to_node: edge.to_node.clone(),
                    source: Box::new(source),
                })?;
        }

        // Gather all the parameters from the nodes
        let mut remaining_parameters: Vec<(Option<&str>, Parameter)> = Vec::new();
        for node in &self.nodes {
            if let Some(local_parameters) = node.local_parameters() {
                remaining_parameters.extend(local_parameters.iter().map(|p| (Some(node.name()), p.clone())));
            }
        }
        // Add any global parameters
        if let Some(parameters) = self.parameters.as_deref() {
            remaining_parameters.extend(parameters.iter().map(|p| (None, p.clone())));
        }

        // Create all the parameters
        while !remaining_parameters.is_empty() {
            let mut failed_parameters: Vec<(Option<&str>, Parameter)> = Vec::new();
            let n = remaining_parameters.len();
            for (parent, parameter) in remaining_parameters.into_iter() {
                if let Err(e) = parameter.add_to_model(&mut network, &args, parent) {
                    // Adding the parameter failed!
                    match e {
                        // And it failed because another parameter was not found.
                        // Let's try to load more parameters and see if this one can be added later
                        SchemaError::CoreParameterNotFound { .. } => failed_parameters.push((parent, parameter)),
                        _ => {
                            return match parent {
                                Some(p) => Err(NetworkSchemaBuildError::AddLocalParameterError {
                                    parent: p.to_string(),
                                    name: parameter.name().to_string(),
                                    source: Box::new(e),
                                }),
                                None => {
                                    // Global parameter
                                    Err(NetworkSchemaBuildError::AddParameterError {
                                        name: parameter.name().to_string(),
                                        source: Box::new(e),
                                    })
                                }
                            };
                        }
                    };
                }
            }

            if failed_parameters.len() == n {
                // Could not load any parameters; must be a circular reference
                let failed_names = failed_parameters.iter().map(|(_n, p)| p.name().to_string()).collect();
                return Err(NetworkSchemaBuildError::CircularParameterReference(failed_names));
            }

            remaining_parameters = failed_parameters;
        }

        // Apply the constraints to the nodes
        for node in &self.nodes {
            node.set_constraints(&mut network, &args).map_err(|source| {
                NetworkSchemaBuildError::SetNodeConstraintsError {
                    name: node.name().to_string(),
                    source: Box::new(source),
                }
            })?;
        }

        // Apply the constraints to the virtual nodes
        if let Some(virtual_nodes) = &self.virtual_nodes {
            for node in virtual_nodes {
                node.set_constraints(&mut network, &args).map_err(|source| {
                    NetworkSchemaBuildError::SetVirtualNodeConstraintsError {
                        name: node.name().to_string(),
                        source: Box::new(source),
                    }
                })?;
            }
        }

        // Create all of the metric sets
        if let Some(metric_sets) = &self.metric_sets {
            for metric_set in metric_sets {
                metric_set.add_to_model(&mut network, &args).map_err(|source| {
                    NetworkSchemaBuildError::AddMetricSetError {
                        name: metric_set.name.clone(),
                        source: Box::new(source),
                    }
                })?;
            }
        }

        // Create all of the outputs
        if let Some(outputs) = &self.outputs {
            for output in outputs {
                output
                    .add_to_model(&mut network, data_path, output_path)
                    .map_err(|source| NetworkSchemaBuildError::AddOutputError {
                        name: output.name().to_string(),
                        source: Box::new(source),
                    })?;
            }
        }

        Ok((network, tables, timeseries))
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Display, EnumDiscriminants)]
#[serde(untagged)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(NetworkSchemaRefType))]
pub enum NetworkSchemaRef {
    Path(PathBuf),
    Inline(NetworkSchema),
}
