mod aggregator;
mod csv;

#[cfg(feature = "hdf5")]
mod hdf;
mod memory;
mod metric_set;
mod py;

use crate::metric::{MetricF64, MetricF64Error, MetricU64, MetricU64Error};
use crate::models::ModelDomain;
use crate::network::Network;
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
use std::fmt;
use std::fmt::{Display, Formatter};
use std::ops::Deref;
use thiserror::Error;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct RecorderIndex(usize);

impl RecorderIndex {
    pub fn new(idx: usize) -> Self {
        Self(idx)
    }
}

impl Deref for RecorderIndex {
    type Target = usize;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for RecorderIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

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

pub struct Array2Recorder {
    meta: RecorderMeta,
    metric: MetricF64,
}

impl Array2Recorder {
    pub fn new(name: &str, metric: MetricF64) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            metric,
        }
    }
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

pub struct AssertionF64Recorder {
    meta: RecorderMeta,
    expected_values: Array2<f64>,
    metric: MetricF64,
    ulps: i64,
    epsilon: f64,
}

impl AssertionF64Recorder {
    pub fn new(
        name: &str,
        metric: MetricF64,
        expected_values: Array2<f64>,
        ulps: Option<i64>,
        epsilon: Option<f64>,
    ) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            expected_values,
            metric,
            ulps: ulps.unwrap_or(5),
            epsilon: epsilon.unwrap_or(1e-6),
        }
    }
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

pub struct AssertionU64Recorder {
    meta: RecorderMeta,
    expected_values: Array2<u64>,
    metric: MetricU64,
}

impl AssertionU64Recorder {
    pub fn new(name: &str, metric: MetricU64, expected_values: Array2<u64>) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            expected_values,
            metric,
        }
    }
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

pub struct AssertionFnRecorder<F> {
    meta: RecorderMeta,
    expected_func: F,
    metric: MetricF64,
    ulps: i64,
    epsilon: f64,
}

impl<F> AssertionFnRecorder<F>
where
    F: Fn(&Timestep, &ScenarioIndex) -> f64,
{
    pub fn new(name: &str, metric: MetricF64, expected_func: F, ulps: Option<i64>, epsilon: Option<f64>) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            expected_func,
            metric,
            ulps: ulps.unwrap_or(2),
            epsilon: epsilon.unwrap_or(f64::EPSILON * 2.0),
        }
    }
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

pub struct IndexAssertionRecorder {
    meta: RecorderMeta,
    expected_values: Array2<u64>,
    metric: MetricU64,
}

impl IndexAssertionRecorder {
    pub fn new(name: &str, metric: MetricU64, expected_values: Array2<u64>) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            expected_values,
            metric,
        }
    }
}

impl Recorder for IndexAssertionRecorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        network: &Network,
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

            let actual_value = self.metric.get_value(network, &state[scenario_index.simulation_id()])?;

            if actual_value != expected_value {
                panic!(
                    r#"assertion failed: (actual eq expected)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::{run_all_solvers, simple_model};

    #[test]
    fn test_array2_recorder() {
        let mut model = simple_model(2, None);

        let node_idx = model.network().get_node_index_by_name("input", None).unwrap();

        let rec = Array2Recorder::new("test", MetricF64::NodeOutFlow(node_idx));

        let _idx = model.network_mut().add_recorder(Box::new(rec)).unwrap();
        // Test all solvers
        run_all_solvers(&model, &[], &[], &[]);

        // TODO fix this with respect to the trait.
        // let array = rec.data_view2().unwrap();
        // assert_almost_eq!(array[[0, 0]], 10.0);
    }
}
