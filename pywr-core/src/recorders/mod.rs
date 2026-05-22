mod aggregator;
mod csv;

#[cfg(feature = "hdf5")]
mod hdf;
mod memory;
mod metric_set;
mod py;

use crate::metric::{
    MetricF64, MetricF64Error, MetricF64ResolutionError, MetricU64, MetricU64Error, MetricU64ResolutionError,
    UnresolvedMetricF64, UnresolvedMetricU64,
};
use crate::models::ModelDomain;
use crate::network::{Network, ResolutionMaps};
use crate::recorders::csv::CsvError;
#[cfg(feature = "hdf5")]
use crate::recorders::hdf::Hdf5Error;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::Timestep;
pub use aggregator::{AggregationFrequency, Aggregator, PeriodValue};
pub use csv::{CsvLongFmtOutput, CsvLongFmtRecord, CsvWideFmtOutput};
use float_cmp::{ApproxEq, F64Margin, approx_eq};
#[cfg(feature = "hdf5")]
pub use hdf::HDF5Recorder;
pub use memory::{Aggregation, AggregationError, AggregationOrder, MemoryRecorder};
pub use metric_set::{MetricSet, MetricSetIndex, MetricSetSaveError, MetricSetState, OutputMetric};
use ndarray::Array2;
use ndarray::prelude::*;
use polars::prelude::PolarsError;
use std::any::Any;
use thiserror::Error;

/// Meta data common to all parameters.
#[derive(Clone, Debug)]
pub struct RecorderMeta {
    pub name: String,
    pub comment: String,
}

impl RecorderMeta {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            comment: "".to_string(),
        }
    }
}

/// Errors returned by recorder setup.
#[derive(Error, Debug)]
pub enum RecorderSetupError {
    #[error("CSV error: {0}")]
    CSVError(#[from] CsvError),
    #[cfg(feature = "hdf5")]
    #[error("HDF5 error: {0}")]
    HDF5Error(#[from] Hdf5Error),
    #[error("Metric set index `{index}` not found")]
    MetricSetIndexNotFound { index: MetricSetIndex },
}

/// Errors returned by recorder saving.
#[derive(Error, Debug)]
pub enum RecorderSaveError {
    #[error("F64 metric error: {0}")]
    MetricF64Error(#[from] MetricF64Error),
    #[error("U64 metric error: {0}")]
    MetricU64Error(#[from] MetricU64Error),
    #[error("Metric set index `{index}` not found")]
    MetricSetIndexNotFound { index: MetricSetIndex },
    #[error("CSV error: {0}")]
    CSVError(#[from] CsvError),
    #[cfg(feature = "hdf5")]
    #[error("HDF5 error: {0}")]
    HDF5Error(#[from] Hdf5Error),
}

/// Errors returned by recorder saving.
#[derive(Error, Debug)]
pub enum RecorderFinaliseError {
    #[error("Metric set index `{index}` not found")]
    MetricSetIndexNotFound { index: MetricSetIndex },
    #[error("CSV error: {0}")]
    CSVError(#[from] CsvError),
    #[cfg(feature = "hdf5")]
    #[error("HDF5 error: {0}")]
    HDF5Error(#[from] Hdf5Error),
}

/// Errors returned by recorder aggregation.
#[derive(Error, Debug)]
pub enum RecorderAggregationError {
    #[error("Recorder does not supported aggregation")]
    RecorderDoesNotSupportAggregation,
    #[error("Error aggregating value for recorder `{name}`: {source}")]
    AggregationError {
        name: String,
        #[source]
        source: AggregationError,
    },
}

#[cfg(feature = "pyo3")]
impl From<RecorderAggregationError> for pyo3::PyErr {
    fn from(err: RecorderAggregationError) -> Self {
        pyo3::exceptions::PyRuntimeError::new_err(err.to_string())
    }
}

/// Errors returned by recorder aggregation.
#[derive(Error, Debug)]
pub enum RecorderDataFrameError {
    #[error("Recorder can not be converted to a dataframe")]
    RecorderCannotBeConvertedToDataFrame,
    #[error("Error creating dataframe for recorder `{name}`: {source}")]
    PolarsError {
        name: String,
        #[source]
        source: PolarsError,
    },
}

#[cfg(feature = "pyo3")]
impl From<RecorderDataFrameError> for pyo3::PyErr {
    fn from(err: RecorderDataFrameError) -> Self {
        pyo3::exceptions::PyRuntimeError::new_err(err.to_string())
    }
}

pub trait RecorderInternalState: Any {}
impl<T> RecorderInternalState for T where T: Any {}

/// Helper function to downcast to internal recorder state and print a helpful panic
/// message if this fails.
fn downcast_internal_state_mut<T: 'static>(internal_state: &mut Option<Box<dyn RecorderInternalState>>) -> &mut T {
    // Downcast the internal state to the correct type
    match internal_state {
        Some(internal) => match (internal.as_mut() as &mut dyn Any).downcast_mut::<T>() {
            Some(pa) => pa,
            None => panic!("Internal state did not downcast to the correct type! :("),
        },
        None => panic!("No internal state defined when one was expected! :("),
    }
}

/// Helper function to downcast to internal recorder state and print a helpful panic
/// message if this fails.
fn downcast_internal_state<T: 'static>(internal_state: Option<Box<dyn RecorderInternalState>>) -> Box<T> {
    // Downcast the internal state to the correct type
    match internal_state {
        Some(internal) => match (internal as Box<dyn Any>).downcast::<T>() {
            Ok(pa) => pa,
            Err(_) => panic!("Internal state did not downcast to the correct type! :("),
        },
        None => panic!("No internal state defined when one was expected! :("),
    }
}

/// Result of finalising a recorder.
///
/// This should be used to store any final results of the recorder, e.g. aggregated values or
/// data. The implementation of this trait can provide methods to access the data in a convenient way.
/// The data should be standalone and not require access to the model or other state.
pub trait RecorderFinalResult: Any + Send + Sync {
    fn aggregated_value(&self) -> Result<f64, RecorderAggregationError> {
        Err(RecorderAggregationError::RecorderDoesNotSupportAggregation)
    }

    fn to_dataframe(&self) -> Result<polars::prelude::DataFrame, RecorderDataFrameError> {
        Err(RecorderDataFrameError::RecorderCannotBeConvertedToDataFrame)
    }
}

pub trait Recorder: Send + Sync {
    fn meta(&self) -> &RecorderMeta;
    fn name(&self) -> &str {
        self.meta().name.as_str()
    }
    fn setup(
        &self,
        _domain: &ModelDomain,
        _model: &Network,
    ) -> Result<Option<Box<dyn RecorderInternalState>>, RecorderSetupError> {
        Ok(None)
    }
    fn before(&self) {}

    fn save(
        &self,
        _timestep: &Timestep,
        _scenario_indices: &[ScenarioIndex],
        _model: &Network,
        _state: &[State],
        _metric_set_states: &[Vec<MetricSetState>],
        _internal_state: &mut Option<Box<dyn RecorderInternalState>>,
    ) -> Result<(), RecorderSaveError> {
        Ok(())
    }

    /// Finalise the recorder, e.g. write out any remaining data and close files.
    ///
    /// This is called once after all timesteps have been processed. The internal state
    /// is consumed by this method and should be used to perform any house-keeping.
    fn finalise(
        &self,
        _network: &Network,
        _scenario_indices: &[ScenarioIndex],
        _metric_set_states: &[Vec<MetricSetState>],
        _internal_state: Option<Box<dyn RecorderInternalState>>,
    ) -> Result<Option<Box<dyn RecorderFinalResult>>, RecorderFinaliseError> {
        Ok(None)
    }
}

#[derive(Debug, Error)]
pub enum RecorderBuilderError {
    #[error("Could not resolve f64 metric for `{attr}` attribute: {source}")]
    ResolveMetricF64Error {
        attr: String,
        #[source]
        source: MetricF64ResolutionError,
    },
    #[error("Could not resolve u64 metric for `{attr}` attribute: {source}")]
    ResolveMetricU64Error {
        attr: String,
        #[source]
        source: MetricU64ResolutionError,
    },
}

pub trait RecorderBuilder {
    fn name(&self) -> &str;
    fn build(self: Box<Self>, resolution_maps: &ResolutionMaps) -> Result<Box<dyn Recorder>, RecorderBuilderError>;
}
pub struct Array2Recorder {
    meta: RecorderMeta,
    metric: MetricF64,
}

impl Recorder for Array2Recorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn setup(
        &self,
        domain: &ModelDomain,
        _model: &Network,
    ) -> Result<Option<Box<dyn RecorderInternalState>>, RecorderSetupError> {
        let array: Array2<f64> = Array::zeros((domain.time().len(), domain.scenarios().len()));

        let array: Box<dyn RecorderInternalState> = Box::new(array);
        Ok(Some(array))
    }

    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        model: &Network,
        state: &[State],
        _metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn RecorderInternalState>>,
    ) -> Result<(), RecorderSaveError> {
        // Downcast the internal state to the correct type
        let array = downcast_internal_state_mut::<Array2<f64>>(internal_state);

        // This panics if out-of-bounds
        for scenario_index in scenario_indices {
            let value = self.metric.get_value(model, &state[scenario_index.simulation_id()])?;
            array[[timestep.index, scenario_index.simulation_id()]] = value
        }

        Ok(())
    }
}

pub struct Array2RecorderBuilder {
    meta: RecorderMeta,
    metric: UnresolvedMetricF64,
}

impl Array2RecorderBuilder {
    pub fn new(name: &str, metric: UnresolvedMetricF64) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            metric,
        }
    }
}

impl RecorderBuilder for Array2RecorderBuilder {
    fn name(&self) -> &str {
        self.meta.name.as_str()
    }
    fn build(self: Box<Self>, resolution_maps: &ResolutionMaps) -> Result<Box<dyn Recorder>, RecorderBuilderError> {
        let metric =
            self.metric
                .resolve(resolution_maps)
                .map_err(|source| RecorderBuilderError::ResolveMetricF64Error {
                    attr: "metric".to_string(),
                    source,
                })?;

        let r = Array2Recorder {
            meta: self.meta,
            metric,
        };

        Ok(Box::new(r))
    }
}

pub struct AssertionF64Recorder {
    meta: RecorderMeta,
    expected_values: Array2<f64>,
    metric: MetricF64,
    ulps: i64,
    epsilon: f64,
}

impl Recorder for AssertionF64Recorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        model: &Network,
        state: &[State],
        _metric_set_states: &[Vec<MetricSetState>],
        _internal_state: &mut Option<Box<dyn RecorderInternalState>>,
    ) -> Result<(), RecorderSaveError> {
        // This panics if out-of-bounds

        for scenario_index in scenario_indices {
            let expected_value = match self
                .expected_values
                .get([timestep.index, scenario_index.simulation_id()])
            {
                Some(v) => *v,
                None => panic!("Simulation produced results out of range."),
            };

            let actual_value = self.metric.get_value(model, &state[scenario_index.simulation_id()])?;

            if !actual_value.approx_eq(
                expected_value,
                F64Margin {
                    epsilon: self.epsilon,
                    ulps: self.ulps,
                },
            ) {
                panic!(
                    r#"assertion failed: (actual approx_eq expected)
recorder: `{}`
timestep: `{:?}` ({})
scenario: `{:?}`
actual: `{:?}`
expected: `{:?}`"#,
                    self.meta.name,
                    timestep.date,
                    timestep.index,
                    scenario_index.simulation_id(),
                    actual_value,
                    expected_value
                )
            }
        }

        Ok(())
    }
}

pub struct AssertionF64RecorderBuilder {
    meta: RecorderMeta,
    expected_values: Array2<f64>,
    metric: UnresolvedMetricF64,
    ulps: i64,
    epsilon: f64,
}

impl AssertionF64RecorderBuilder {
    pub fn new(name: &str, metric: UnresolvedMetricF64, expected_values: Array2<f64>) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            expected_values,
            metric,
            ulps: 5,
            epsilon: 1e-6,
        }
    }

    pub fn ulps(&mut self, ulps: i64) -> &mut Self {
        self.ulps = ulps;
        self
    }

    pub fn epsilon(&mut self, epsilon: f64) -> &mut Self {
        self.epsilon = epsilon;
        self
    }
}

impl RecorderBuilder for AssertionF64RecorderBuilder {
    fn name(&self) -> &str {
        &self.meta.name
    }
    fn build(self: Box<Self>, resolution_maps: &ResolutionMaps) -> Result<Box<dyn Recorder>, RecorderBuilderError> {
        let metric =
            self.metric
                .resolve(resolution_maps)
                .map_err(|source| RecorderBuilderError::ResolveMetricF64Error {
                    attr: "metric".to_string(),
                    source,
                })?;

        let r = AssertionF64Recorder {
            meta: self.meta,
            expected_values: self.expected_values,
            metric,
            ulps: self.ulps,
            epsilon: self.epsilon,
        };

        Ok(Box::new(r))
    }
}

pub struct AssertionU64Recorder {
    meta: RecorderMeta,
    expected_values: Array2<u64>,
    metric: MetricU64,
}

impl Recorder for AssertionU64Recorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        model: &Network,
        state: &[State],
        _metric_set_states: &[Vec<MetricSetState>],
        _internal_state: &mut Option<Box<dyn RecorderInternalState>>,
    ) -> Result<(), RecorderSaveError> {
        // This panics if out-of-bounds

        for scenario_index in scenario_indices {
            let expected_value = match self
                .expected_values
                .get([timestep.index, scenario_index.simulation_id()])
            {
                Some(v) => *v,
                None => panic!("Simulation produced results out of range."),
            };

            let actual_value = self.metric.get_value(model, &state[scenario_index.simulation_id()])?;

            if actual_value != expected_value {
                panic!(
                    r#"assertion failed: (actual approx_eq expected)
recorder: `{}`
timestep: `{:?}` ({})
scenario: `{:?}`
actual: `{:?}`
expected: `{:?}`"#,
                    self.meta.name,
                    timestep.date,
                    timestep.index,
                    scenario_index.simulation_id(),
                    actual_value,
                    expected_value
                )
            }
        }

        Ok(())
    }
}

pub struct AssertionU64RecorderBuilder {
    meta: RecorderMeta,
    expected_values: Array2<u64>,
    metric: UnresolvedMetricU64,
}

impl AssertionU64RecorderBuilder {
    pub fn new(name: &str, metric: UnresolvedMetricU64, expected_values: Array2<u64>) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            expected_values,
            metric,
        }
    }
}

impl RecorderBuilder for AssertionU64RecorderBuilder {
    fn name(&self) -> &str {
        &self.meta.name
    }
    fn build(self: Box<Self>, resolution_maps: &ResolutionMaps) -> Result<Box<dyn Recorder>, RecorderBuilderError> {
        let metric =
            self.metric
                .resolve(resolution_maps)
                .map_err(|source| RecorderBuilderError::ResolveMetricU64Error {
                    attr: "metric".to_string(),
                    source,
                })?;

        let r = AssertionU64Recorder {
            meta: self.meta,
            expected_values: self.expected_values,
            metric,
        };

        Ok(Box::new(r))
    }
}

pub struct AssertionFnRecorder<F> {
    meta: RecorderMeta,
    expected_func: F,
    metric: MetricF64,
    ulps: i64,
    epsilon: f64,
}

impl<F> Recorder for AssertionFnRecorder<F>
where
    F: Send + Sync + Fn(&Timestep, &ScenarioIndex) -> f64,
{
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        model: &Network,
        state: &[State],
        _metric_set_states: &[Vec<MetricSetState>],
        _internal_state: &mut Option<Box<dyn RecorderInternalState>>,
    ) -> Result<(), RecorderSaveError> {
        // This panics if out-of-bounds

        for scenario_index in scenario_indices {
            let expected_value = (self.expected_func)(timestep, scenario_index);
            let actual_value = self.metric.get_value(model, &state[scenario_index.simulation_id()])?;

            if !approx_eq!(
                f64,
                actual_value,
                expected_value,
                epsilon = self.epsilon,
                ulps = self.ulps
            ) {
                panic!(
                    r#"assertion failed at timestep {timestep:?} in scenario {scenario_index:?}: `(actual approx_eq expected)`
   actual: `{actual_value:?}`,
 expected: `{expected_value:?}`"#,
                )
            }
        }

        Ok(())
    }
}

pub struct AssertionFnRecorderBuilder<F> {
    meta: RecorderMeta,
    expected_func: F,
    metric: UnresolvedMetricF64,
    ulps: i64,
    epsilon: f64,
}

impl<F> AssertionFnRecorderBuilder<F>
where
    F: Fn(&Timestep, &ScenarioIndex) -> f64,
{
    pub fn new(name: &str, metric: UnresolvedMetricF64, expected_func: F) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            expected_func,
            metric,
            ulps: 2,
            epsilon: f64::EPSILON * 2.0,
        }
    }

    pub fn ulps(&mut self, ulps: i64) -> &mut Self {
        self.ulps = ulps;
        self
    }

    pub fn epsilon(&mut self, epsilon: f64) -> &mut Self {
        self.epsilon = epsilon;
        self
    }
}

impl<F> RecorderBuilder for AssertionFnRecorderBuilder<F>
where
    F: Send + Sync + Fn(&Timestep, &ScenarioIndex) -> f64 + 'static,
{
    fn name(&self) -> &str {
        &self.meta.name
    }
    fn build(self: Box<Self>, resolution_maps: &ResolutionMaps) -> Result<Box<dyn Recorder>, RecorderBuilderError> {
        let metric =
            self.metric
                .resolve(resolution_maps)
                .map_err(|source| RecorderBuilderError::ResolveMetricF64Error {
                    attr: "metric".to_string(),
                    source,
                })?;

        let r = AssertionFnRecorder {
            meta: self.meta,
            expected_func: self.expected_func,
            metric,
            ulps: self.ulps,
            epsilon: self.epsilon,
        };

        Ok(Box::new(r))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{run_all_solvers, simple_model};

    #[test]
    fn test_array2_recorder() {
        let mut model_builder = simple_model(2, None);

        let rec = Array2RecorderBuilder::new("test", UnresolvedMetricF64::NodeOutFlow("input".into()));

        model_builder.network_builder().recorder(Box::new(rec));

        let model = model_builder.build().unwrap();

        // Test all solvers
        run_all_solvers(&model, &[], &[], &[]);

        // TODO fix this with respect to the trait.
        // let array = rec.data_view2().unwrap();
        // assert_almost_eq!(array[[0, 0]], 10.0);
    }
}
