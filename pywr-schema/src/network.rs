use super::edge::Edge;
use super::nodes::{Node, NodeOrVirtualNode, VirtualNode};
use super::parameters::{Parameter, ParameterOrTimeSeriesRef};
use crate::ConversionError;
use crate::data_tables::DataTable;
#[cfg(feature = "core")]
use crate::data_tables::{LoadedTableCollection, TableCollectionLoadError};
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::error::{
    ComponentConversionError, DuplicateNodeName, EdgeProblem, EdgeValidationError, NetworkProblem,
    NetworkValidationError,
};
use crate::metric::Metric;
use crate::metric_sets::MetricSet;
#[cfg(feature = "core")]
use crate::model::MultiNetworkTransfer;
use crate::outputs::Output;
use crate::time_series::TimeSeries;
#[cfg(feature = "core")]
use crate::time_series::{LoadedTimeSeriesCollection, LoadedTimeSeriesCollectionError};
use crate::v1::{ConversionData, TryIntoV2};
use crate::visit::{Owner, Reference, ReferenceMut, VisitMetrics, VisitPaths, VisitReferences};
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
        source: NetworkValidationError,
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
    #[cfg(feature = "core")]
    #[error("{0}")]
    LoadedTimeSeriesCollectionError(#[from] LoadedTimeSeriesCollectionError),
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
            NetworkSchemaBuildError::LoadedTimeSeriesCollectionError(e) => e.try_into(),
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
    #[error("Duplicate time series name found when merging networks: {0}")]
    DuplicateTimeSeriesName(String),
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
    pub time_series: &'a LoadedTimeSeriesCollection,
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
    pub time_series: Option<Vec<TimeSeries>>,
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

        for virtual_node in self.virtual_nodes.as_deref().into_iter().flatten() {
            virtual_node.visit_paths(visitor);
        }

        for parameter in self.parameters.as_deref().into_iter().flatten() {
            parameter.visit_paths(visitor);
        }

        for table in self.tables.as_deref().into_iter().flatten() {
            table.visit_paths(visitor);
        }

        for time_series in self.time_series.as_deref().into_iter().flatten() {
            time_series.visit_paths(visitor);
        }

        for metric_set in self.metric_sets.as_deref().into_iter().flatten() {
            metric_set.visit_paths(visitor);
        }

        for outputs in self.outputs.as_deref().into_iter().flatten() {
            outputs.visit_paths(visitor);
        }
    }
    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        for node in self.nodes.iter_mut() {
            node.visit_paths_mut(visitor);
        }

        for virtual_node in self.virtual_nodes.as_deref_mut().into_iter().flatten() {
            virtual_node.visit_paths_mut(visitor);
        }

        for parameter in self.parameters.as_deref_mut().into_iter().flatten() {
            parameter.visit_paths_mut(visitor);
        }

        for table in self.tables.as_deref_mut().into_iter().flatten() {
            table.visit_paths_mut(visitor);
        }

        for time_series in self.time_series.as_deref_mut().into_iter().flatten() {
            time_series.visit_paths_mut(visitor);
        }

        for metric_set in self.metric_sets.as_deref_mut().into_iter().flatten() {
            metric_set.visit_paths_mut(visitor);
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

impl VisitReferences for NetworkSchema {
    fn visit_references<F: FnMut(Reference<'_>)>(&self, visitor: &mut F) {
        for node in &self.nodes {
            node.visit_references(visitor);
        }

        for edge in &self.edges {
            edge.visit_references(visitor);
        }

        for virtual_node in self.virtual_nodes.as_deref().into_iter().flatten() {
            virtual_node.visit_references(visitor);
        }

        for parameter in self.parameters.as_deref().into_iter().flatten() {
            parameter.visit_references(visitor);
        }

        for metric_set in self.metric_sets.as_deref().into_iter().flatten() {
            metric_set.visit_references(visitor);
        }

        for output in self.outputs.as_deref().into_iter().flatten() {
            output.visit_references(visitor);
        }
    }

    fn visit_references_mut<F: FnMut(ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        for node in self.nodes.iter_mut() {
            node.visit_references_mut(visitor);
        }

        for edge in self.edges.iter_mut() {
            edge.visit_references_mut(visitor);
        }

        for virtual_node in self.virtual_nodes.as_deref_mut().into_iter().flatten() {
            virtual_node.visit_references_mut(visitor);
        }

        for parameter in self.parameters.as_deref_mut().into_iter().flatten() {
            parameter.visit_references_mut(visitor);
        }

        for metric_set in self.metric_sets.as_deref_mut().into_iter().flatten() {
            metric_set.visit_references_mut(visitor);
        }

        for output in self.outputs.as_deref_mut().into_iter().flatten() {
            output.visit_references_mut(visitor);
        }
    }
}

/// The names used by more than one of `items`, with how many use each, sorted by name.
fn duplicate_names<T>(items: Option<&[T]>, name_of: impl Fn(&T) -> &str) -> Vec<(String, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();

    for item in items.into_iter().flatten() {
        *counts.entry(name_of(item)).or_default() += 1;
    }

    let mut duplicates: Vec<(String, usize)> = counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, count)| (name.to_string(), count))
        .collect();

    // The hash map's order is random.
    duplicates.sort();

    duplicates
}

impl NetworkSchema {
    /// Visit every reference together with the top-level component holding it.
    pub fn visit_owned_references<F: FnMut(Owner<'_>, Reference<'_>)>(&self, visitor: &mut F) {
        for node in &self.nodes {
            let owner = Owner::Node(node.name());
            node.visit_references(&mut |reference| visitor(owner, reference));
        }

        for edge in &self.edges {
            let owner = Owner::Edge(edge);
            edge.visit_references(&mut |reference| visitor(owner, reference));
        }

        for virtual_node in self.virtual_nodes.as_deref().into_iter().flatten() {
            let owner = Owner::VirtualNode(virtual_node.name());
            virtual_node.visit_references(&mut |reference| visitor(owner, reference));
        }

        for parameter in self.parameters.as_deref().into_iter().flatten() {
            let owner = Owner::Parameter(parameter.name());
            parameter.visit_references(&mut |reference| visitor(owner, reference));
        }

        for metric_set in self.metric_sets.as_deref().into_iter().flatten() {
            let owner = Owner::MetricSet(&metric_set.name);
            metric_set.visit_references(&mut |reference| visitor(owner, reference));
        }

        for output in self.outputs.as_deref().into_iter().flatten() {
            let owner = Owner::Output(output.name());
            output.visit_references(&mut |reference| visitor(owner, reference));
        }
    }

    /// As [`NetworkSchema::visit_owned_references`], but able to rewrite each reference.
    pub fn visit_owned_references_mut<F: FnMut(Owner<'_>, ReferenceMut<'_>)>(&mut self, visitor: &mut F) {
        for node in self.nodes.iter_mut() {
            let owner_name = node.name().to_string();
            node.visit_references_mut(&mut |reference| visitor(Owner::Node(&owner_name), reference));
        }

        for edge in self.edges.iter_mut() {
            let owner_edge = edge.clone();
            edge.visit_references_mut(&mut |reference| visitor(Owner::Edge(&owner_edge), reference));
        }

        for virtual_node in self.virtual_nodes.as_deref_mut().into_iter().flatten() {
            let owner_name = virtual_node.name().to_string();
            virtual_node.visit_references_mut(&mut |reference| visitor(Owner::VirtualNode(&owner_name), reference));
        }

        for parameter in self.parameters.as_deref_mut().into_iter().flatten() {
            let owner_name = parameter.name().to_string();
            parameter.visit_references_mut(&mut |reference| visitor(Owner::Parameter(&owner_name), reference));
        }

        for metric_set in self.metric_sets.as_deref_mut().into_iter().flatten() {
            let owner_name = metric_set.name.clone();
            metric_set.visit_references_mut(&mut |reference| visitor(Owner::MetricSet(&owner_name), reference));
        }

        for output in self.outputs.as_deref_mut().into_iter().flatten() {
            let owner_name = output.name().to_string();
            output.visit_references_mut(&mut |reference| visitor(Owner::Output(&owner_name), reference));
        }
    }

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
        // We will use this to store any time series or parameters that are extracted from the v1 nodes
        let mut conversion_data = ConversionData::default();

        let mut nodes = Vec::with_capacity(v1.nodes.as_ref().map(|n| n.len()).unwrap_or_default());
        let mut virtual_nodes = Vec::with_capacity(v1.nodes.as_ref().map(|n| n.len()).unwrap_or_default());
        let mut parameters = Vec::new();
        let mut time_series = Vec::new();

        // Extract nodes and any time series data from the v1 nodes
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

        // Collect any parameters that have been replaced by time series
        // These references will be referred to by ParameterReferences elsewhere in the schema
        // We will update these references to TimeSeriesReferences later
        let mut time_series_refs = Vec::new();
        if let Some(params) = v1.parameters {
            // Reset the unnamed count for global parameters
            conversion_data.reset_count();
            for p in params {
                let result: Result<ParameterOrTimeSeriesRef, _> = p.try_into_v2(None, &mut conversion_data);
                match result {
                    Ok(p_or_t) => match p_or_t {
                        ParameterOrTimeSeriesRef::Parameter(p) => parameters.push(*p),
                        ParameterOrTimeSeriesRef::TimeSeries(t) => time_series_refs.push(t),
                    },
                    Err(e) => errors.push(*e),
                }
            }
        }

        // Finally add any extracted time series data to the time series list
        time_series.extend(conversion_data.time_series);
        parameters.extend(conversion_data.parameters);

        // Closure to update a parameter ref with a time series ref when names match.
        // We match on the original parameter name because the parameter name may have been changed
        let update_to_ts_ref = &mut |m: &mut Metric| {
            if let Metric::Parameter(p) = m {
                if let Some(converted_ts_ref) = time_series_refs.iter().find(|ts| ts.original_parameter_name == p.name)
                {
                    *m = Metric::TimeSeries(converted_ts_ref.ts_ref.clone());
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
        let time_series = if !time_series.is_empty() {
            Some(time_series)
        } else {
            None
        };

        (
            Self {
                nodes,
                edges,
                virtual_nodes,
                parameters,
                tables,
                time_series,
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

    pub fn get_time_series_by_name(&self, name: &str) -> Option<&TimeSeries> {
        match &self.time_series {
            Some(time_series) => time_series.iter().find(|t| t.name() == name),
            None => None,
        }
    }

    pub fn get_time_series_by_name_mut(&mut self, name: &str) -> Option<&mut TimeSeries> {
        match &mut self.time_series {
            Some(time_series) => time_series.iter_mut().find(|t| t.name() == name),
            None => None,
        }
    }

    pub fn time_series_exists(&self, name: &str) -> bool {
        self.get_time_series_by_name(name).is_some()
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

    /// Validate an edge against the network
    ///
    /// The following conditions are checked, with the first problem found being
    /// returned:
    ///
    /// - Both ends name an entry of `nodes`; a virtual node is not an edge end.
    /// - The two ends are different nodes.
    /// - Each slot is one that the node at that end has.
    /// - The `from_node` can provide flow, and the `to_node` can receive it.
    ///
    /// All but the second are checks `pywr-core` makes only while building. The second is a
    /// schema-level rule: a composite node such as a `Reservoir` is one node here, so
    /// `Reservoir[Spill] -> Reservoir` is a loop, whereas `pywr-core` sees the flattened network,
    /// where the storage and spill are separate nodes.
    ///
    /// An end whose name is used by more than one node resolves to the first of them.
    pub fn validate_edge(&self, edge: &Edge) -> Result<(), EdgeProblem> {
        let from_node = self.get_node_by_name(&edge.from_node).ok_or_else(|| {
            match self.get_virtual_node_by_name(&edge.from_node) {
                Some(virtual_node) => EdgeProblem::VirtualFromNode {
                    name: edge.from_node.clone(),
                    node_type: virtual_node.node_type(),
                },
                None => EdgeProblem::UnknownFromNode(edge.from_node.clone()),
            }
        })?;

        let to_node =
            self.get_node_by_name(&edge.to_node)
                .ok_or_else(|| match self.get_virtual_node_by_name(&edge.to_node) {
                    Some(virtual_node) => EdgeProblem::VirtualToNode {
                        name: edge.to_node.clone(),
                        node_type: virtual_node.node_type(),
                    },
                    None => EdgeProblem::UnknownToNode(edge.to_node.clone()),
                })?;

        if edge.from_node == edge.to_node {
            return Err(EdgeProblem::SelfEdge(edge.from_node.clone()));
        }

        if let Some(slot) = &edge.from_slot {
            from_node
                .validate_output_slot(Some(slot))
                .map_err(|_| EdgeProblem::UnknownFromSlot {
                    name: from_node.name().to_string(),
                    node_type: from_node.node_type(),
                    slot: slot.clone(),
                    valid: from_node.iter_output_slots().map(|slots| slots.collect()),
                })?;
        }

        if let Some(slot) = &edge.to_slot {
            to_node
                .validate_input_slot(Some(slot))
                .map_err(|_| EdgeProblem::UnknownToSlot {
                    name: to_node.name().to_string(),
                    node_type: to_node.node_type(),
                    slot: slot.clone(),
                    valid: to_node.iter_input_slots().map(|slots| slots.collect()),
                })?;
        }

        if !from_node.provides_outflow() {
            return Err(EdgeProblem::NoOutflow {
                name: from_node.name().to_string(),
                node_type: from_node.node_type(),
            });
        }

        if !to_node.accepts_inflow() {
            return Err(EdgeProblem::NoInflow {
                name: to_node.name().to_string(),
                node_type: to_node.node_type(),
            });
        }

        Ok(())
    }

    /// Validate the network schema and report every problem.
    ///
    /// This checks that the schema is unambiguous and that its edges could be made, not that the
    /// whole model can be built; use [`NetworkSchema::add_to_network`] for the latter. See
    /// [`NetworkProblem`] for the problems that are detected, and
    /// [`NetworkSchema::validate_edge`] for the edge rules in particular.
    pub fn validate(&self) -> Result<(), NetworkValidationError> {
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

        let invalid_edges: Vec<EdgeValidationError> = self
            .edges
            .iter()
            .filter_map(|edge| {
                self.validate_edge(edge).err().map(|problem| EdgeValidationError {
                    edge: edge.clone(),
                    problem,
                })
            })
            .collect();

        // The duplicates come out of the hash map in a random order.
        duplicates.sort_by(|a, b| a.name.cmp(&b.name));

        let problems: Vec<NetworkProblem> = duplicates
            .into_iter()
            .map(NetworkProblem::DuplicateNodeName)
            .chain(
                duplicate_names(self.parameters.as_deref(), Parameter::name)
                    .into_iter()
                    .map(|(name, count)| NetworkProblem::DuplicateParameterName { name, count }),
            )
            .chain(
                duplicate_names(self.tables.as_deref(), DataTable::name)
                    .into_iter()
                    .map(|(name, count)| NetworkProblem::DuplicateTableName { name, count }),
            )
            .chain(
                duplicate_names(self.time_series.as_deref(), TimeSeries::name)
                    .into_iter()
                    .map(|(name, count)| NetworkProblem::DuplicateTimeSeriesName { name, count }),
            )
            .chain(
                duplicate_names(self.metric_sets.as_deref(), |metric_set| metric_set.name.as_str())
                    .into_iter()
                    .map(|(name, count)| NetworkProblem::DuplicateMetricSetName { name, count }),
            )
            .chain(invalid_edges.into_iter().map(NetworkProblem::InvalidEdge))
            .collect();

        if problems.is_empty() {
            Ok(())
        } else {
            Err(NetworkValidationError { name: None, problems })
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
    ) -> Result<(LoadedTableCollection, LoadedTimeSeriesCollection), NetworkSchemaBuildError> {
        // Reject an invalid schema before doing any work to build it.
        self.validate()
            .map_err(|source| NetworkSchemaBuildError::Validation { source })?;

        let tables = LoadedTableCollection::from_schema(self.tables.as_deref(), data_path)?;
        let time_series = LoadedTimeSeriesCollection::from_schema(self.time_series.as_deref(), data_path)?;

        let args = LoadArgs {
            schema: self,
            domain,
            tables: &tables,
            time_series: &time_series,
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

        // Add all the local parameters from the nodes and the virtual nodes.
        let node_parameters = self.nodes.iter().map(|node| (node.name(), node.local_parameters()));
        let virtual_node_parameters = self
            .virtual_nodes
            .iter()
            .flatten()
            .map(|node| (node.name(), node.local_parameters()));

        for (parent, local_parameters) in node_parameters.chain(virtual_node_parameters) {
            for parameter in local_parameters.into_iter().flatten() {
                parameter
                    .add_to_network(network_builder, &args, Some(parent))
                    .map_err(|source| NetworkSchemaBuildError::AddLocalParameterError {
                        parent: parent.to_string(),
                        name: parameter.name().to_string(),
                        source: Box::new(source),
                    })?;
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

        Ok((tables, time_series))
    }

    /// Merge another [`NetworkSchema`] into this one.
    ///
    /// This will combine the nodes, virtual nodes, edges, and parameters of both networks.
    /// If there are any duplicate node or parameter names, an error will be returned. However,
    /// placeholder types (e.g. [`crate::nodes::PlaceholderNode`]) are replaced.
    ///
    /// Metric sets are merged by name, with the metrics of any metric sets with the same name being
    /// combined. Other information in the metric set (e.g. filters) is **not** merged.
    ///
    /// If an error occurs during the merge, the network will be left in a partially merged state.
    /// It is recommended to clone the network before merging if you want to keep the original network
    /// intact.
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
                        // Check if the node name exists as a regular node
                        if self.get_node_index_by_name(v_node.name()).is_some() {
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

        // Merge time series
        if let Some(other_time_series) = other.time_series {
            for ts in other_time_series {
                match self.get_time_series_by_name_mut(ts.name()) {
                    Some(existing_ts) => {
                        if existing_ts.is_placeholder() {
                            *existing_ts = ts;
                        } else {
                            return Err(NetworkMergeError::DuplicateTimeSeriesName(ts.name().to_string()));
                        }
                    }
                    None => {
                        self.time_series.get_or_insert_default().push(ts);
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
    use crate::error::{DuplicateNodeName, EdgeProblem, EdgeValidationError, NetworkProblem};
    use crate::nodes::{NodeSlot, NodeType, VirtualNodeType};
    use crate::visit::VisitPaths;
    use std::path::PathBuf;
    use std::str::FromStr;

    /// Return the problems reported by [`NetworkSchema::validate`], or panic if it succeeded.
    fn expect_problems(network: &NetworkSchema) -> Vec<NetworkProblem> {
        match network.validate() {
            Err(error) => {
                assert_eq!(error.name, None, "A network validated on its own has no name");
                assert!(!error.problems.is_empty(), "An error must hold at least one problem");
                error.problems
            }
            Ok(()) => panic!("Expected validation to fail, but it succeeded"),
        }
    }

    /// Return the duplicates reported by [`NetworkSchema::validate`], or panic if it reported
    /// anything else.
    fn expect_duplicates(network: &NetworkSchema) -> Vec<DuplicateNodeName> {
        expect_problems(network)
            .into_iter()
            .map(|problem| match problem {
                NetworkProblem::DuplicateNodeName(duplicate) => duplicate,
                other => panic!("Expected only duplicate node names, but got: {other:?}"),
            })
            .collect()
    }

    /// Return the invalid edges reported by [`NetworkSchema::validate`] as `(edge, problem)`
    /// pairs, or panic if it reported anything else.
    fn expect_invalid_edges(network: &NetworkSchema) -> Vec<(String, EdgeProblem)> {
        expect_problems(network)
            .into_iter()
            .map(|problem| match problem {
                NetworkProblem::InvalidEdge(e) => (e.edge.to_string(), e.problem),
                other => panic!("Expected only invalid edges, but got: {other:?}"),
            })
            .collect()
    }

    fn parse_network(data: &str) -> NetworkSchema {
        NetworkSchema::from_str(data).expect("Failed to parse test network JSON")
    }

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

    /// Every duplicate is reported, not just the first one found. Nodes and virtual nodes are a
    /// single name-space, so a name shared between the two lists is a duplicate too.
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

    /// A network with an edge for every [`EdgeProblem`] that does not need a virtual node, and
    /// into both node types that cannot receive flow.
    const NETWORK_WITH_INVALID_EDGES: &str = r#"
    {
        "nodes": [
            { "meta": { "name": "supply" }, "type": "Input" },
            { "meta": { "name": "catchment" }, "type": "Catchment" },
            { "meta": { "name": "link" }, "type": "Link" },
            { "meta": { "name": "demand" }, "type": "Output" }
        ],
        "edges": [
            { "from_node": "link", "to_node": "supply" },
            { "from_node": "link", "to_node": "missing" },
            { "from_node": "absent", "to_node": "link" },
            { "from_node": "demand", "to_node": "link" },
            { "from_node": "link", "from_slot": { "type": "Spill" }, "to_node": "demand" },
            { "from_node": "link", "to_node": "link" },
            { "from_node": "link", "to_node": "demand", "to_slot": { "type": "Storage" } },
            { "from_node": "link", "to_node": "catchment" }
        ]
    }
    "#;

    /// Every invalid edge is reported, in the order the edges are listed.
    #[test]
    fn test_validate_reports_all_invalid_edges() {
        let network = parse_network(NETWORK_WITH_INVALID_EDGES);

        assert_eq!(
            expect_invalid_edges(&network),
            vec![
                (
                    "link->supply".to_string(),
                    EdgeProblem::NoInflow {
                        name: "supply".to_string(),
                        node_type: NodeType::Input,
                    }
                ),
                (
                    "link->missing".to_string(),
                    EdgeProblem::UnknownToNode("missing".to_string())
                ),
                (
                    "absent->link".to_string(),
                    EdgeProblem::UnknownFromNode("absent".to_string())
                ),
                (
                    "demand->link".to_string(),
                    EdgeProblem::NoOutflow {
                        name: "demand".to_string(),
                        node_type: NodeType::Output,
                    }
                ),
                (
                    "link[Spill]->demand".to_string(),
                    EdgeProblem::UnknownFromSlot {
                        name: "link".to_string(),
                        node_type: NodeType::Link,
                        slot: NodeSlot::Spill,
                        valid: None,
                    }
                ),
                ("link->link".to_string(), EdgeProblem::SelfEdge("link".to_string())),
                (
                    "link->demand[Storage]".to_string(),
                    EdgeProblem::UnknownToSlot {
                        name: "demand".to_string(),
                        node_type: NodeType::Output,
                        slot: NodeSlot::Storage,
                        valid: None,
                    }
                ),
                (
                    "link->catchment".to_string(),
                    EdgeProblem::NoInflow {
                        name: "catchment".to_string(),
                        node_type: NodeType::Catchment,
                    }
                ),
            ]
        );
    }

    /// Edges connect only entries of `nodes`. A virtual node at either end is reported as the
    /// virtual node it is, rather than as a name the network does not define.
    #[test]
    fn test_validate_rejects_virtual_node_as_edge_end() {
        let network = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "supply" }, "type": "Input" },
                    { "meta": { "name": "demand" }, "type": "Output" }
                ],
                "virtual_nodes": [
                    {
                        "meta": { "name": "licence" },
                        "type": "Aggregated",
                        "nodes": [{ "name": "demand" }]
                    }
                ],
                "edges": [
                    { "from_node": "supply", "to_node": "demand" },
                    { "from_node": "licence", "to_node": "demand" },
                    { "from_node": "supply", "to_node": "licence" }
                ]
            }
            "#,
        );

        assert_eq!(
            expect_invalid_edges(&network),
            vec![
                (
                    "licence->demand".to_string(),
                    EdgeProblem::VirtualFromNode {
                        name: "licence".to_string(),
                        node_type: VirtualNodeType::Aggregated,
                    }
                ),
                (
                    "supply->licence".to_string(),
                    EdgeProblem::VirtualToNode {
                        name: "licence".to_string(),
                        node_type: VirtualNodeType::Aggregated,
                    }
                ),
            ]
        );
    }

    /// A node cannot connect to itself even through a slot, although the flattened network that
    /// `pywr-core` builds would accept the edge.
    #[test]
    fn test_validate_rejects_self_edge_through_a_slot() {
        let network = parse_network(
            r#"
            {
                "nodes": [
                    {
                        "meta": { "name": "reservoir" },
                        "type": "Reservoir",
                        "max_volume": { "type": "Literal", "value": 100.0 },
                        "initial_volume": { "type": "Proportional", "proportion": 1.0 },
                        "spill": "LinkNode"
                    }
                ],
                "edges": [
                    { "from_node": "reservoir", "from_slot": { "type": "Spill" }, "to_node": "reservoir" }
                ]
            }
            "#,
        );

        assert_eq!(
            expect_invalid_edges(&network),
            vec![(
                "reservoir[Spill]->reservoir".to_string(),
                EdgeProblem::SelfEdge("reservoir".to_string())
            )]
        );
    }

    /// A slot is checked against the node's own configuration, not just its type: a `Reservoir`
    /// only has a `Spill` output slot when its spill is a link node.
    #[test]
    fn test_validate_checks_slot_against_node_configuration() {
        let network = parse_network(
            r#"
            {
                "nodes": [
                    {
                        "meta": { "name": "with-spill" },
                        "type": "Reservoir",
                        "max_volume": { "type": "Literal", "value": 100.0 },
                        "initial_volume": { "type": "Proportional", "proportion": 1.0 },
                        "spill": "LinkNode"
                    },
                    {
                        "meta": { "name": "without-spill" },
                        "type": "Reservoir",
                        "max_volume": { "type": "Literal", "value": 100.0 },
                        "initial_volume": { "type": "Proportional", "proportion": 1.0 }
                    },
                    { "meta": { "name": "river" }, "type": "River" }
                ],
                "edges": [
                    { "from_node": "with-spill", "from_slot": { "type": "Spill" }, "to_node": "river" },
                    { "from_node": "without-spill", "from_slot": { "type": "Spill" }, "to_node": "river" }
                ]
            }
            "#,
        );

        assert_eq!(
            expect_invalid_edges(&network),
            vec![(
                "without-spill[Spill]->river".to_string(),
                EdgeProblem::UnknownFromSlot {
                    name: "without-spill".to_string(),
                    node_type: NodeType::Reservoir,
                    slot: NodeSlot::Spill,
                    valid: Some(vec![NodeSlot::Storage]),
                }
            )]
        );
    }

    /// A slot problem names the slots the node does have, so that a mistyped slot can be
    /// corrected without reading the node's definition; a node with no slots of that kind says so.
    #[test]
    fn test_invalid_slot_problem_lists_the_slots_the_node_has() {
        let network = parse_network(
            r#"
            {
                "nodes": [
                    {
                        "meta": { "name": "split" },
                        "type": "RiverSplitWithGauge",
                        "splits": [
                            { "factor": { "type": "Literal", "value": 0.5 } },
                            { "factor": { "type": "Literal", "value": 0.5 }, "slot_name": "to-supply" }
                        ]
                    },
                    { "meta": { "name": "river" }, "type": "Link" },
                    { "meta": { "name": "demand" }, "type": "Output" }
                ],
                "edges": [
                    { "from_node": "split", "from_slot": { "type": "Split", "position": 5 }, "to_node": "river" },
                    { "from_node": "river", "from_slot": { "type": "Spill" }, "to_node": "demand" }
                ]
            }
            "#,
        );

        let messages: Vec<String> = expect_invalid_edges(&network)
            .iter()
            .map(|(_, problem)| problem.to_string())
            .collect();

        assert_eq!(
            messages,
            vec![
                "The `RiverSplitWithGauge` node `split` has no output slot `Split[5]`. Its output slots are: `River`, `Split[0]`, `User[to-supply]`.",
                "The `Link` node `river` has no output slot `Spill`. Nodes of this type have no output slots.",
            ]
        );
    }

    /// A duplicated name does not stop the edges being checked: both problems are reported, the
    /// duplicate first.
    #[test]
    fn test_validate_reports_duplicate_names_and_edges_together() {
        let network = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "link" }, "type": "Link" },
                    { "meta": { "name": "link" }, "type": "Link" }
                ],
                "edges": [
                    { "from_node": "link", "to_node": "missing" }
                ]
            }
            "#,
        );

        let problems = expect_problems(&network);

        assert_eq!(
            problems,
            vec![
                NetworkProblem::DuplicateNodeName(DuplicateNodeName {
                    name: "link".to_string(),
                    nodes: 2,
                    virtual_nodes: 0,
                }),
                NetworkProblem::InvalidEdge(EdgeValidationError {
                    edge: network.edges[0].clone(),
                    problem: EdgeProblem::UnknownToNode("missing".to_string()),
                }),
            ]
        );

        assert_eq!(
            network.validate().unwrap_err().report().to_string(),
            "The network has 2 problem(s):\n\
             - The name `link` is used by 2 node(s) and 0 virtual node(s), but each name must be unique.\n\
             - The edge `link->missing` is invalid. There is no node named `missing` to connect to."
        );
    }

    /// Every list is checked, and every duplicate is reported in the documented order. A
    /// placeholder entry counts like any other, and a name shared across lists is not a duplicate.
    #[test]
    fn test_validate_reports_duplicate_names_in_every_list() {
        let network = parse_network(
            r#"
            {
                "nodes": [
                    { "meta": { "name": "link" }, "type": "Link" },
                    { "meta": { "name": "link" }, "type": "Link" },
                    { "meta": { "name": "shared" }, "type": "Link" }
                ],
                "edges": [
                    { "from_node": "link", "to_node": "missing" }
                ],
                "parameters": [
                    { "meta": { "name": "p2" }, "type": "Constant", "value": { "type": "Literal", "value": 1.0 } },
                    { "meta": { "name": "p1" }, "type": "Constant", "value": { "type": "Literal", "value": 1.0 } },
                    { "meta": { "name": "p2" }, "type": "Placeholder" },
                    { "meta": { "name": "p1" }, "type": "Constant", "value": { "type": "Literal", "value": 2.0 } },
                    { "meta": { "name": "p2" }, "type": "Constant", "value": { "type": "Literal", "value": 3.0 } },
                    { "meta": { "name": "shared" }, "type": "Constant", "value": { "type": "Literal", "value": 1.0 } }
                ],
                "tables": [
                    { "meta": { "name": "tbl" }, "format": "Placeholder" },
                    { "meta": { "name": "tbl" }, "type": "Scalar", "format": "CSV", "lookup": { "type": "Row", "cols": 1 }, "url": "tbl.csv" },
                    { "meta": { "name": "shared" }, "format": "Placeholder" }
                ],
                "time_series": [
                    { "meta": { "name": "ts" }, "type": "Polars", "time_col": "date", "path": "ts.csv" },
                    { "meta": { "name": "ts" }, "type": "Placeholder" },
                    { "meta": { "name": "shared" }, "type": "Placeholder" }
                ],
                "metric_sets": [
                    { "name": "ms", "filters": { "all_nodes": true } },
                    { "name": "ms", "filters": { "all_virtual_nodes": true } },
                    { "name": "shared", "filters": { "all_nodes": true } }
                ]
            }
            "#,
        );

        let problems = expect_problems(&network);

        assert_eq!(
            problems,
            vec![
                NetworkProblem::DuplicateNodeName(DuplicateNodeName {
                    name: "link".to_string(),
                    nodes: 2,
                    virtual_nodes: 0,
                }),
                NetworkProblem::DuplicateParameterName {
                    name: "p1".to_string(),
                    count: 2,
                },
                NetworkProblem::DuplicateParameterName {
                    name: "p2".to_string(),
                    count: 3,
                },
                NetworkProblem::DuplicateTableName {
                    name: "tbl".to_string(),
                    count: 2,
                },
                NetworkProblem::DuplicateTimeSeriesName {
                    name: "ts".to_string(),
                    count: 2,
                },
                NetworkProblem::DuplicateMetricSetName {
                    name: "ms".to_string(),
                    count: 2,
                },
                NetworkProblem::InvalidEdge(EdgeValidationError {
                    edge: network.edges[0].clone(),
                    problem: EdgeProblem::UnknownToNode("missing".to_string()),
                }),
            ]
        );

        assert_eq!(
            network.validate().unwrap_err().report().to_string(),
            "The network has 7 problem(s):\n\
             - The name `link` is used by 2 node(s) and 0 virtual node(s), but each name must be unique.\n\
             - The name `p1` is used by 2 parameters, but each name must be unique.\n\
             - The name `p2` is used by 3 parameters, but each name must be unique.\n\
             - The name `tbl` is used by 2 tables, but each name must be unique.\n\
             - The name `ts` is used by 2 time series, but each name must be unique.\n\
             - The name `ms` is used by 2 metric sets, but each name must be unique.\n\
             - The edge `link->missing` is invalid. There is no node named `missing` to connect to."
        );
    }

    /// However many problems there are, `Display` stays a single line, while the report lists
    /// every one of them.
    #[test]
    fn test_validate_display_summarises_and_report_lists_every_problem() {
        let count = 13;
        let edges = (0..count)
            .map(|i| format!(r#"{{ "from_node": "link", "to_node": "missing-{i:02}" }}"#))
            .collect::<Vec<_>>()
            .join(", ");

        let network = parse_network(&format!(
            r#"{{ "nodes": [{{ "meta": {{ "name": "link" }}, "type": "Link" }}], "edges": [{edges}] }}"#
        ));

        let error = network.validate().unwrap_err();

        assert_eq!(error.to_string(), "The network has 13 problem(s).");

        // The summary, then one line per problem, down to the last edge listed.
        let report = error.report().to_string();
        let lines: Vec<&str> = report.lines().collect();

        assert_eq!(lines.len(), 1 + count);
        assert_eq!(lines[0], "The network has 13 problem(s):");
        assert!(lines[count].contains("`missing-12`"));
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
    fn test_merge_replaces_placeholder_time_series() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "time_series": [
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
                "time_series": [
                    { "type": "Polars", "meta": { "name": "ts-shared" }, "path": "time-series.csv" }
                ]
            }
            "#,
        );

        base.merge(other).expect("Merge should replace placeholder time series");

        let merged = base
            .get_time_series_by_name("ts-shared")
            .expect("TimeSeries should exist after merge");
        assert!(!merged.is_placeholder());
    }

    #[test]
    fn test_merge_rejects_duplicate_time_series_name() {
        let mut base = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "time_series": [
                    { "type": "Polars", "meta": { "name": "ts-shared" }, "path": "time-series.csv" }
                ]
            }
            "#,
        );

        let other = parse_network(
            r#"
            {
                "nodes": [],
                "edges": [],
                "time_series": [
                    { "type": "Polars", "meta": { "name": "ts-shared" }, "path": "other.csv" }
                ]
            }
            "#,
        );

        let err = base
            .merge(other)
            .expect_err("Merge should reject duplicate time series names");
        assert!(matches!(err, NetworkMergeError::DuplicateTimeSeriesName(name) if name == "ts-shared"));
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

    /// A network holding a path in every list that can hold one, each named for its location so
    /// that a missed one is identifiable. The metric set's aggregator nests a second one in `child`.
    const NETWORK_WITH_PATHS: &str = r#"
    {
        "nodes": [
            {
                "meta": { "name": "supply" },
                "type": "Input",
                "parameters": [
                    {
                        "meta": { "name": "supply-local" },
                        "type": "Python",
                        "source": { "type": "Path", "path": "node-local-parameter.py" },
                        "object": { "type": "Class", "class": "FloatParameter" }
                    }
                ]
            },
            { "meta": { "name": "demand" }, "type": "Output" }
        ],
        "edges": [
            { "from_node": "supply", "to_node": "demand" }
        ],
        "virtual_nodes": [
            {
                "meta": { "name": "licence" },
                "type": "Aggregated",
                "nodes": [{ "name": "supply" }],
                "parameters": [
                    {
                        "meta": { "name": "licence-local" },
                        "type": "Python",
                        "source": { "type": "Path", "path": "virtual-node-local-parameter.py" },
                        "object": { "type": "Class", "class": "FloatParameter" }
                    }
                ]
            }
        ],
        "parameters": [
            {
                "meta": { "name": "global" },
                "type": "Python",
                "source": { "type": "Path", "path": "global-parameter.py" },
                "object": { "type": "Class", "class": "FloatParameter" }
            }
        ],
        "tables": [
            {
                "meta": { "name": "t1" },
                "type": "Scalar",
                "format": "CSV",
                "lookup": { "type": "Row", "cols": 1 },
                "url": "table.csv"
            },
            {
                "meta": { "name": "t2" },
                "format": "Placeholder"
            }
        ],
        "time_series": [
            { "type": "Polars", "meta": { "name": "ts1" }, "path": "timeseries.csv" }
        ],
        "metric_sets": [
            {
                "name": "ms1",
                "metrics": [{ "type": "Node", "name": "demand" }],
                "aggregator": {
                    "func": {
                        "type": "Python",
                        "source": { "type": "Path", "path": "aggregation.py" },
                        "object": "agg"
                    },
                    "child": {
                        "func": {
                            "type": "Python",
                            "source": { "type": "Path", "path": "child-aggregation.py" },
                            "object": "child_agg"
                        }
                    }
                }
            }
        ],
        "outputs": [
            { "name": "csv-out", "type": "CSV", "format": "Long", "filename": "output.csv", "metric_set": "ms1" }
        ]
    }
    "#;

    /// Every path in [`NETWORK_WITH_PATHS`], sorted. The placeholder table holds none.
    const EXPECTED_PATHS: [&str; 8] = [
        "aggregation.py",
        "child-aggregation.py",
        "global-parameter.py",
        "node-local-parameter.py",
        "output.csv",
        "table.csv",
        "timeseries.csv",
        "virtual-node-local-parameter.py",
    ];

    /// Collect every visited path, sorted, so the assertions do not depend on the walk order.
    fn collect_paths(network: &NetworkSchema) -> Vec<String> {
        let mut paths: Vec<String> = Vec::new();
        network.visit_paths(&mut |path| paths.push(path.to_string_lossy().into_owned()));
        paths.sort();
        paths
    }

    /// Every list that can hold a path should be reached by the visitor.
    #[test]
    fn test_visit_paths_reaches_every_path() {
        let network = parse_network(NETWORK_WITH_PATHS);

        assert_eq!(collect_paths(&network), EXPECTED_PATHS);
    }

    /// The mutable visitor should hand out borrows into the schema, so that a path it rewrites
    /// is replaced in the network itself.
    #[test]
    fn test_visit_paths_mut_rewrites_every_path() {
        const NEW_PATH: &str = "rebased/on/another/directory";

        let mut network = parse_network(NETWORK_WITH_PATHS);

        let mut count = 0;
        network.visit_paths_mut(&mut |path| {
            *path = PathBuf::from(NEW_PATH);
            count += 1;
        });
        assert_eq!(count, EXPECTED_PATHS.len());

        // Any path left un-rewritten is one the mutable visitor failed to reach.
        assert_eq!(collect_paths(&network), [NEW_PATH; EXPECTED_PATHS.len()]);
    }
}
