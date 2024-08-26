use super::edge::Edge;
use super::nodes::Node;
use super::parameters::{convert_parameter_v1_to_v2, Parameter};
use crate::data_tables::DataTable;
#[cfg(feature = "core")]
use crate::data_tables::LoadedTableCollection;
use crate::error::{ConversionError, SchemaError};
use crate::metric::{Metric, TimeseriesColumns, TimeseriesReference};
use crate::metric_sets::MetricSet;
use crate::nodes::NodeAndTimeseries;
use crate::outputs::Output;
#[cfg(feature = "core")]
use crate::timeseries::LoadedTimeseriesCollection;
use crate::timeseries::{convert_from_v1_data, Timeseries};
use crate::visit::{VisitMetrics, VisitPaths};
#[cfg(feature = "core")]
use chrono::NaiveTime;
use chrono::{NaiveDate, NaiveDateTime};
#[cfg(feature = "core")]
use pywr_core::{models::ModelDomain, timestep::TimestepDuration, PywrError};
use schemars::JsonSchema;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
pub struct Metadata {
    pub title: String,
    pub description: Option<String>,
    pub minimum_version: Option<String>,
}

impl Default for Metadata {
    fn default() -> Self {
        Self {
            title: "Untitled model".to_string(),
            description: None,
            minimum_version: None,
        }
    }
}

impl TryFrom<pywr_v1_schema::model::Metadata> for Metadata {
    type Error = ConversionError;

    fn try_from(v1: pywr_v1_schema::model::Metadata) -> Result<Self, Self::Error> {
        Ok(Self {
            title: v1
                .title
                .unwrap_or("Model converted from Pywr v1.x with no title.".to_string()),
            description: v1.description,
            minimum_version: v1.minimum_version,
        })
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema)]
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

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, JsonSchema)]
#[serde(untagged)]
pub enum DateType {
    Date(NaiveDate),
    DateTime(NaiveDateTime),
}

impl From<pywr_v1_schema::model::DateType> for DateType {
    fn from(v1: pywr_v1_schema::model::DateType) -> Self {
        match v1 {
            pywr_v1_schema::model::DateType::Date(date) => Self::Date(date),
            pywr_v1_schema::model::DateType::DateTime(date_time) => Self::DateTime(date_time),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema)]
pub struct Timestepper {
    pub start: DateType,
    pub end: DateType,
    pub timestep: Timestep,
}

impl Default for Timestepper {
    fn default() -> Self {
        Self {
            start: DateType::Date(NaiveDate::from_ymd_opt(2000, 1, 1).expect("Invalid date")),
            end: DateType::Date(NaiveDate::from_ymd_opt(2000, 12, 31).expect("Invalid date")),
            timestep: Timestep::Days(1),
        }
    }
}

impl From<pywr_v1_schema::model::Timestepper> for Timestepper {
    fn from(v1: pywr_v1_schema::model::Timestepper) -> Self {
        Self {
            start: v1.start.into(),
            end: v1.end.into(),
            timestep: v1.timestep.into(),
        }
    }
}

#[cfg(feature = "core")]
impl From<Timestepper> for pywr_core::timestep::Timestepper {
    fn from(ts: Timestepper) -> Self {
        let timestep = match ts.timestep {
            Timestep::Days(d) => TimestepDuration::Days(d),
            Timestep::Frequency(f) => TimestepDuration::Frequency(f),
        };

        let start = match ts.start {
            DateType::Date(date) => NaiveDateTime::new(date, NaiveTime::default()),
            DateType::DateTime(date_time) => date_time,
        };

        let end = match ts.end {
            DateType::Date(date) => NaiveDateTime::new(date, NaiveTime::default()),
            DateType::DateTime(date_time) => date_time,
        };

        Self::new(start, end, timestep)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
pub struct Scenario {
    pub name: String,
    pub size: usize,
    pub ensemble_names: Option<Vec<String>>,
}

#[cfg(feature = "core")]
#[derive(Clone)]
pub struct LoadArgs<'a> {
    pub schema: &'a PywrNetwork,
    pub domain: &'a ModelDomain,
    pub tables: &'a LoadedTableCollection,
    pub timeseries: &'a LoadedTimeseriesCollection,
    pub data_path: Option<&'a Path>,
    pub inter_network_transfers: &'a [PywrMultiNetworkTransfer],
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Default, JsonSchema)]
pub struct PywrNetwork {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub parameters: Option<Vec<Parameter>>,
    pub tables: Option<Vec<DataTable>>,
    pub timeseries: Option<Vec<Timeseries>>,
    pub metric_sets: Option<Vec<MetricSet>>,
    pub outputs: Option<Vec<Output>>,
}

impl FromStr for PywrNetwork {
    type Err = SchemaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(serde_json::from_str(s)?)
    }
}

impl VisitPaths for PywrNetwork {
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

impl VisitMetrics for PywrNetwork {
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

impl PywrNetwork {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, SchemaError> {
        let data = std::fs::read_to_string(path).map_err(|e| SchemaError::IO(e.to_string()))?;
        Ok(serde_json::from_str(data.as_str())?)
    }

    /// Convert a v1 network to a v2 network.
    ///
    /// This function is used to convert a v1 model to a v2 model. The conversion is not always
    /// possible and may result in errors. The errors are returned as a vector of [`ConversionError`]s.
    /// alongside the (partially) converted model. This may result in a model that will not
    /// function as expected. The user should check the errors and the converted model to ensure
    /// that the conversion has been successful.
    pub fn from_v1(v1: pywr_v1_schema::PywrNetwork) -> (Self, Vec<ConversionError>) {
        let mut errors = Vec::new();

        // Extract nodes and any timeseries data from the v1 nodes
        let nodes_and_ts: Vec<NodeAndTimeseries> = match v1.nodes {
            Some(nodes) => nodes
                .into_iter()
                .filter_map(|n| match n.try_into() {
                    Ok(n) => Some(n),
                    Err(e) => {
                        errors.push(e);
                        None
                    }
                })
                .collect::<Vec<_>>(),
            None => Vec::new(),
        };

        let mut ts_data = nodes_and_ts
            .iter()
            .filter_map(|n| n.timeseries.clone())
            .flatten()
            .collect::<Vec<_>>();

        let mut nodes = nodes_and_ts.into_iter().map(|n| n.node).collect::<Vec<_>>();

        let edges = match v1.edges {
            Some(edges) => edges.into_iter().map(|e| e.into()).collect(),
            None => Vec::new(),
        };

        let mut parameters = if let Some(v1_parameters) = v1.parameters {
            let mut unnamed_count: usize = 0;
            let (parameters, param_ts_data) =
                convert_parameter_v1_to_v2(v1_parameters, &mut unnamed_count, &mut errors);
            ts_data.extend(param_ts_data);
            Some(parameters)
        } else {
            None
        };

        // closure to update a parameter ref with a timeseries ref when names match.
        let update_to_ts_ref = &mut |m: &mut Metric| {
            if let Metric::Parameter(p) = m {
                let ts_ref = ts_data.iter().find(|ts| ts.name == Some(p.name.clone()));
                if let Some(ts_ref) = ts_ref {
                    // The timeseries requires a name to be used as a reference
                    let name = match &ts_ref.name {
                        Some(n) => n.clone(),
                        None => return,
                    };

                    let cols = match (&ts_ref.column, &ts_ref.scenario) {
                        (Some(col), None) => Some(TimeseriesColumns::Column(col.clone())),
                        (None, Some(scenario)) => Some(TimeseriesColumns::Scenario(scenario.clone())),
                        (Some(_), Some(_)) => return,
                        (None, None) => None,
                    };

                    *m = Metric::Timeseries(TimeseriesReference::new(name, cols));
                }
            }
        };

        nodes.visit_metrics_mut(update_to_ts_ref);
        if let Some(p) = parameters.as_mut() {
            p.visit_metrics_mut(update_to_ts_ref)
        }

        let timeseries = if !ts_data.is_empty() {
            let ts = convert_from_v1_data(ts_data, &v1.tables, &mut errors);
            Some(ts)
        } else {
            None
        };

        // TODO convert v1 tables!
        let tables = None;
        let outputs = None;
        let metric_sets = None;

        (
            Self {
                nodes,
                edges,
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

    pub fn get_parameter_by_name(&self, name: &str) -> Option<&Parameter> {
        match &self.parameters {
            Some(parameters) => parameters.iter().find(|p| p.name() == name),
            None => None,
        }
    }

    #[cfg(feature = "core")]
    pub fn load_tables(&self, data_path: Option<&Path>) -> Result<LoadedTableCollection, SchemaError> {
        Ok(LoadedTableCollection::from_schema(self.tables.as_deref(), data_path)?)
    }

    #[cfg(feature = "core")]
    pub fn load_timeseries(
        &self,
        domain: &ModelDomain,
        data_path: Option<&Path>,
    ) -> Result<LoadedTimeseriesCollection, SchemaError> {
        Ok(LoadedTimeseriesCollection::from_schema(
            self.timeseries.as_deref(),
            domain,
            data_path,
        )?)
    }

    #[cfg(feature = "core")]
    pub fn build_network(
        &self,
        domain: &ModelDomain,
        data_path: Option<&Path>,
        output_path: Option<&Path>,
        tables: &LoadedTableCollection,
        timeseries: &LoadedTimeseriesCollection,
        inter_network_transfers: &[PywrMultiNetworkTransfer],
    ) -> Result<pywr_core::network::Network, SchemaError> {
        let mut network = pywr_core::network::Network::default();

        let args = LoadArgs {
            schema: self,
            domain,
            tables,
            timeseries,
            data_path,
            inter_network_transfers,
        };

        // Create all the nodes
        let mut remaining_nodes = self.nodes.clone();

        while !remaining_nodes.is_empty() {
            let mut failed_nodes: Vec<Node> = Vec::new();
            let n = remaining_nodes.len();
            for node in remaining_nodes.into_iter() {
                if let Err(e) = node.add_to_model(&mut network, &args) {
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
                        network.get_node_index_by_name(from_connector.0, from_connector.1.as_deref())?;
                    let to_node_index = network.get_node_index_by_name(to_connector.0, to_connector.1.as_deref())?;
                    network.connect_nodes(from_node_index, to_node_index)?;
                }
            }
        }

        // Create all the parameters
        if let Some(mut remaining_parameters) = self.parameters.clone() {
            while !remaining_parameters.is_empty() {
                let mut failed_parameters: Vec<Parameter> = Vec::new();
                let n = remaining_parameters.len();
                for parameter in remaining_parameters.into_iter() {
                    if let Err(e) = parameter.add_to_model(&mut network, &args) {
                        // Adding the parameter failed!
                        match e {
                            SchemaError::PywrCore(core_err) => match core_err {
                                // And it failed because another parameter was not found.
                                // Let's try to load more parameters and see if this one can tried
                                // again later
                                PywrError::ParameterNotFound(_) => failed_parameters.push(parameter),
                                _ => return Err(SchemaError::PywrCore(core_err)),
                            },
                            SchemaError::ParameterNotFound(_) => failed_parameters.push(parameter),
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
            node.set_constraints(&mut network, &args)?;
        }

        // Create all of the metric sets
        if let Some(metric_sets) = &self.metric_sets {
            for metric_set in metric_sets {
                metric_set.add_to_model(&mut network, &args)?;
            }
        }

        // Create all of the outputs
        if let Some(outputs) = &self.outputs {
            for output in outputs {
                output.add_to_model(&mut network, output_path)?;
            }
        }

        Ok(network)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
#[serde(untagged)]
pub enum PywrNetworkRef {
    Path(PathBuf),
    Inline(PywrNetwork),
}

/// The top-level schema for a Pywr model.
///
/// A Pywr model is defined by this top-level schema which is mostly conveniently loaded from a
/// JSON file. The schema is used to "build" a [`pywr_core::models::Model`] which can then be
/// "run" to produce results. The purpose of the schema is to provide a higher level and more
/// user friendly interface to model definition than the core model itself. This allows
/// abstractions, such as [`crate::nodes::WaterTreatmentWorks`], to be created and used in the
/// schema without the user needing to know the details of how this is implemented in the core
/// model.
///
///
/// # Example
///
/// The simplest model is given in the example below:
///
/// ```json
#[doc = include_str!("test_models/simple1.json")]
/// ```
///
///
///
#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
pub struct PywrModel {
    pub metadata: Metadata,
    pub timestepper: Timestepper,
    pub scenarios: Option<Vec<Scenario>>,
    pub network: PywrNetwork,
}

impl FromStr for PywrModel {
    type Err = SchemaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(serde_json::from_str(s)?)
    }
}

impl VisitPaths for PywrModel {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        self.network.visit_paths(visitor);
    }
    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        self.network.visit_paths_mut(visitor)
    }
}

impl VisitMetrics for PywrModel {
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        self.network.visit_metrics(visitor);
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        self.network.visit_metrics_mut(visitor);
    }
}

impl PywrModel {
    pub fn new(title: &str, start: &DateType, end: &DateType) -> Self {
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
            network: PywrNetwork::default(),
        }
    }

    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, SchemaError> {
        let data = std::fs::read_to_string(path).map_err(|e| SchemaError::IO(e.to_string()))?;
        Ok(serde_json::from_str(data.as_str())?)
    }

    #[cfg(feature = "core")]
    pub fn build_model(
        &self,
        data_path: Option<&Path>,
        output_path: Option<&Path>,
    ) -> Result<pywr_core::models::Model, SchemaError> {
        let timestepper = self.timestepper.clone().into();

        let mut scenario_collection = pywr_core::scenario::ScenarioGroupCollection::default();

        if let Some(scenarios) = &self.scenarios {
            for scenario in scenarios {
                scenario_collection.add_group(&scenario.name, scenario.size);
            }
        }

        let domain = ModelDomain::from(timestepper, scenario_collection)?;

        let tables = self.network.load_tables(data_path)?;
        let timeseries = self.network.load_timeseries(&domain, data_path)?;

        let network = self
            .network
            .build_network(&domain, data_path, output_path, &tables, &timeseries, &[])?;

        let model = pywr_core::models::Model::new(domain, network);

        Ok(model)
    }

    /// Convert a v1 model to a v2 model.
    ///
    /// This function is used to convert a v1 model to a v2 model. The conversion is not always
    /// possible and may result in errors. The errors are returned as a vector of [`ConversionError`]s.
    /// alongside the (partially) converted model. This may result in a model that will not
    /// function as expected. The user should check the errors and the converted model to ensure
    /// that the conversion has been successful.
    pub fn from_v1(v1: pywr_v1_schema::PywrModel) -> (Self, Vec<ConversionError>) {
        let mut errors = Vec::new();

        let metadata = v1.metadata.try_into().unwrap_or_else(|e| {
            errors.push(e);
            Metadata::default()
        });

        let timestepper = v1.timestepper.into();

        let (network, network_errors) = PywrNetwork::from_v1(v1.network);
        errors.extend(network_errors);

        (
            Self {
                metadata,
                timestepper,
                scenarios: None,
                network,
            },
            errors,
        )
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct PywrMultiNetworkTransfer {
    pub from_network: String,
    pub metric: Metric,
    pub name: String,
    pub initial_value: Option<f64>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct PywrMultiNetworkEntry {
    pub name: String,
    pub network: PywrNetworkRef,
    pub transfers: Vec<PywrMultiNetworkTransfer>,
}

/// A Pywr model containing multiple link networks.
///
/// This schema is used to define a model containing multiple linked networks. Each network
/// is self-contained and solved as like a single a model. However, the networks can be linked
/// together using [`PywrMultiNetworkTransfer`]s. These transfers allow the value of a metric
/// in one network to be used as the value of a parameter in another network. This allows complex
/// inter-model relationships to be defined.
///
/// The model is solved by iterating over the networks within each time-step. Inter-network
/// transfers are updated between each network solve. The networks are solved in the order
/// that they are defined. This means that the order of the networks is important. For example,
/// the 1st network will only be able to use the previous time-step's state from other networks.
/// Whereas the 2nd network can use metrics calculated in the current time-step of the 1st model.
///
/// The overall algorithm produces an single model run with interleaved solving of each network.
/// The pseudo-code for the algorithm is:
///
/// ```text
/// for time_step in time_steps {
///     for network in networks {
///         // Get the latest values from the other networks
///         network.update_inter_network_transfers();
///         // Solve this network's allocation routine / linear program
///         network.solve();
///     }
/// }
/// ```
///
/// # When to use
///
/// A [`PywrMultiNetworkModel`] should be used in cases where there is a strong separation between
/// the networks being simulated. The allocation routine (linear program) of each network is solved
/// independently each time-step. This means that the only way in which the networks can share
/// information and data is between the linear program solves via the user defined transfers.
///
/// Configuring a model like this maybe be beneficial in the following cases:
///   1. Represent separate systems with limited and/or prescribed connectivity. For example,
///     linking networks from two suppliers connected by a strategic transfer.
///   2. Have important validated behaviour of the allocation that should be retained. If the
///     networks (linear programs) were combined into a single model, the allocation routine could
///     produce different results (i.e. penalty costs from one model influencing another).
///   2. Are very large and/or complex to control model run times. The run time of a
///     [`PywrMultiNetworkModel`] is roughly the sum of the individual networks. Whereas the time
///     solve a large linear program combining all the networks could be significantly longer.
///
/// # Example
///
/// The following example shows a model with networks with the inflow to "supply2" in the second
/// network defined as the flow to "demand1" in the first network.
///
/// ```json5
/// // model.json
#[doc = include_str!("test_models/multi1/model.json")]
/// // network1.json
#[doc = include_str!("test_models/multi1/network1.json")]
/// // network2.json
#[doc = include_str!("test_models/multi1/network2.json")]
/// ```
///
///
///
#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct PywrMultiNetworkModel {
    pub metadata: Metadata,
    pub timestepper: Timestepper,
    pub scenarios: Option<Vec<Scenario>>,
    pub networks: Vec<PywrMultiNetworkEntry>,
}

impl FromStr for PywrMultiNetworkModel {
    type Err = SchemaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(serde_json::from_str(s)?)
    }
}

impl PywrMultiNetworkModel {
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, SchemaError> {
        let data = std::fs::read_to_string(path).map_err(|e| SchemaError::IO(e.to_string()))?;
        Ok(serde_json::from_str(data.as_str())?)
    }

    #[cfg(feature = "core")]
    pub fn build_model(
        &self,
        data_path: Option<&Path>,
        output_path: Option<&Path>,
    ) -> Result<pywr_core::models::MultiNetworkModel, SchemaError> {
        let timestepper = self.timestepper.clone().into();

        let mut scenario_collection = pywr_core::scenario::ScenarioGroupCollection::default();

        if let Some(scenarios) = &self.scenarios {
            for scenario in scenarios {
                scenario_collection.add_group(&scenario.name, scenario.size);
            }
        }

        let domain = ModelDomain::from(timestepper, scenario_collection)?;
        let mut networks = Vec::with_capacity(self.networks.len());
        let mut inter_network_transfers = Vec::new();
        let mut schemas: Vec<(PywrNetwork, LoadedTableCollection, LoadedTimeseriesCollection)> =
            Vec::with_capacity(self.networks.len());

        // First load all the networks
        // These will contain any parameters that are referenced by the inter-model transfers
        // Because of potential circular references, we need to load all the networks first.
        for network_entry in &self.networks {
            // Load the network itself
            let (network, schema, tables, timeseries) = match &network_entry.network {
                PywrNetworkRef::Path(path) => {
                    let pth = if let Some(dp) = data_path {
                        if path.is_relative() {
                            dp.join(path)
                        } else {
                            path.clone()
                        }
                    } else {
                        path.clone()
                    };

                    let network_schema = PywrNetwork::from_path(pth)?;
                    let tables = network_schema.load_tables(data_path)?;
                    let timeseries = network_schema.load_timeseries(&domain, data_path)?;
                    let net = network_schema.build_network(
                        &domain,
                        data_path,
                        output_path,
                        &tables,
                        &timeseries,
                        &network_entry.transfers,
                    )?;

                    (net, network_schema, tables, timeseries)
                }
                PywrNetworkRef::Inline(network_schema) => {
                    let tables = network_schema.load_tables(data_path)?;
                    let timeseries = network_schema.load_timeseries(&domain, data_path)?;
                    let net = network_schema.build_network(
                        &domain,
                        data_path,
                        output_path,
                        &tables,
                        &timeseries,
                        &network_entry.transfers,
                    )?;

                    (net, network_schema.clone(), tables, timeseries)
                }
            };

            schemas.push((schema, tables, timeseries));
            networks.push((network_entry.name.clone(), network));
        }

        // Now load the inter-model transfers
        for (to_network_idx, network_entry) in self.networks.iter().enumerate() {
            for transfer in &network_entry.transfers {
                // Load the metric from the "from" network

                let (from_network_idx, from_network) = networks
                    .iter_mut()
                    .enumerate()
                    .find_map(|(idx, (name, net))| {
                        if name.as_str() == transfer.from_network.as_str() {
                            Some((idx, net))
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| SchemaError::NetworkNotFound(transfer.from_network.clone()))?;

                // The transfer metric will fail to load if it is defined as an inter-model transfer itself.
                let (from_schema, from_tables, from_timeseries) = &schemas[from_network_idx];

                let args = LoadArgs {
                    schema: from_schema,
                    domain: &domain,
                    tables: from_tables,
                    timeseries: from_timeseries,
                    data_path,
                    inter_network_transfers: &[],
                };

                let from_metric = transfer.metric.load(from_network, &args)?;

                inter_network_transfers.push((from_network_idx, from_metric, to_network_idx, transfer.initial_value));
            }
        }

        // Now construct the model from the loaded components
        let mut model = pywr_core::models::MultiNetworkModel::new(domain);

        for (name, network) in networks {
            model.add_network(&name, network)?;
        }

        for (from_network_idx, from_metric, to_network_idx, initial_value) in inter_network_transfers {
            model.add_inter_network_transfer(from_network_idx, from_metric, to_network_idx, initial_value);
        }

        Ok(model)
    }
}

#[cfg(test)]
mod tests {
    use super::PywrModel;
    use crate::model::Timestepper;
    use crate::visit::VisitPaths;
    use std::path::PathBuf;

    fn model_str() -> &'static str {
        include_str!("./test_models/simple1.json")
    }

    #[test]
    fn test_simple1_schema() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();

        assert_eq!(schema.network.nodes.len(), 3);
        assert_eq!(schema.network.edges.len(), 2);
    }

    #[test]
    fn test_date() {
        let timestepper_str = r#"
        {
            "start": "2015-01-01",
            "end": "2015-12-31",
            "timestep": 1
        }
        "#;

        let timestep: Timestepper = serde_json::from_str(timestepper_str).unwrap();

        match timestep.start {
            super::DateType::Date(date) => {
                assert_eq!(date, chrono::NaiveDate::from_ymd_opt(2015, 1, 1).unwrap());
            }
            _ => panic!("Expected a date"),
        }

        match timestep.end {
            super::DateType::Date(date) => {
                assert_eq!(date, chrono::NaiveDate::from_ymd_opt(2015, 12, 31).unwrap());
            }
            _ => panic!("Expected a date"),
        }
    }

    #[test]
    fn test_datetime() {
        let timestepper_str = r#"
        {
            "start": "2015-01-01T12:30:00",
            "end": "2015-01-01T14:30:00",
            "timestep": 1
        }
        "#;

        let timestep: Timestepper = serde_json::from_str(timestepper_str).unwrap();

        match timestep.start {
            super::DateType::DateTime(date_time) => {
                assert_eq!(
                    date_time,
                    chrono::NaiveDate::from_ymd_opt(2015, 1, 1)
                        .unwrap()
                        .and_hms_opt(12, 30, 0)
                        .unwrap()
                );
            }
            _ => panic!("Expected a date"),
        }

        match timestep.end {
            super::DateType::DateTime(date_time) => {
                assert_eq!(
                    date_time,
                    chrono::NaiveDate::from_ymd_opt(2015, 1, 1)
                        .unwrap()
                        .and_hms_opt(14, 30, 0)
                        .unwrap()
                );
            }
            _ => panic!("Expected a date"),
        }
    }

    /// Test that the visit_paths functions works as expected.
    #[test]
    fn test_visit_paths() {
        let mut model_fn = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        model_fn.push("src/test_models/timeseries.json");

        let mut schema = PywrModel::from_path(model_fn.as_path()).unwrap();

        let expected_paths = vec![PathBuf::from("inflow.csv")];

        let mut paths: Vec<PathBuf> = Vec::new();

        schema.visit_paths(&mut |p| {
            paths.push(p.to_path_buf());
        });

        assert_eq!(&paths, &expected_paths);

        schema.visit_paths_mut(&mut |p: &mut PathBuf| {
            *p = PathBuf::from("this-file-does-not-exist.csv");
        });

        // Expect this to file as the path has been updated to a missing file.
        #[cfg(feature = "core")]
        if schema.build_model(model_fn.parent(), None).is_ok() {
            let str = serde_json::to_string_pretty(&schema).unwrap();
            panic!("Expected an error due to missing file: {str}");
        }
    }

    #[test]
    fn test_v1_conversion() {
        let v1_str = include_str!("./test_models/v1/timeseries.json");
        let v1: pywr_v1_schema::PywrModel = serde_json::from_str(v1_str).unwrap();

        let (v2, errors) = PywrModel::from_v1(v1);

        assert_eq!(errors.len(), 0);

        std::fs::write("tmp.json", serde_json::to_string_pretty(&v2).unwrap()).unwrap();

        let v2_converted: serde_json::Value =
            serde_json::from_str(&serde_json::to_string_pretty(&v2).unwrap()).unwrap();

        let v2_expected: serde_json::Value =
            serde_json::from_str(include_str!("./test_models/v1/timeseries-converted.json")).unwrap();

        assert_eq!(v2_converted, v2_expected);
    }
}

#[cfg(test)]
#[cfg(feature = "core")]
mod core_tests {
    use super::{PywrModel, PywrMultiNetworkModel};
    use crate::metric::{Metric, ParameterReference};
    use crate::parameters::{AggFunc, AggregatedParameter, ConstantParameter, ConstantValue, Parameter, ParameterMeta};
    use ndarray::{Array1, Array2, Axis};
    use pywr_core::{metric::MetricF64, recorders::AssertionRecorder, solvers::ClpSolver, test_utils::run_all_solvers};
    use std::path::PathBuf;

    fn model_str() -> &'static str {
        include_str!("./test_models/simple1.json")
    }

    #[test]
    fn test_simple1_run() {
        let data = model_str();
        let schema: PywrModel = serde_json::from_str(data).unwrap();
        let mut model = schema.build_model(None, None).unwrap();

        let network = model.network_mut();
        assert_eq!(network.nodes().len(), 3);
        assert_eq!(network.edges().len(), 2);

        let demand1_idx = network.get_node_index_by_name("demand1", None).unwrap();

        let expected_values: Array1<f64> = [10.0; 365].to_vec().into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        let rec = AssertionRecorder::new(
            "assert-demand1",
            MetricF64::NodeInFlow(demand1_idx),
            expected_values,
            None,
            None,
        );
        network.add_recorder(Box::new(rec)).unwrap();

        // Test all solvers
        run_all_solvers(&model, &[], &[]);
    }

    /// Test that a cycle in parameter dependencies does not load.
    #[test]
    fn test_cycle_error() {
        let data = model_str();
        let mut schema: PywrModel = serde_json::from_str(data).unwrap();

        // Add additional parameters for the test
        if let Some(parameters) = &mut schema.network.parameters {
            parameters.extend(vec![
                Parameter::Aggregated(AggregatedParameter {
                    meta: ParameterMeta {
                        name: "agg1".to_string(),
                        comment: None,
                    },
                    agg_func: AggFunc::Sum,
                    metrics: vec![
                        Metric::Parameter(ParameterReference {
                            name: "p1".to_string(),
                            key: None,
                        }),
                        Metric::Parameter(ParameterReference {
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
                        Metric::Parameter(ParameterReference {
                            name: "p1".to_string(),
                            key: None,
                        }),
                        Metric::Parameter(ParameterReference {
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

        if let Some(parameters) = &mut schema.network.parameters {
            parameters.extend(vec![
                Parameter::Aggregated(AggregatedParameter {
                    meta: ParameterMeta {
                        name: "agg1".to_string(),
                        comment: None,
                    },
                    agg_func: AggFunc::Sum,
                    metrics: vec![
                        Metric::Parameter(ParameterReference {
                            name: "p1".to_string(),
                            key: None,
                        }),
                        Metric::Parameter(ParameterReference {
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
        let _ = schema.build_model(None, None).unwrap();
    }

    /// Test the multi1 model
    #[test]
    fn test_multi1_model() {
        let mut model_fn = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        model_fn.push("src/test_models/multi1/model.json");

        let schema = PywrMultiNetworkModel::from_path(model_fn.as_path()).unwrap();
        let mut model = schema.build_model(model_fn.parent(), None).unwrap();

        // Add some recorders for the expected outputs
        let network_1_idx = model
            .get_network_index_by_name("network1")
            .expect("network 1 not found");
        let network_1 = model.network_mut(network_1_idx).expect("network 1 not found");
        let demand1_idx = network_1.get_node_index_by_name("demand1", None).unwrap();

        let expected_values: Array1<f64> = [10.0; 365].to_vec().into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        let rec = AssertionRecorder::new(
            "assert-demand1",
            MetricF64::NodeInFlow(demand1_idx),
            expected_values,
            None,
            None,
        );
        network_1.add_recorder(Box::new(rec)).unwrap();

        // Inflow to demand2 should be 10.0 via the transfer from network1 (demand1)
        let network_2_idx = model
            .get_network_index_by_name("network2")
            .expect("network 1 not found");
        let network_2 = model.network_mut(network_2_idx).expect("network 2 not found");
        let demand1_idx = network_2.get_node_index_by_name("demand2", None).unwrap();

        let expected_values: Array1<f64> = [10.0; 365].to_vec().into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        let rec = AssertionRecorder::new(
            "assert-demand2",
            MetricF64::NodeInFlow(demand1_idx),
            expected_values,
            None,
            None,
        );
        network_2.add_recorder(Box::new(rec)).unwrap();

        model.run::<ClpSolver>(&Default::default()).unwrap();
    }

    /// Test the multi2 model
    #[test]
    fn test_multi2_model() {
        let mut model_fn = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        model_fn.push("src/test_models/multi2/model.json");

        let schema = PywrMultiNetworkModel::from_path(model_fn.as_path()).unwrap();
        let mut model = schema.build_model(model_fn.parent(), None).unwrap();

        // Add some recorders for the expected outputs
        // inflow1 should be set to a max of 20.0 from the "demand" parameter in network2
        let network_1_idx = model
            .get_network_index_by_name("network1")
            .expect("network 1 not found");
        let network_1 = model.network_mut(network_1_idx).expect("network 1 not found");
        let demand1_idx = network_1.get_node_index_by_name("demand1", None).unwrap();

        let expected_values: Array1<f64> = [10.0; 365].to_vec().into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        let rec = AssertionRecorder::new(
            "assert-demand1",
            MetricF64::NodeInFlow(demand1_idx),
            expected_values,
            None,
            None,
        );
        network_1.add_recorder(Box::new(rec)).unwrap();

        // Inflow to demand2 should be 10.0 via the transfer from network1 (demand1)
        let network_2_idx = model
            .get_network_index_by_name("network2")
            .expect("network 1 not found");
        let network_2 = model.network_mut(network_2_idx).expect("network 2 not found");
        let demand1_idx = network_2.get_node_index_by_name("demand2", None).unwrap();

        let expected_values: Array1<f64> = [10.0; 365].to_vec().into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        let rec = AssertionRecorder::new(
            "assert-demand2",
            MetricF64::NodeInFlow(demand1_idx),
            expected_values,
            None,
            None,
        );
        network_2.add_recorder(Box::new(rec)).unwrap();

        model.run::<ClpSolver>(&Default::default()).unwrap();
    }
}
