mod merge;
mod validate;

use super::edge::Edge;
use super::nodes::{Node, NodeOrVirtualNode, VirtualNode};
use super::parameters::{Parameter, ParameterOrTimeSeriesRef};
use crate::data_tables::DataTable;
#[cfg(feature = "core")]
use crate::data_tables::{LoadedTableCollection, TableCollectionLoadError};
use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::meta::{ComponentMeta, ComponentProvenance, ProvenanceSource};
use crate::metric::Metric;
use crate::metric_sets::MetricSet;
#[cfg(feature = "core")]
use crate::model::MultiNetworkTransfer;
use crate::outputs::Output;
use crate::time_series::TimeSeries;
#[cfg(feature = "core")]
use crate::time_series::{LoadedTimeSeriesCollection, LoadedTimeSeriesCollectionError};
use crate::v1::{ConversionData, TryIntoV2};
#[cfg(feature = "core")]
use crate::validation::NetworkValidationError;
use crate::visit::{Owner, Reference, ReferenceMut, VisitMetrics, VisitPaths, VisitReferences};
use crate::{ConversionError, FileProvider, FileSystem};
pub use merge::{NetworkMergeError, NetworkMergeOptions};
#[cfg(all(feature = "core", feature = "pyo3"))]
use pyo3::PyErr;
#[cfg(feature = "pyo3")]
use pyo3::pyclass;
#[cfg(feature = "core")]
use pywr_core::models::ModelDomain;
use pywr_schema_macros::skip_serializing_none;
use pywr_v1_schema::nodes::{CoreNode as CoreNodeV1, Node as NodeV1};
use schemars::JsonSchema;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
use thiserror::Error;

/// Error type for reading a [`NetworkSchema`] network from a file or string.
#[derive(Error, Debug)]
pub enum NetworkSchemaReadError {
    #[error("IO error on path `{path}`.")]
    IO {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("JSON error deserialising network.")]
    Json(#[from] serde_json::Error),
}

/// Error type for building a `pywr_core::PywrNetwork` network from a schema ([`NetworkSchema`]).
#[cfg(feature = "core")]
#[derive(Error, Debug)]
pub enum NetworkSchemaBuildError {
    #[error("Network schema validation failed.")]
    Validation {
        #[source]
        source: NetworkValidationError,
    },
    #[error("Circular node reference(s) found.")]
    CircularNodeReference,
    #[error("Circular parameters reference(s) found. Unable to load the following parameters: {0:?}")]
    CircularParameterReference(Vec<String>),
    #[error("Failed to add node `{name}` to the model.")]
    AddNodeError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to add virtual node `{name}` to the model.")]
    AddVirtualNodeError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to set constraints for node `{name}`.")]
    SetNodeConstraintsError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to set constraints for virtual node `{name}`.")]
    SetVirtualNodeConstraintsError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to add edge from `{from_node}` to `{to_node}`.")]
    AddEdgeError {
        from_node: String,
        to_node: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to add parameter `{name}` to the model.")]
    AddParameterError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to add local parameter from node `{parent}` with `{name}` to the model.")]
    AddLocalParameterError {
        name: String,
        parent: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to add metric set with name `{name}` to the model.")]
    AddMetricSetError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to add output with name `{name}` to the model.")]
    AddOutputError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to load table data.")]
    TableLoadError(#[from] TableCollectionLoadError),
    #[cfg(feature = "core")]
    #[error("Failed to load time-series data.")]
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

impl NetworkSchema {
    /// Label definitions in this network with their source.
    pub fn set_provenance(&mut self, source: ProvenanceSource) {
        let provenance = ComponentProvenance::new(source);
        for node in &mut self.nodes {
            node.meta_mut().set_provenance(provenance.clone());
            for param in node.local_parameters_mut().into_iter().flatten() {
                param.meta_mut().set_provenance(provenance.clone());
            }
        }
        for node in self.virtual_nodes.iter_mut().flatten() {
            node.meta_mut().set_provenance(provenance.clone());
            for param in node.local_parameters_mut().into_iter().flatten() {
                param.meta_mut().set_provenance(provenance.clone());
            }
        }
        for edge in &mut self.edges {
            edge.meta.get_or_insert_default().set_provenance(provenance.clone());
        }
        for param in self.parameters.iter_mut().flatten() {
            param.meta_mut().set_provenance(provenance.clone());
        }
        for table in self.tables.iter_mut().flatten() {
            table.meta_mut().set_provenance(provenance.clone());
        }
        for ts in self.time_series.iter_mut().flatten() {
            ts.meta_mut().set_provenance(provenance.clone());
        }
        for ms in self.metric_sets.iter_mut().flatten() {
            ms.meta_mut().set_provenance(provenance.clone());
        }
        for output in self.outputs.iter_mut().flatten() {
            output.meta_mut().set_provenance(provenance.clone());
        }
    }

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
            let owner = Owner::MetricSet(metric_set.name());
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
            let owner_name = metric_set.name().to_string();
            metric_set.visit_references_mut(&mut |reference| visitor(Owner::MetricSet(&owner_name), reference));
        }

        for output in self.outputs.as_deref_mut().into_iter().flatten() {
            let owner_name = output.name().to_string();
            output.visit_references_mut(&mut |reference| visitor(Owner::Output(&owner_name), reference));
        }
    }

    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, NetworkSchemaReadError> {
        Self::from_files(&FileSystem, path.as_ref())
    }

    /// Read the network at `path` as `files` opens it.
    pub(crate) fn from_files(files: &dyn FileProvider, path: &Path) -> Result<Self, NetworkSchemaReadError> {
        let mut data = String::new();
        files
            .open(path)
            .and_then(|mut file| file.read_to_string(&mut data))
            .map_err(|source| NetworkSchemaReadError::IO {
                path: path.to_path_buf(),
                source,
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
            if let Metric::Parameter(p) = m
                && let Some(converted_ts_ref) = time_series_refs.iter().find(|ts| ts.original_parameter_name == p.name)
            {
                *m = Metric::TimeSeries(converted_ts_ref.ts_ref.clone());
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
            Some(metric_sets) => metric_sets.iter().find(|ms| ms.name() == name),
            None => None,
        }
    }

    pub fn get_metric_set_by_name_mut(&mut self, name: &str) -> Option<&mut MetricSet> {
        match &mut self.metric_sets {
            Some(metric_sets) => metric_sets.iter_mut().find(|ms| ms.name() == name),
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

    #[cfg(feature = "core")]
    pub fn add_to_network(
        &self,
        network_builder: &mut pywr_core::network::NetworkBuilder,
        domain: &ModelDomain,
        files: &dyn FileProvider,
        data_path: Option<&Path>,
        output_path: Option<&Path>,
        inter_network_transfers: &[MultiNetworkTransfer],
    ) -> Result<(LoadedTableCollection, LoadedTimeSeriesCollection), NetworkSchemaBuildError> {
        // Reject an invalid schema before doing any work to build it.
        self.validate()
            .map_err(|source| NetworkSchemaBuildError::Validation { source })?;

        let tables = LoadedTableCollection::from_schema(self.tables.as_deref(), files, data_path)?;
        let time_series = LoadedTimeSeriesCollection::from_schema(self.time_series.as_deref(), files, data_path)?;

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
                        name: metric_set.name().to_string(),
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
}

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema, Display, EnumDiscriminants)]
#[serde(untagged)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(NetworkSchemaRefType))]
pub enum NetworkSchemaRef {
    Path(PathBuf),
    Inline(NetworkSchema),
}

#[cfg(test)]
mod tests {
    use super::NetworkSchema;
    use crate::meta::{ComponentMeta, ProvenanceSource};
    use crate::visit::VisitPaths;
    use std::path::PathBuf;
    use std::str::FromStr;

    pub(super) fn parse_network(data: &str) -> NetworkSchema {
        NetworkSchema::from_str(data).expect("Failed to parse test network JSON")
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
                "meta": { "name": "ms1" },
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
            { "meta": { "name": "csv-out" }, "type": "CSV", "format": "Long", "filename": "output.csv", "metric_set": "ms1" }
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

    #[test]
    fn provenance_covers_all_component_metadata_without_becoming_a_data_path() {
        let mut network = parse_network(NETWORK_WITH_PATHS);
        let source = ProvenanceSource {
            file: "sets/network.json".into(),
            network_set: Some("sets".into()),
        };
        assert!(
            serde_json::to_value(&network).unwrap()["nodes"][0]["meta"]
                .get("provenance")
                .is_none()
        );
        network.set_provenance(source.clone());
        let origin = |meta: &dyn ComponentMeta| meta.provenance().unwrap().origin.clone();
        assert_eq!(origin(network.nodes[0].meta()), source);
        assert_eq!(origin(network.nodes[0].local_parameters().unwrap()[0].meta()), source);
        assert_eq!(origin(network.virtual_nodes.as_ref().unwrap()[0].meta()), source);
        assert_eq!(
            origin(network.virtual_nodes.as_ref().unwrap()[0].local_parameters().unwrap()[0].meta()),
            source
        );
        assert_eq!(origin(network.edges[0].meta().unwrap()), source);
        assert_eq!(origin(network.parameters.as_ref().unwrap()[0].meta()), source);
        assert_eq!(origin(network.tables.as_ref().unwrap()[0].meta()), source);
        assert_eq!(origin(network.time_series.as_ref().unwrap()[0].meta()), source);
        assert_eq!(origin(network.metric_sets.as_ref().unwrap()[0].meta()), source);
        assert_eq!(origin(network.outputs.as_ref().unwrap()[0].meta()), source);
        assert_eq!(collect_paths(&network), EXPECTED_PATHS);
        network.visit_paths_mut(&mut |path| *path = PathBuf::from("updated"));
        assert_eq!(origin(network.nodes[0].meta()), source);
        let round_trip: NetworkSchema = serde_json::from_value(serde_json::to_value(network).unwrap()).unwrap();
        assert_eq!(origin(round_trip.nodes[0].meta()), source);
    }

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
