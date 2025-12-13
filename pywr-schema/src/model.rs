#[cfg(feature = "core")]
use crate::data_tables::LoadedTableCollection;
use crate::error::ComponentConversionError;
#[cfg(feature = "core")]
use crate::error::SchemaError;
use crate::metric::Metric;
#[cfg(feature = "core")]
use crate::network::{LoadArgs, NetworkSchemaBuildError, NetworkSchemaReadError};
#[cfg(feature = "core")]
use crate::timeseries::LoadedTimeseriesCollection;
use crate::visit::{VisitMetrics, VisitPaths};
use crate::{ConversionError, NetworkSchema, NetworkSchemaRef};
#[cfg(feature = "core")]
use chrono::NaiveTime;
use chrono::{NaiveDate, NaiveDateTime};
#[cfg(all(feature = "core", feature = "pyo3"))]
use pyo3::Python;
#[cfg(feature = "pyo3")]
use pyo3::{Bound, PyErr, PyResult, exceptions::PyRuntimeError, pyclass, pymethods, types::PyType};
#[cfg(feature = "core")]
use pywr_core::{
    models::{Model, ModelDomain, MultiNetworkModel, MultiNetworkModelError},
    timestep::TimestepDuration,
};
use pywr_schema_macros::skip_serializing_none;
use schemars::JsonSchema;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
use thiserror::Error;

#[skip_serializing_none]
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

impl From<pywr_v1_schema::model::Metadata> for Metadata {
    fn from(v1: pywr_v1_schema::model::Metadata) -> Self {
        Self {
            title: v1
                .title
                .unwrap_or("Model converted from Pywr v1.x with no title.".to_string()),
            description: v1.description,
            minimum_version: v1.minimum_version,
        }
    }
}

/// A timestep defines the time interval between each step in the model.
///
/// The timestep can be defined in three ways:
/// - A fixed number of non-zero hours.
/// - A fixed number of non-zero days.
/// - A frequency string that can be parsed by polars (e.g. '7d').
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(TimestepType))]
pub enum Timestep {
    /// A fixed number of hours.
    Hours { hours: NonZeroU64 },
    /// A fixed number of days.
    Days { days: NonZeroU64 },
    /// A frequency string that can be parsed by polars.
    Frequency { freq: String },
}

impl From<pywr_v1_schema::model::Timestep> for Timestep {
    fn from(v1: pywr_v1_schema::model::Timestep) -> Self {
        match v1 {
            pywr_v1_schema::model::Timestep::Days(d) => Self::Days {
                days: NonZeroU64::new(d).expect("days must be non-zero"),
            },
            pywr_v1_schema::model::Timestep::Frequency(freq) => Self::Frequency { freq },
        }
    }
}

impl Default for Timestep {
    fn default() -> Self {
        Self::Days {
            days: NonZeroU64::new(1).expect("1 is non-zero"),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Copy, Debug, JsonSchema, Display, EnumDiscriminants)]
#[serde(untagged)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(DateType))]
pub enum Date {
    Date(NaiveDate),
    DateTime(NaiveDateTime),
}

impl From<pywr_v1_schema::model::DateType> for Date {
    fn from(v1: pywr_v1_schema::model::DateType) -> Self {
        match v1 {
            pywr_v1_schema::model::DateType::Date(date) => Self::Date(date),
            pywr_v1_schema::model::DateType::DateTime(date_time) => Self::DateTime(date_time),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug, JsonSchema)]
pub struct Timestepper {
    pub start: Date,
    pub end: Date,
    pub timestep: Timestep,
}

impl Default for Timestepper {
    fn default() -> Self {
        Self {
            start: Date::Date(NaiveDate::from_ymd_opt(2000, 1, 1).expect("Invalid date")),
            end: Date::Date(NaiveDate::from_ymd_opt(2000, 12, 31).expect("Invalid date")),
            timestep: Timestep::default(),
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
            Timestep::Hours { hours } => TimestepDuration::Hours(hours),
            Timestep::Days { days } => TimestepDuration::Days(days),
            Timestep::Frequency { freq } => TimestepDuration::Frequency(freq),
        };

        let start = match ts.start {
            Date::Date(date) => NaiveDateTime::new(date, NaiveTime::default()),
            Date::DateTime(date_time) => date_time,
        };

        let end = match ts.end {
            Date::Date(date) => NaiveDateTime::new(date, NaiveTime::default()),
            Date::DateTime(date_time) => date_time,
        };

        Self::new(start, end, timestep)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioGroupSlice {
    pub start: usize,
    pub end: usize,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioGroupIndices {
    pub indices: Vec<usize>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioGroupLabels {
    pub labels: Vec<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(ScenarioGroupSubsetType))]
pub enum ScenarioGroupSubset {
    Slice(ScenarioGroupSlice),
    Indices(ScenarioGroupIndices),
    Labels(ScenarioGroupLabels),
}

/// A scenario group defines a set of scenarios that can be run in a model.
///
/// A scenario group is defined by a name and a size. The size is the number of scenarios in the group.
/// Optional labels can be defined for the group. These labels are used in output data
/// to identify the scenario group. A subset can be defined to simulate only part of the group.
///
/// See also the examples in the [`ScenarioDomain`] documentation.
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
pub struct ScenarioGroup {
    pub name: String,
    pub size: usize,
    pub labels: Option<Vec<String>>,
    pub subset: Option<ScenarioGroupSubset>,
}

#[cfg(feature = "core")]
impl TryInto<pywr_core::scenario::ScenarioGroup> for ScenarioGroup {
    type Error = pywr_core::scenario::ScenarioError;

    fn try_into(self) -> Result<pywr_core::scenario::ScenarioGroup, Self::Error> {
        let mut builder = pywr_core::scenario::ScenarioGroupBuilder::new(&self.name, self.size);

        if let Some(labels) = self.labels {
            builder = builder.with_labels(&labels);
        }

        if let Some(subset) = self.subset {
            match subset {
                ScenarioGroupSubset::Slice(slice) => {
                    builder = builder.with_subset_slice(slice.start, slice.end);
                }
                ScenarioGroupSubset::Indices(indices) => {
                    builder = builder.with_subset_indices(indices.indices);
                }
                ScenarioGroupSubset::Labels(labels) => {
                    builder = builder.with_subset_labels(&labels.labels);
                }
            }
        }

        builder.build()
    }
}

impl TryFrom<pywr_v1_schema::model::Scenario> for ScenarioGroup {
    type Error = ConversionError;

    fn try_from(v1: pywr_v1_schema::model::Scenario) -> Result<Self, Self::Error> {
        let subset = v1
            .slice
            .map(|s| match s.len() {
                1 => {
                    let start = 0;
                    let end = v1.size;
                    Ok(ScenarioGroupSubset::Slice(ScenarioGroupSlice { start, end }))
                }
                2 => {
                    let start = s[0].unwrap_or_default();
                    let end = match s[1] {
                        Some(v) => v,
                        None => v1.size,
                    };
                    Ok(ScenarioGroupSubset::Slice(ScenarioGroupSlice { start, end }))
                }
                3 => {
                    let start = s[0].unwrap_or_default();
                    let end = match s[1] {
                        Some(v) => v,
                        None => v1.size,
                    };
                    match s[2] {
                        Some(step) => {
                            let indices = (start..end).step_by(step).collect();
                            Ok(ScenarioGroupSubset::Indices(ScenarioGroupIndices { indices }))
                        }
                        None => Ok(ScenarioGroupSubset::Slice(ScenarioGroupSlice { start, end })),
                    }
                }
                _ => Err(ConversionError::InvalidScenarioSlice { length: s.len() }),
            })
            .transpose()?;

        Ok(Self {
            name: v1.name,
            size: v1.size,
            labels: v1.ensemble_names,
            subset,
        })
    }
}

#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema, Display, EnumDiscriminants)]
#[serde(untagged)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(ScenarioLabelOrIndexType))]
pub enum ScenarioLabelOrIndex {
    Label(String),
    Index(usize),
}

#[cfg(feature = "core")]
impl From<ScenarioLabelOrIndex> for pywr_core::scenario::ScenarioLabelOrIndex {
    fn from(val: ScenarioLabelOrIndex) -> pywr_core::scenario::ScenarioLabelOrIndex {
        match val {
            ScenarioLabelOrIndex::Label(label) => pywr_core::scenario::ScenarioLabelOrIndex::Label(label),
            ScenarioLabelOrIndex::Index(index) => pywr_core::scenario::ScenarioLabelOrIndex::Index(index),
        }
    }
}

/// A scenario domain is a collection of scenario groups that define the possible scenarios that
/// can be run in a model.
///
/// Each scenario group has a name and size. The full space of the domain is defined as the
/// cartesian product of the sizes of each group. For simulation purposes, the domain can be
/// constrained (or "subsetted") by defining a subset for each group. A subset can be defined
/// using specific labels or indices of the group, or using slice of the group. The slice is a contiguous
/// subset of the group that will be used in the simulation. The slice is defined by the `start`
/// and `end` indices of the group. The `start` index is inclusive and the `end` index is exclusive.
///
/// Alternatively, the domain can be constrained by defining a list of combinations of the groups
/// that will be used in the simulation. The combinations are defined as a list of lists of indices
/// of the groups.
///
/// It is an error if both a `slice`(s) and `combinations` are defined.
///
/// # JSON Examples
///
/// The examples below show how a scenario group can be defined in JSON.
///
/// ```json
#[doc = include_str!("doc_examples/scenario_domain1.json")]
/// ```
///
/// The example below shows how a scenario group can be defined with custom labels. In this
/// case Roman numerals are used to identify the individual scenarios.
///
/// ```json
#[doc = include_str!("doc_examples/scenario_domain2.json")]
/// ```
///
/// The example below shows how to define two scenario groups.
///
/// ```json
#[doc = include_str!("doc_examples/scenario_domain3.json")]
/// ```
#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScenarioDomain {
    /// The groups that define the scenario domain.
    pub groups: Vec<ScenarioGroup>,
    /// Optional combinations of the groups that allow simulation of specific scenarios.
    pub combinations: Option<Vec<Vec<ScenarioLabelOrIndex>>>,
}

impl TryFrom<Vec<pywr_v1_schema::model::Scenario>> for ScenarioDomain {
    type Error = ConversionError;

    fn try_from(v1: Vec<pywr_v1_schema::model::Scenario>) -> Result<Self, Self::Error> {
        let groups = v1.into_iter().map(|g| g.try_into()).collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            groups,
            combinations: None,
        })
    }
}

#[cfg(feature = "core")]
impl TryInto<pywr_core::scenario::ScenarioDomainBuilder> for ScenarioDomain {
    type Error = pywr_core::scenario::ScenarioError;

    fn try_into(self) -> Result<pywr_core::scenario::ScenarioDomainBuilder, Self::Error> {
        let mut builder = pywr_core::scenario::ScenarioDomainBuilder::default();

        for group in self.groups {
            builder = builder.with_group(group.try_into()?)?;
        }

        if let Some(combinations) = self.combinations {
            builder = builder.with_combinations(combinations.into_iter().collect());
        }

        Ok(builder)
    }
}

/// Error type for reading a [`ModelSchema`] or [`MultiNetworkModelSchema`] network from a file or string.
#[derive(Error, Debug)]
pub enum ModelSchemaReadError {
    #[error("IO error on path `{path}`: {error}")]
    IO { path: PathBuf, error: std::io::Error },
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(feature = "pyo3")]
impl From<ModelSchemaReadError> for PyErr {
    fn from(err: ModelSchemaReadError) -> PyErr {
        pyo3::exceptions::PyRuntimeError::new_err(err.to_string())
    }
}

#[derive(Error, Debug)]
#[cfg(feature = "core")]
pub enum ModelSchemaBuildError {
    #[error("Failed to construct scenario builder: {0}")]
    ScenarioBuilderError(#[from] pywr_core::scenario::ScenarioError),
    #[error("Failed to construct model domain: {0}")]
    CoreModelDomainError(#[from] pywr_core::models::ModelDomainError),
    #[error("Failed to construct the network: {source}")]
    NetworkBuildError {
        #[source]
        source: Box<NetworkSchemaBuildError>,
    },
}

#[cfg(all(feature = "core", feature = "pyo3"))]
impl From<ModelSchemaBuildError> for PyErr {
    fn from(err: ModelSchemaBuildError) -> PyErr {
        let py_err = pyo3::exceptions::PyRuntimeError::new_err(err.to_string());

        // Check if the error has a cause that can be converted to a PyErr
        let py_cause: Result<PyErr, ()> = match err {
            ModelSchemaBuildError::NetworkBuildError { source } => (*source).try_into(),
            _ => Err(()),
        };

        if let Ok(py_cause) = py_cause {
            // If the cause is a PyErr, set it as the cause of the PyErr
            return Python::attach(|py| {
                py_err.set_cause(py, Some(py_cause));
                py_err
            });
        }

        py_err
    }
}

/// The top-level schema for a Pywr model.
///
/// A Pywr model is defined by this top-level schema which is mostly conveniently loaded from a
/// JSON file. The schema is used to "build" a [`pywr_core::models::Model`] which can then be
/// "run" to produce results. The purpose of the schema is to provide a higher level and more
/// user friendly interface to model definition than the core model itself. This allows
/// abstractions, such as [`crate::nodes::WaterTreatmentWorksNode`], to be created and used in the
/// schema without the user needing to know the details of how this is implemented in the core
/// model.
///
///
/// # Example
///
/// The simplest model is given in the example below:
///
/// ```json
#[doc = include_str!("../tests/simple1.json")]
/// ```
///
///
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone, JsonSchema)]
#[cfg_attr(feature = "pyo3", pyclass)]
pub struct ModelSchema {
    pub metadata: Metadata,
    pub timestepper: Timestepper,
    pub scenarios: Option<ScenarioDomain>,
    pub network: NetworkSchema,
}

impl FromStr for ModelSchema {
    type Err = ModelSchemaReadError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(serde_json::from_str(s)?)
    }
}

impl VisitPaths for ModelSchema {
    fn visit_paths<F: FnMut(&Path)>(&self, visitor: &mut F) {
        self.network.visit_paths(visitor);
    }
    fn visit_paths_mut<F: FnMut(&mut PathBuf)>(&mut self, visitor: &mut F) {
        self.network.visit_paths_mut(visitor)
    }
}

impl VisitMetrics for ModelSchema {
    fn visit_metrics<F: FnMut(&Metric)>(&self, visitor: &mut F) {
        self.network.visit_metrics(visitor);
    }

    fn visit_metrics_mut<F: FnMut(&mut Metric)>(&mut self, visitor: &mut F) {
        self.network.visit_metrics_mut(visitor);
    }
}

impl ModelSchema {
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
                timestep: Timestep::default(),
            },
            scenarios: None,
            network: NetworkSchema::default(),
        }
    }

    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, ModelSchemaReadError> {
        let data = std::fs::read_to_string(&path).map_err(|error| ModelSchemaReadError::IO {
            path: path.as_ref().to_path_buf(),
            error,
        })?;
        Ok(serde_json::from_str(data.as_str())?)
    }

    #[cfg(feature = "core")]
    pub fn build_model(
        &self,
        data_path: Option<&Path>,
        output_path: Option<&Path>,
    ) -> Result<Model, ModelSchemaBuildError> {
        let timestepper = self.timestepper.clone().into();

        let scenario_builder = match &self.scenarios {
            Some(scenarios) => scenarios.clone().try_into()?,
            None => pywr_core::scenario::ScenarioDomainBuilder::default(),
        };

        let domain = ModelDomain::try_from(timestepper, scenario_builder)?;

        let (network, _tables, _ts) = self
            .network
            .build_network(&domain, data_path, output_path, &[])
            .map_err(|source| ModelSchemaBuildError::NetworkBuildError {
                source: Box::new(source),
            })?;

        let model = Model::new(domain, network);

        Ok(model)
    }

    /// Convert a v1 model to a v2 model.
    ///
    /// This function is used to convert a v1 model to a v2 model. The conversion is not always
    /// possible and may result in errors. The errors are returned as a vector of [`ComponentConversionError`]s.
    /// alongside the (partially) converted model. This may result in a model that will not
    /// function as expected. The user should check the errors and the converted model to ensure
    /// that the conversion has been successful.
    pub fn from_v1(v1: pywr_v1_schema::PywrModel) -> (Self, Vec<ComponentConversionError>) {
        let mut errors = Vec::new();

        let metadata = v1.metadata.into();
        let timestepper = v1.timestepper.into();
        let scenarios = match v1.scenarios.map(|s| s.try_into()) {
            Some(Ok(scenarios)) => Some(scenarios),
            Some(Err(err)) => {
                errors.push(ComponentConversionError::Scenarios { error: err });
                None
            }
            None => None,
        };

        let (network, network_errors) = NetworkSchema::from_v1(v1.network);
        errors.extend(network_errors);

        (
            Self {
                metadata,
                timestepper,
                scenarios,
                network,
            },
            errors,
        )
    }

    /// Convert a v1 JSON string to a v2 model.
    ///
    /// See [`ModelSchema::from_v1`] for more information.
    pub fn from_v1_str(v1: &str) -> Result<(Self, Vec<ComponentConversionError>), pywr_v1_schema::PywrSchemaError> {
        let v1_model: pywr_v1_schema::PywrModel = serde_json::from_str(v1)?;

        Ok(Self::from_v1(v1_model))
    }
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl ModelSchema {
    #[new]
    fn new_py(title: &str, start: NaiveDateTime, end: NaiveDateTime) -> Self {
        let start = Date::DateTime(start);
        let end = Date::DateTime(end);

        Self::new(title, &start, &end)
    }

    /// Create a new schema object from a file path.
    #[classmethod]
    #[pyo3(name = "from_path")]
    fn from_path_py(_cls: &Bound<'_, PyType>, path: PathBuf) -> PyResult<Self> {
        Ok(Self::from_path(path)?)
    }

    ///  Create a new schema object from a JSON string.
    #[classmethod]
    #[pyo3(name = "from_json_string")]
    fn from_json_string_py(_cls: &Bound<'_, PyType>, data: &str) -> PyResult<Self> {
        Ok(Self::from_str(data)?)
    }

    /// Serialize the schema to a JSON string.
    #[pyo3(name = "to_json_string")]
    fn to_json_string_py(&self) -> PyResult<String> {
        let data = serde_json::to_string_pretty(&self).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(data)
    }

    /// Build the schema in to a Pywr model.
    #[cfg(feature = "core")]
    #[pyo3(name="build", signature = (data_path=None, output_path=None))]
    fn build_py(&mut self, data_path: Option<PathBuf>, output_path: Option<PathBuf>) -> PyResult<Model> {
        let model = self.build_model(data_path.as_deref(), output_path.as_deref())?;
        Ok(model)
    }
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct MultiNetworkTransfer {
    pub from_network: String,
    pub metric: Metric,
    pub name: String,
    pub initial_value: Option<f64>,
}

#[derive(serde::Deserialize, serde::Serialize, Clone)]
pub struct MultiNetworkEntry {
    pub name: String,
    pub network: NetworkSchemaRef,
    pub transfers: Vec<MultiNetworkTransfer>,
}

#[derive(Error, Debug)]
#[cfg(feature = "core")]
pub enum MultiNetworkModelSchemaBuildError {
    #[error("Failed to construct scenario builder: {0}")]
    ScenarioBuilderError(#[from] pywr_core::scenario::ScenarioError),
    #[error("Failed to construct model domain: {0}")]
    CoreModelDomainError(#[from] pywr_core::models::ModelDomainError),
    #[error("Failed to construct the network `{name}`: {source}")]
    NetworkBuildError {
        name: String,
        #[source]
        source: Box<NetworkSchemaBuildError>,
    },
    #[error("Failed to read Pywr network from path `{path}`: {source}")]
    NetworkReadError {
        path: PathBuf,
        #[source]
        source: NetworkSchemaReadError,
    },
    #[error("Failed to add node `{name}` to the model: {source}")]
    AddTransferError {
        name: String,
        #[source]
        source: Box<SchemaError>,
    },
    #[error("Failed to add network `{name}` to the model: {source}")]
    AddNetworkError {
        name: String,
        #[source]
        source: MultiNetworkModelError,
    },
}

#[cfg(all(feature = "core", feature = "pyo3"))]
impl From<MultiNetworkModelSchemaBuildError> for PyErr {
    fn from(err: MultiNetworkModelSchemaBuildError) -> PyErr {
        let py_err = PyRuntimeError::new_err(err.to_string());

        // Check if the error has a cause that can be converted to a PyErr
        let py_cause: Result<PyErr, ()> = match err {
            MultiNetworkModelSchemaBuildError::NetworkBuildError { source, .. } => (*source).try_into(),
            _ => Err(()),
        };

        if let Ok(py_cause) = py_cause {
            // If the cause is a PyErr, set it as the cause of the PyErr
            return Python::attach(|py| {
                py_err.set_cause(py, Some(py_cause));
                py_err
            });
        }

        py_err
    }
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
/// A [`MultiNetworkModelSchema`] should be used in cases where there is a strong separation between
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
///     [`MultiNetworkModelSchema`] is roughly the sum of the individual networks. Whereas the time
///     solve a large linear program combining all the networks could be significantly longer.
///
/// # Example
///
/// The following example shows a model with networks with the inflow to "supply2" in the second
/// network defined as the flow to "demand1" in the first network.
///
/// ```json5
/// // model.json
#[doc = include_str!("../tests/multi1/model.json")]
/// // network1.json
#[doc = include_str!("../tests/multi1/network1.json")]
/// // network2.json
#[doc = include_str!("../tests/multi1/network2.json")]
/// ```
///
///
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Clone)]
#[cfg_attr(feature = "pyo3", pyclass)]
pub struct MultiNetworkModelSchema {
    pub metadata: Metadata,
    pub timestepper: Timestepper,
    pub scenarios: Option<ScenarioDomain>,
    pub networks: Vec<MultiNetworkEntry>,
}

impl FromStr for MultiNetworkModelSchema {
    type Err = ModelSchemaReadError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(serde_json::from_str(s)?)
    }
}

impl MultiNetworkModelSchema {
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
                timestep: Timestep::default(),
            },
            scenarios: None,
            networks: Vec::new(),
        }
    }
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, ModelSchemaReadError> {
        let data = std::fs::read_to_string(&path).map_err(|error| ModelSchemaReadError::IO {
            path: path.as_ref().to_path_buf(),
            error,
        })?;
        Ok(serde_json::from_str(data.as_str())?)
    }

    #[cfg(feature = "core")]
    pub fn build_model(
        &self,
        data_path: Option<&Path>,
        output_path: Option<&Path>,
    ) -> Result<MultiNetworkModel, MultiNetworkModelSchemaBuildError> {
        let timestepper = self.timestepper.clone().into();

        let scenario_builder = match &self.scenarios {
            Some(scenarios) => scenarios.clone().try_into()?,
            None => pywr_core::scenario::ScenarioDomainBuilder::default(),
        };

        let domain = ModelDomain::try_from(timestepper, scenario_builder)?;
        let mut networks = Vec::with_capacity(self.networks.len());
        let mut inter_network_transfers = Vec::new();
        let mut schemas: Vec<(NetworkSchema, LoadedTableCollection, LoadedTimeseriesCollection)> =
            Vec::with_capacity(self.networks.len());

        // First load all the networks
        // These will contain any parameters that are referenced by the inter-model transfers
        // Because of potential circular references, we need to load all the networks first.
        for network_entry in &self.networks {
            // Load the network itself
            let (network, schema, tables, timeseries) = match &network_entry.network {
                NetworkSchemaRef::Path(path) => {
                    let pth = if let Some(dp) = data_path {
                        if path.is_relative() {
                            dp.join(path)
                        } else {
                            path.clone()
                        }
                    } else {
                        path.clone()
                    };

                    let network_schema = NetworkSchema::from_path(&pth)
                        .map_err(|source| MultiNetworkModelSchemaBuildError::NetworkReadError { path: pth, source })?;

                    let (net, tables, timeseries) = network_schema
                        .build_network(&domain, data_path, output_path, &network_entry.transfers)
                        .map_err(|source| MultiNetworkModelSchemaBuildError::NetworkBuildError {
                            name: network_entry.name.clone(),
                            source: Box::new(source),
                        })?;

                    (net, network_schema, tables, timeseries)
                }
                NetworkSchemaRef::Inline(network_schema) => {
                    let (net, tables, timeseries) = network_schema
                        .build_network(&domain, data_path, output_path, &network_entry.transfers)
                        .map_err(|source| MultiNetworkModelSchemaBuildError::NetworkBuildError {
                            name: network_entry.name.clone(),
                            source: Box::new(source),
                        })?;

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
                    .ok_or_else(|| MultiNetworkModelSchemaBuildError::AddTransferError {
                        name: transfer.name.clone(),
                        source: Box::new(SchemaError::NetworkNotFound(transfer.from_network.clone())),
                    })?;

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

                let from_metric = transfer.metric.load(from_network, &args, None).map_err(|source| {
                    MultiNetworkModelSchemaBuildError::AddTransferError {
                        name: transfer.name.clone(),
                        source: Box::new(source),
                    }
                })?;

                inter_network_transfers.push((from_network_idx, from_metric, to_network_idx, transfer.initial_value));
            }
        }

        // Now construct the model from the loaded components
        let mut model = MultiNetworkModel::new(domain);

        for (name, network) in networks {
            model
                .add_network(&name, network)
                .map_err(|source| MultiNetworkModelSchemaBuildError::AddNetworkError { name, source })?;
        }

        for (from_network_idx, from_metric, to_network_idx, initial_value) in inter_network_transfers {
            model.add_inter_network_transfer(from_network_idx, from_metric, to_network_idx, initial_value);
        }

        Ok(model)
    }
}

#[cfg(feature = "pyo3")]
#[pymethods]
impl MultiNetworkModelSchema {
    #[new]
    fn new_py(title: &str, start: NaiveDateTime, end: NaiveDateTime) -> Self {
        let start = Date::DateTime(start);
        let end = Date::DateTime(end);

        Self::new(title, &start, &end)
    }

    /// Create a new schema object from a file path.
    #[classmethod]
    #[pyo3(name = "from_path")]
    fn from_path_py(_cls: &Bound<'_, PyType>, path: PathBuf) -> PyResult<Self> {
        Ok(Self::from_path(path)?)
    }

    ///  Create a new schema object from a JSON string.
    #[classmethod]
    #[pyo3(name = "from_json_string")]
    fn from_json_string_py(_cls: &Bound<'_, PyType>, data: &str) -> PyResult<Self> {
        Ok(Self::from_str(data)?)
    }

    /// Serialize the schema to a JSON string.
    #[pyo3(name = "to_json_string")]
    fn to_json_string_py(&self) -> PyResult<String> {
        let data = serde_json::to_string_pretty(&self).map_err(|e| PyRuntimeError::new_err(e.to_string()))?;
        Ok(data)
    }

    /// Build the schema in to a Pywr model.
    #[cfg(feature = "core")]
    #[pyo3(name="build", signature = (data_path=None, output_path=None))]
    fn build_py(
        &mut self,
        data_path: Option<PathBuf>,
        output_path: Option<PathBuf>,
    ) -> PyResult<pywr_core::models::MultiNetworkModel> {
        let model = self.build_model(data_path.as_deref(), output_path.as_deref())?;
        Ok(model)
    }
}

#[cfg(test)]
mod tests {
    use super::{ModelSchema, ScenarioDomain};
    use crate::model::Timestepper;
    use crate::visit::VisitPaths;
    use std::fs;
    use std::fs::read_to_string;
    use std::path::PathBuf;

    fn model_str() -> String {
        read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/simple1.json")).unwrap()
    }

    #[test]
    fn test_simple1_schema() {
        let data = model_str();
        let schema: ModelSchema = serde_json::from_str(&data).unwrap();

        assert_eq!(schema.network.nodes.len(), 3);
        assert_eq!(schema.network.edges.len(), 2);
    }

    #[test]
    fn test_date() {
        let timestepper_str = r#"
        {
            "start": "2015-01-01",
            "end": "2015-12-31",
            "timestep": {
              "type": "Days",
              "days": 1
            }
        }
        "#;

        let timestep: Timestepper = serde_json::from_str(timestepper_str).unwrap();

        match timestep.start {
            super::Date::Date(date) => {
                assert_eq!(date, chrono::NaiveDate::from_ymd_opt(2015, 1, 1).unwrap());
            }
            _ => panic!("Expected a date"),
        }

        match timestep.end {
            super::Date::Date(date) => {
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
            "timestep": {
                "type": "Hours",
                "hours": 1
            }
        }
        "#;

        let timestep: Timestepper = serde_json::from_str(timestepper_str).unwrap();

        match timestep.start {
            super::Date::DateTime(date_time) => {
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
            super::Date::DateTime(date_time) => {
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
        model_fn.push("tests/timeseries.json");

        let mut schema = ModelSchema::from_path(model_fn.as_path()).unwrap();

        let expected_paths = vec![PathBuf::from("inflow.csv"), PathBuf::from("timeseries-expected.csv")];

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
    fn test_scenario_domain_doc_examples() {
        let mut doc_examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        doc_examples.push("src/doc_examples");

        for entry in fs::read_dir(doc_examples).unwrap() {
            let p = entry.unwrap().path();
            if p.is_file() && p.file_name().unwrap().to_str().unwrap().starts_with("scenario_domain") {
                let data = read_to_string(&p).unwrap_or_else(|e| panic!("Failed to read file: {p:?}: {e}",));

                let _value: ScenarioDomain =
                    serde_json::from_str(&data).unwrap_or_else(|e| panic!("Failed to deserialize {p:?}: {e}",));
            }
        }
    }
}

#[cfg(test)]
#[cfg(feature = "core")]
mod core_tests {
    use super::{ModelSchema, MultiNetworkModelSchema};
    use crate::agg_funcs::AggFunc;
    use crate::metric::{Metric, ParameterReference};
    use crate::parameters::{AggregatedParameter, ConstantParameter, Parameter, ParameterMeta};
    use ndarray::{Array1, Array2, Axis};
    use pywr_core::{
        metric::MetricF64, recorders::AssertionF64Recorder, solvers::ClpSolver, test_utils::run_all_solvers,
    };
    use std::fs::read_to_string;
    use std::path::PathBuf;

    fn model_str() -> String {
        read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/simple1.json")).unwrap()
    }

    #[test]
    fn test_simple1_run() {
        let data = model_str();
        let schema: ModelSchema = serde_json::from_str(&data).unwrap();
        let mut model = schema.build_model(None, None).unwrap();

        let network = model.network_mut();
        assert_eq!(network.nodes().len(), 3);
        assert_eq!(network.edges().len(), 2);

        let demand1_idx = network.get_node_index_by_name("demand1", None).unwrap();

        let expected_values: Array1<f64> = [10.0; 365].to_vec().into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        let rec = AssertionF64Recorder::new(
            "assert-demand1",
            MetricF64::NodeInFlow(demand1_idx),
            expected_values,
            None,
            None,
        );
        network.add_recorder(Box::new(rec)).unwrap();

        // Test all solvers
        run_all_solvers(&model, &[], &[], &[]);
    }

    /// Test that a cycle in parameter dependencies does not load.
    #[test]
    fn test_cycle_error() {
        let data = model_str();
        let mut schema: ModelSchema = serde_json::from_str(&data).unwrap();

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
                    value: 10.0.into(),
                    variable: None,
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
        let mut schema: ModelSchema = serde_json::from_str(&data).unwrap();

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
                    value: 10.0.into(),
                    variable: None,
                }),
                Parameter::Constant(ConstantParameter {
                    meta: ParameterMeta {
                        name: "p2".to_string(),
                        comment: None,
                    },
                    value: 10.0.into(),
                    variable: None,
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
        model_fn.push("tests/multi1/model.json");

        let schema = MultiNetworkModelSchema::from_path(model_fn.as_path()).unwrap();
        let mut model = schema.build_model(model_fn.parent(), None).unwrap();

        // Add some recorders for the expected outputs
        let network_1_idx = model
            .get_network_index_by_name("network1")
            .expect("network 1 not found");
        let network_1 = model.network_mut(network_1_idx).expect("network 1 not found");
        let demand1_idx = network_1.get_node_index_by_name("demand1", None).unwrap();

        let expected_values: Array1<f64> = [10.0; 365].to_vec().into();
        let expected_values: Array2<f64> = expected_values.insert_axis(Axis(1));

        let rec = AssertionF64Recorder::new(
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

        let rec = AssertionF64Recorder::new(
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
        model_fn.push("tests/multi2/model.json");

        let schema = MultiNetworkModelSchema::from_path(model_fn.as_path()).unwrap();
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

        let rec = AssertionF64Recorder::new(
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

        let rec = AssertionF64Recorder::new(
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
