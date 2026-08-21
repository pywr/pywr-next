use super::edge::Edge;
use super::nodes::{Node, NodeOrVirtualNode, VirtualNode};
use super::parameters::{Parameter, ParameterOrTimeseriesRef};
use crate::ConversionError;
use crate::data_tables::DataTable;
#[cfg(feature = "core")]
use crate::data_tables::{LoadedTableCollection, TableCollectionLoadError};
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::error::{ComponentConversionError, DuplicateNodeName, ValidationError};
use crate::metric::Metric;
use crate::metric_sets::MetricSet;
#[cfg(feature = "core")]
use crate::model::MultiNetworkTransfer;
use crate::outputs::Output;
use crate::timeseries::Timeseries;
#[cfg(feature = "core")]
use crate::timeseries::{LoadTimeseriesError, LoadedTimeseriesCollection};
use crate::v1::{ConversionData, TryIntoV2};
use crate::visit::{VisitMetrics, VisitNodeReferences, VisitPaths};
#[cfg(all(feature = "core", feature = "pyo3"))]
use pyo3::PyErr;
#[cfg(feature = "pyo3")]
use pyo3::pyclass;
#[cfg(feature = "core")]
use pywr_core::models::ModelDomain;
use pywr_schema_macros::skip_serializing_none;
use pywr_v1_schema::nodes::{CoreNode as CoreNodeV1, Node as NodeV1};
use schemars::JsonSchema;
use std::collections::HashMap;
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
    #[error("Network schema validation failed: {source}")]
    Validation {
        #[source]
        source: ValidationError,
    },
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

#[derive(Error, Debug)]
#[allow(clippy::enum_variant_names)] // We want to be explicit about the error types for clarity.
pub enum NetworkMergeError {
    #[error("Duplicate node name found when merging networks: {0}")]
    DuplicateNodeName(String),
    #[error("Duplicate parameter name found when merging networks: {0}")]
    DuplicateParameterName(String),
    #[error("Duplicate edge from `{from_node}` to `{to_node}`")]
    DuplicateEdge { from_node: String, to_node: String },
    #[error("Duplicate table name found when merging networks: {0}")]
    DuplicateTableName(String),
    #[error("Duplicate timeseries name found when merging networks: {0}")]
    DuplicateTimeseriesName(String),
    #[error("Duplicate output name found when merging networks: {0}")]
    DuplicateOutputName(String),
    #[error("Duplicate metric found when merging metric sets with name `{0}`")]
    DuplicateMetric(String),
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
#[cfg_attr(feature = "pyo3", pyclass(skip_from_py_object))]
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

        for virtual_node in self.virtual_nodes.as_deref().into_iter().flatten() {
            virtual_node.visit_metrics(visitor);
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

        for virtual_node in self.virtual_nodes.as_deref_mut().into_iter().flatten() {
            virtual_node.visit_metrics_mut(visitor);
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

impl VisitNodeReferences for NetworkSchema {
    fn visit_node_references<F: FnMut(&str)>(&self, visitor: &mut F) {
        for node in &self.nodes {
            node.visit_node_references(visitor);
        }

        for edge in &self.edges {
            edge.visit_node_references(visitor);
        }

        for virtual_node in self.virtual_nodes.as_deref().into_iter().flatten() {
            virtual_node.visit_node_references(visitor);
        }

        for parameter in self.parameters.as_deref().into_iter().flatten() {
            parameter.visit_node_references(visitor);
        }

        for metric_set in self.metric_sets.as_deref().into_iter().flatten() {
            metric_set.metrics.visit_node_references(visitor);
        }
    }

    fn visit_node_references_mut<F: FnMut(&mut String)>(&mut self, visitor: &mut F) {
        for node in self.nodes.iter_mut() {
            node.visit_node_references_mut(visitor);
        }

        for edge in self.edges.iter_mut() {
            edge.visit_node_references_mut(visitor);
        }

        for virtual_node in self.virtual_nodes.as_deref_mut().into_iter().flatten() {
            virtual_node.visit_node_references_mut(visitor);
        }

        for parameter in self.parameters.as_deref_mut().into_iter().flatten() {
            parameter.visit_node_references_mut(visitor);
        }

        for metric_set in self.metric_sets.as_deref_mut().into_iter().flatten() {
            metric_set.metrics.visit_node_references_mut(visitor);
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
                        errors.push(*e);
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
                    Err(e) => errors.push(*e),
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

    pub fn get_node_by_name_mut(&mut self, name: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.name() == name)
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

    pub fn get_virtual_node_by_name_mut(&mut self, name: &str) -> Option<&mut VirtualNode> {
        match &mut self.virtual_nodes {
            Some(virtual_nodes) => virtual_nodes.iter_mut().find(|n| n.name() == name),
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

    /// Returns true if any node or virtual node in the network is called `name`.
    pub fn node_name_exists(&self, name: &str) -> bool {
        self.get_node_by_name(name).is_some() || self.get_virtual_node_by_name(name).is_some()
    }

    pub fn get_parameter_by_name(&self, name: &str) -> Option<&Parameter> {
        match &self.parameters {
            Some(parameters) => parameters.iter().find(|p| p.name() == name),
            None => None,
        }
    }

    pub fn get_parameter_by_name_mut(&mut self, name: &str) -> Option<&mut Parameter> {
        match &mut self.parameters {
            Some(parameters) => parameters.iter_mut().find(|p| p.name() == name),
            None => None,
        }
    }

    pub fn parameter_exists(&self, name: &str) -> bool {
        self.get_parameter_by_name(name).is_some()
    }

    pub fn get_table_by_name(&self, name: &str) -> Option<&DataTable> {
        match &self.tables {
            Some(tables) => tables.iter().find(|t| t.name() == name),
            None => None,
        }
    }

    pub fn get_table_by_name_mut(&mut self, name: &str) -> Option<&mut DataTable> {
        match &mut self.tables {
            Some(tables) => tables.iter_mut().find(|t| t.name() == name),
            None => None,
        }
    }

    pub fn table_exists(&self, name: &str) -> bool {
        self.get_table_by_name(name).is_some()
    }

    pub fn get_timeseries_by_name(&self, name: &str) -> Option<&Timeseries> {
        match &self.timeseries {
            Some(timeseries) => timeseries.iter().find(|t| t.name() == name),
            None => None,
        }
    }

    pub fn get_timeseries_by_name_mut(&mut self, name: &str) -> Option<&mut Timeseries> {
        match &mut self.timeseries {
            Some(timeseries) => timeseries.iter_mut().find(|t| t.name() == name),
            None => None,
        }
    }

    pub fn timeseries_exists(&self, name: &str) -> bool {
        self.get_timeseries_by_name(name).is_some()
    }

    pub fn get_metric_set_by_name(&self, name: &str) -> Option<&MetricSet> {
        match &self.metric_sets {
            Some(metric_sets) => metric_sets.iter().find(|ms| ms.name == name),
            None => None,
        }
    }

    pub fn get_metric_set_by_name_mut(&mut self, name: &str) -> Option<&mut MetricSet> {
        match &mut self.metric_sets {
            Some(metric_sets) => metric_sets.iter_mut().find(|ms| ms.name == name),
            None => None,
        }
    }

    pub fn metric_set_exists(&self, name: &str) -> bool {
        self.get_metric_set_by_name(name).is_some()
    }

    pub fn get_output_by_name(&self, name: &str) -> Option<&Output> {
        match &self.outputs {
            Some(outputs) => outputs.iter().find(|o| o.name() == name),
            None => None,
        }
    }

    pub fn get_output_by_name_mut(&mut self, name: &str) -> Option<&mut Output> {
        match &mut self.outputs {
            Some(outputs) => outputs.iter_mut().find(|o| o.name() == name),
            None => None,
        }
    }

    pub fn output_exists(&self, name: &str) -> bool {
        self.get_output_by_name(name).is_some()
    }

    /// Validate the network schema.
    ///
    /// This checks that the schema is unambiguous, not that it can be built; use
    /// [`NetworkSchema::add_to_network`] for the latter. See [`ValidationError`] for the
    /// problems that are detected.
    pub fn validate(&self) -> Result<(), ValidationError> {
        // Count the occurrences of each name in each of the two lists.
        let mut counts: HashMap<&str, (usize, usize)> = HashMap::with_capacity(self.nodes.len());

        for node in &self.nodes {
            counts.entry(node.name()).or_default().0 += 1;
        }

        for virtual_node in self.virtual_nodes.as_deref().into_iter().flatten() {
            counts.entry(virtual_node.name()).or_default().1 += 1;
        }

        let mut duplicates: Vec<DuplicateNodeName> = counts
            .into_iter()
            .filter(|(_, (nodes, virtual_nodes))| nodes + virtual_nodes > 1)
            .map(|(name, (nodes, virtual_nodes))| DuplicateNodeName {
                name: name.to_string(),
                nodes,
                virtual_nodes,
            })
            .collect();

        if duplicates.is_empty() {
            Ok(())
        } else {
            // Sort for a deterministic error message.
            duplicates.sort_by(|a, b| a.name.cmp(&b.name));
            Err(ValidationError::DuplicateNodeNames(duplicates))
        }
    }

    #[cfg(feature = "core")]
    pub fn add_to_network(
        &self,
        network_builder: &mut pywr_core::network::NetworkBuilder,
        domain: &ModelDomain,
        data_path: Option<&Path>,
        output_path: Option<&Path>,
        inter_network_transfers: &[MultiNetworkTransfer],
    ) -> Result<(LoadedTableCollection, LoadedTimeseriesCollection), NetworkSchemaBuildError> {
        // Reject an invalid schema before doing any work to build it.
        self.validate()
            .map_err(|source| NetworkSchemaBuildError::Validation { source })?;

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

        for node in &self.nodes {
            node.add_to_network(network_builder, &args)
                .map_err(|source| NetworkSchemaBuildError::AddNodeError {
                    name: node.name().to_string(),
                    source: Box::new(source),
                })?;
        }

        if let Some(virtual_nodes) = &self.virtual_nodes {
            for v_node in virtual_nodes {
                v_node.add_to_network(network_builder, &args).map_err(|source| {
                    NetworkSchemaBuildError::AddVirtualNodeError {
                        name: v_node.name().to_string(),
                        source: Box::new(source),
                    }
                })?;
            }
        }

        // Create the edges
        for edge in &self.edges {
            edge.add_to_network(network_builder, &args)
                .map_err(|source| NetworkSchemaBuildError::AddEdgeError {
                    from_node: edge.from_node.clone(),
                    to_node: edge.to_node.clone(),
                    source: Box::new(source),
                })?;
        }

        // Add all the parameters from the nodes
        for node in &self.nodes {
            if let Some(local_parameters) = node.local_parameters() {
                for parameter in local_parameters {
                    parameter
                        .add_to_network(network_builder, &args, Some(node.name()))
                        .map_err(|source| NetworkSchemaBuildError::AddLocalParameterError {
                            parent: node.name().to_string(),
                            name: parameter.name().to_string(),
                            source: Box::new(source),
                        })?;
                }
            }
        }
        // Add any global parameters
        if let Some(parameters) = self.parameters.as_deref() {
            for parameter in parameters {
                parameter
                    .add_to_network(network_builder, &args, None)
                    .map_err(|source| NetworkSchemaBuildError::AddParameterError {
                        name: parameter.name().to_string(),
                        source: Box::new(source),
                    })?;
            }
        }

        // Create all of the metric sets
        if let Some(metric_sets) = &self.metric_sets {
            for metric_set in metric_sets {
                metric_set.add_to_network(network_builder, &args).map_err(|source| {
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
                    .add_to_model(network_builder, data_path, output_path)
                    .map_err(|source| NetworkSchemaBuildError::AddOutputError {
                        name: output.name().to_string(),
                        source: Box::new(source),
                    })?;
            }
        }

        Ok((tables, timeseries))
    }

    /// Merge another [`NetworkSchema`] into this one.
    ///
    /// This will combine the nodes, virtual nodes, edges, and parameters of both networks.
    /// If there are any duplicate node or parameter names, an error will be returned. However,
    /// placeholder types (e.g. [`crate::nodes::PlaceholderNode`]) are replaced.
    ///
    /// Metric sets are merged by name, with the metrics of any metric sets with the same name being
    /// combined. Other information in the metric set (e.g. filters) is **not** merged.
    pub fn merge(&mut self, other: NetworkSchema) -> Result<(), NetworkMergeError> {
        // Merge nodes replacing placeholders at their index if they exist, otherwise appending
        // to the end of the list, or returning an error if a duplicate name is found.
        for node in other.nodes {
            match self.get_node_by_name_mut(node.name()) {
                Some(existing_node) => {
                    if existing_node.is_placeholder() {
                        *existing_node = node;
                    } else {
                        return Err(NetworkMergeError::DuplicateNodeName(node.name().to_string()));
                    }
                }
                None => {
                    // Check if the node name exists in the virtual nodes list
                    if self.get_virtual_node_index_by_name(node.name()).is_some() {
                        return Err(NetworkMergeError::DuplicateNodeName(node.name().to_string()));
                    }
                    self.nodes.push(node.clone());
                }
            }
        }

        // Merge virtual nodes. As per nodes, replacing placeholders at their index if they exist,
        // otherwise appending to the end of the list, or returning an error if a duplicate name is found.
        if let Some(other_virtual_nodes) = other.virtual_nodes {
            for v_node in other_virtual_nodes {
                match self.get_virtual_node_by_name_mut(v_node.name()) {
                    Some(existing_node) => {
                        if existing_node.is_placeholder() {
                            *existing_node = v_node;
                        } else {
                            return Err(NetworkMergeError::DuplicateNodeName(v_node.name().to_string()));
                        }
                    }
                    None => {
                        // Check if the node name exists in the virtual nodes list
                        if self.get_virtual_node_index_by_name(v_node.name()).is_some() {
                            return Err(NetworkMergeError::DuplicateNodeName(v_node.name().to_string()));
                        }

                        self.virtual_nodes.get_or_insert_default().push(v_node);
                    }
                }
            }
        }

        // Merge edges checking for duplicates
        for edge in other.edges {
            if self.edges.iter().any(|e| e == &edge) {
                return Err(NetworkMergeError::DuplicateEdge {
                    from_node: edge.from_node,
                    to_node: edge.to_node,
                });
            }
            self.edges.push(edge);
        }

        // Merge parameters
        if let Some(other_parameters) = other.parameters {
            for param in other_parameters {
                match self.get_parameter_by_name_mut(param.name()) {
                    Some(existing_param) => {
                        if existing_param.is_placeholder() {
                            *existing_param = param;
                        } else {
                            return Err(NetworkMergeError::DuplicateParameterName(param.name().to_string()));
                        }
                    }
                    None => {
                        self.parameters.get_or_insert_default().push(param);
                    }
                }
            }
        }

        // Merge tables
        if let Some(other_tables) = other.tables {
            for table in other_tables {
                match self.get_table_by_name_mut(table.name()) {
                    Some(existing_table) => {
                        if existing_table.is_placeholder() {
                            *existing_table = table;
                        } else {
                            return Err(NetworkMergeError::DuplicateTableName(table.name().to_string()));
                        }
                    }
                    None => {
                        self.tables.get_or_insert_default().push(table);
                    }
                }
            }
        }

        // Merge timeseries
        if let Some(other_timeseries) = other.timeseries {
            for ts in other_timeseries {
                match self.get_timeseries_by_name_mut(ts.name()) {
                    Some(existing_ts) => {
                        if existing_ts.is_placeholder() {
                            *existing_ts = ts;
                        } else {
                            return Err(NetworkMergeError::DuplicateTimeseriesName(ts.name().to_string()));
                        }
                    }
                    None => {
                        self.timeseries.get_or_insert_default().push(ts);
                    }
                }
            }
        }

        // Merge metric sets. There are no placeholder metric sets. Instead, we merge the metrics
        // of any metric sets with the same name.
        if let Some(other_metric_sets) = other.metric_sets {
            for ms in other_metric_sets {
                match self.get_metric_set_by_name_mut(&ms.name) {
                    Some(existing_ms) => {
                        // Merge the metrics of the existing metric set with the new one.
                        if let Some(existing_metrics) = &mut existing_ms.metrics {
                            if let Some(new_metrics) = ms.metrics {
                                // Check for duplicate metrics
                                for new_metric in &new_metrics {
                                    if existing_metrics.iter().any(|m| m == new_metric) {
                                        return Err(NetworkMergeError::DuplicateMetric(ms.name.clone()));
                                    }
                                }

                                existing_metrics.extend(new_metrics);
                            }
                        } else {
                            existing_ms.metrics = ms.metrics;
                        }
                    }
                    None => {
                        // No existing metric set with this name, so we can just add it.
                        self.metric_sets.get_or_insert_default().push(ms);
                    }
                }
            }
        }

        // Merge outputs. Replacing placeholders at their index if they exist, otherwise appending
        // to the end of the list, or returning an error if a duplicate name is found.
        if let Some(other_outputs) = other.outputs {
            for output in other_outputs {
                match self.get_output_by_name_mut(output.name()) {
                    Some(existing_output) => {
                        if existing_output.is_placeholder() {
                            *existing_output = output;
                        } else {
                            return Err(NetworkMergeError::DuplicateOutputName(output.name().to_string()));
                        }
                    }
                    None => {
                        self.outputs.get_or_insert_default().push(output);
                    }
                }
            }
        }

        Ok(())
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

#[cfg(test)]
mod tests {
    use super::{NetworkMergeError, NetworkSchema};
    use crate::error::{DuplicateNodeName, ValidationError};
    use std::str::FromStr;

    /// Return the duplicates reported by [`NetworkSchema::validate`], or panic if it succeeded.
    fn expect_duplicates(network: &NetworkSchema) -> Vec<DuplicateNodeName> {
        match network.validate() {
            Err(ValidationError::DuplicateNodeNames(duplicates)) => duplicates,
            Ok(()) => panic!("Expected validation to fail, but it succeeded"),
        }
    }

    fn parse_network(data: &str) -> NetworkSchema {
        NetworkSchema::from_str(data).expect("Failed to parse test network JSON")
    }

    /// A network where a node and a virtual node are both called `licence`.
    const NETWORK_WITH_SHARED_NODE_AND_VIRTUAL_NODE_NAME: &str = r#"
    {
        "nodes": [
            { "meta": { "name": "licence" }, "type": "Input" },
            { "meta": { "name": "demand1" }, "type": "Output" }
        ],
        "virtual_nodes": [
            {
                "meta": { "name": "licence" },
                "type": "Aggregated",
                "nodes": [{ "name": "demand1" }]
            }
        ],
        "edges": [
            { "from_node": "licence", "to_node": "demand1" }
        ]
    }
    "#;

    /// Nodes and virtual nodes are a single name-space, so a name shared between the two lists
    /// is a duplicate.
    #[test]
    fn test_validate_rejects_name_shared_with_virtual_node() {
        let network = parse_network(NETWORK_WITH_SHARED_NODE_AND_VIRTUAL_NODE_NAME);

        assert_eq!(
            expect_duplicates(&network),
            vec![DuplicateNodeName {
                name: "licence".to_string(),
                nodes: 1,
                virtual_nodes: 1,
            }]
        );
    }

    /// A network with two separately duplicated names, plus a unique one.
    const NETWORK_WITH_SEVERAL_DUPLICATES: &str = r#"
    {
        "nodes": [
            { "meta": { "name": "zzz" }, "type": "Input" },
            { "meta": { "name": "zzz" }, "type": "Input" },
            { "meta": { "name": "aaa" }, "type": "Output" },
            { "meta": { "name": "unique" }, "type": "Output" }
        ],
        "virtual_nodes": [
            {
                "meta": { "name": "aaa" },
                "type": "Aggregated",
                "nodes": [{ "name": "unique" }]
            }
        ],
        "edges": []
    }
    "#;

    /// Every duplicate is reported, not just the first one found.
    #[test]
    fn test_validate_reports_all_duplicates() {
        let network = parse_network(NETWORK_WITH_SEVERAL_DUPLICATES);

        assert_eq!(
            expect_duplicates(&network),
            vec![
                DuplicateNodeName {
                    name: "aaa".to_string(),
                    nodes: 1,
                    virtual_nodes: 1,
                },
                DuplicateNodeName {
                    name: "zzz".to_string(),
                    nodes: 2,
                    virtual_nodes: 0,
                },
            ]
        );
    }

    #[test]
    fn test_merge_appends_unique_nodes_and_edges() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "a" }, "type": "Input" },
                    { "meta": { "name": "b" }, "type": "Output" }
                ],
                "edges": [
                    { "from_node": "a", "to_node": "b" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "c" }, "type": "Output" }
                ],
                "edges": [
                    { "from_node": "b", "to_node": "c" }
                ]
            }
            "#,
        );

        base.merge(other).expect("Merge should succeed");

        assert_eq!(base.nodes.len(), 3);
        assert_eq!(base.edges.len(), 2);
        assert!(base.get_node_by_name("c").is_some());
    }

    #[test]
    fn test_merge_replaces_placeholder_node() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "shared" }, "type": "Placeholder" }
                ],
                "edges": []
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "shared" }, "type": "Input" }
                ],
                "edges": []
            }
            "#,
        );

        base.merge(other).expect("Merge should replace placeholder node");

        let merged = base.get_node_by_name("shared").expect("Node should exist after merge");
        assert!(!merged.is_placeholder());
    }

    #[test]
    fn test_merge_rejects_duplicate_non_placeholder_node_name() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "shared" }, "type": "Input" }
                ],
                "edges": []
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "shared" }, "type": "Output" }
                ],
                "edges": []
            }
            "#,
        );

        let err = base.merge(other).expect_err("Merge should reject duplicate node names");
        assert!(matches!(err, NetworkMergeError::DuplicateNodeName(name) if name == "shared"));
    }

    #[test]
    fn test_merge_rejects_duplicate_edge() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "a" }, "type": "Input" },
                    { "meta": { "name": "b" }, "type": "Output" }
                ],
                "edges": [
                    { "from_node": "a", "to_node": "b" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [
                    { "from_node": "a", "to_node": "b" }
                ]
            }
            "#,
        );

        let err = base.merge(other).expect_err("Merge should reject duplicate edges");
        assert!(matches!(
            err,
            NetworkMergeError::DuplicateEdge { from_node, to_node } if from_node == "a" && to_node == "b"
        ));
    }

    #[test]
    fn test_merge_combines_metric_set_content_for_matching_names() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "metric_sets": [
                    { "name": "main" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "metric_sets": [
                    { "name": "main", "metrics": [] }
                ]
            }
            "#,
        );

        base.merge(other).expect("Merge should succeed");

        let metric_sets = base.metric_sets.as_ref().expect("Metric sets should exist");
        assert_eq!(metric_sets.len(), 1);
        assert!(
            base.get_metric_set_by_name("main")
                .and_then(|ms| ms.metrics.as_ref())
                .is_some_and(|metrics| metrics.is_empty())
        );
    }

    #[test]
    fn test_merge_replaces_placeholder_virtual_node() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "virtual_nodes": [
                    { "meta": { "name": "v-shared" }, "type": "Placeholder" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "virtual_nodes": [
                    { "meta": { "name": "v-shared" }, "type": "Aggregated", "nodes": [] }
                ]
            }
            "#,
        );

        base.merge(other)
            .expect("Merge should replace placeholder virtual node");

        let merged = base
            .get_virtual_node_by_name("v-shared")
            .expect("Virtual node should exist after merge");
        assert!(!merged.is_placeholder());
    }

    #[test]
    fn test_merge_replaces_placeholder_parameter() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "parameters": [
                    { "type": "Placeholder", "meta": { "name": "p-shared" } }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "parameters": [
                    { "type": "Constant", "meta": { "name": "p-shared" }, "value": { "type": "Literal", "value": 1.0 } }
                ]
            }
            "#,
        );

        base.merge(other).expect("Merge should replace placeholder parameter");

        let merged = base
            .get_parameter_by_name("p-shared")
            .expect("Parameter should exist after merge");
        assert!(!merged.is_placeholder());
    }

    #[test]
    fn test_merge_rejects_duplicate_parameter_name() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "parameters": [
                    { "type": "Constant", "meta": { "name": "p-shared" }, "value": { "type": "Literal", "value": 1.0 } }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "parameters": [
                    { "type": "Constant", "meta": { "name": "p-shared" }, "value": { "type": "Literal", "value": 2.0 } }
                ]
            }
            "#,
        );

        let err = base
            .merge(other)
            .expect_err("Merge should reject duplicate parameter names");
        assert!(matches!(err, NetworkMergeError::DuplicateParameterName(name) if name == "p-shared"));
    }

    #[test]
    fn test_merge_replaces_placeholder_table() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "tables": [
                    { "format": "Placeholder", "meta": { "name": "tbl-shared" } }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "tables": [
                    { "format": "CSV", "meta": { "name": "tbl-shared" }, "type": "Scalar", "lookup": { "type": "Row", "cols": 1 }, "url": "data.csv" }
                ]
            }
            "#,
        );

        base.merge(other).expect("Merge should replace placeholder table");

        let merged = base
            .get_table_by_name("tbl-shared")
            .expect("Table should exist after merge");
        assert!(!merged.is_placeholder());
    }

    #[test]
    fn test_merge_rejects_duplicate_table_name() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "tables": [
                    { "format": "CSV", "meta": { "name": "tbl-shared" }, "type": "Scalar", "lookup": { "type": "Row", "cols": 1 }, "url": "data.csv" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "tables": [
                    { "format": "CSV", "meta": { "name": "tbl-shared" }, "type": "Scalar", "lookup": { "type": "Row", "cols": 1 }, "url": "other.csv" }
                ]
            }
            "#,
        );

        let err = base
            .merge(other)
            .expect_err("Merge should reject duplicate table names");
        assert!(matches!(err, NetworkMergeError::DuplicateTableName(name) if name == "tbl-shared"));
    }

    #[test]
    fn test_merge_replaces_placeholder_timeseries() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "timeseries": [
                    { "type": "Placeholder", "meta": { "name": "ts-shared" } }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "timeseries": [
                    { "type": "Polars", "meta": { "name": "ts-shared" }, "url": "timeseries.csv" }
                ]
            }
            "#,
        );

        base.merge(other).expect("Merge should replace placeholder timeseries");

        let merged = base
            .get_timeseries_by_name("ts-shared")
            .expect("Timeseries should exist after merge");
        assert!(!merged.is_placeholder());
    }

    #[test]
    fn test_merge_rejects_duplicate_timeseries_name() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "timeseries": [
                    { "type": "Polars", "meta": { "name": "ts-shared" }, "url": "timeseries.csv" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "timeseries": [
                    { "type": "Polars", "meta": { "name": "ts-shared" }, "url": "other.csv" }
                ]
            }
            "#,
        );

        let err = base
            .merge(other)
            .expect_err("Merge should reject duplicate timeseries names");
        assert!(matches!(err, NetworkMergeError::DuplicateTimeseriesName(name) if name == "ts-shared"));
    }

    #[test]
    fn test_merge_replaces_placeholder_output() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "outputs": [
                    { "type": "Placeholder", "name": "out-shared" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "outputs": [
                    { "type": "Memory", "name": "out-shared", "metric_set": "ms" }
                ]
            }
            "#,
        );

        base.merge(other).expect("Merge should replace placeholder output");

        let merged = base
            .get_output_by_name("out-shared")
            .expect("Output should exist after merge");
        assert!(!merged.is_placeholder());
    }

    #[test]
    fn test_merge_rejects_duplicate_output_name() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "outputs": [
                    { "type": "Memory", "name": "out-shared", "metric_set": "ms" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "outputs": [
                    { "type": "Memory", "name": "out-shared", "metric_set": "ms2" }
                ]
            }
            "#,
        );

        let err = base
            .merge(other)
            .expect_err("Merge should reject duplicate output names");
        assert!(matches!(err, NetworkMergeError::DuplicateOutputName(name) if name == "out-shared"));
    }
}
