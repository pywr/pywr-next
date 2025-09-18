use super::{
    MetricSetState, Recorder, RecorderFinalResult, RecorderFinaliseError, RecorderInternalState, RecorderMeta,
    RecorderSaveError, RecorderSetupError, Timestep, downcast_internal_state, downcast_internal_state_mut,
};
use crate::models::ModelDomain;
use crate::network::Network;
use crate::recorders::metric_set::MetricSetIndex;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::num::NonZeroU32;
use std::ops::Deref;
use std::path::PathBuf;
use thiserror::Error;

/// Errors returned by recorder saving.
#[derive(Error, Debug)]
pub enum CsvError {
    #[error("Metric set index `{index}` not found")]
    MetricSetIndexNotFound { index: MetricSetIndex },
    #[error("CSV error with file at `{path}`: {source}")]
    CSVError {
        path: PathBuf,
        #[source]
        source: ::csv::Error,
    },
}

/// Output the values from a [`crate::recorders::MetricSet`] to a CSV file.
#[derive(Clone, Debug)]
pub struct CsvWideFmtOutput {
    meta: RecorderMeta,
    filename: PathBuf,
    metric_set_idx: MetricSetIndex,
}

struct Internal {
    writer: csv::Writer<File>,
}

impl CsvWideFmtOutput {
    pub fn new<P: Into<PathBuf>>(name: &str, filename: P, metric_set_idx: MetricSetIndex) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            filename: filename.into(),
            metric_set_idx,
        }
    }

    fn write_values(&self, metric_set_states: &[Vec<MetricSetState>], internal: &mut Internal) -> Result<(), CsvError> {
        let mut row = Vec::new();

        // Iterate through all scenario's state
        for ms_scenario_states in metric_set_states.iter() {
            let metric_set_state =
                ms_scenario_states
                    .get(*self.metric_set_idx.deref())
                    .ok_or(CsvError::MetricSetIndexNotFound {
                        index: self.metric_set_idx,
                    })?;

            if let Some(current_values) = metric_set_state.current_values() {
                let values = current_values
                    .iter()
                    .map(|v| format!("{:.2}", v.value))
                    .collect::<Vec<_>>();

                // If the row is empty, add the start time
                if row.is_empty() {
                    row.push(current_values.first().unwrap().start.to_string())
                }

                row.extend(values);
            }
        }

        // Only write
        if row.len() > 1 {
            internal.writer.write_record(row).map_err(|source| CsvError::CSVError {
                path: self.filename.clone(),
                source,
            })?;
        }

        Ok(())
    }
}

impl Recorder for CsvWideFmtOutput {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }
    fn setup(
        &self,
        domain: &ModelDomain,
        network: &Network,
    ) -> Result<Option<Box<dyn RecorderInternalState>>, RecorderSetupError> {
        let mut writer = csv::Writer::from_path(&self.filename).map_err(|source| CsvError::CSVError {
            path: self.filename.clone(),
            source,
        })?;

        let mut names = vec![];
        let mut attributes = vec![];

        let metric_set =
            network
                .get_metric_set(self.metric_set_idx)
                .ok_or_else(|| RecorderSetupError::MetricSetIndexNotFound {
                    index: self.metric_set_idx,
                })?;

        for metric in metric_set.iter_metrics() {
            let name = metric.name().to_string();
            let attribute = metric.attribute().to_string();

            // Add entries for each scenario
            names.push(name);
            attributes.push(attribute);
        }

        // These are the header rows in the CSV file; we start each
        let mut header_name = vec!["node".to_string()];
        let mut header_attribute = vec!["attribute".to_string()];
        let mut header_scenario = vec!["simulation_id".to_string()];
        let mut header_label = vec!["label".to_string()];

        for scenario_index in domain.scenarios().indices().iter() {
            // Repeat the names, sub-names and attributes for every scenario
            header_name.extend(names.clone());
            header_attribute.extend(attributes.clone());
            header_scenario.extend(vec![format!("{}", scenario_index.simulation_id()); names.len()]);
            header_label.extend(vec![format!("{}", scenario_index.label()); names.len()]);
        }

        writer.write_record(header_name).map_err(|source| CsvError::CSVError {
            path: self.filename.clone(),
            source,
        })?;

        writer
            .write_record(header_attribute)
            .map_err(|source| CsvError::CSVError {
                path: self.filename.clone(),
                source,
            })?;
        writer
            .write_record(header_scenario)
            .map_err(|source| CsvError::CSVError {
                path: self.filename.clone(),
                source,
            })?;
        writer.write_record(header_label).map_err(|source| CsvError::CSVError {
            path: self.filename.clone(),
            source,
        })?;

        let internal = Internal { writer };

        Ok(Some(Box::new(internal)))
    }

    fn save(
        &self,
        _timestep: &Timestep,
        _scenario_indices: &[ScenarioIndex],
        _network: &Network,
        _state: &[State],
        metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn RecorderInternalState>>,
    ) -> Result<(), RecorderSaveError> {
        let internal = downcast_internal_state_mut::<Internal>(internal_state);

        self.write_values(metric_set_states, internal)?;

        Ok(())
    }

    fn finalise(
        &self,
        _network: &Network,
        _scenario_indices: &[ScenarioIndex],
        metric_set_states: &[Vec<MetricSetState>],
        internal_state: Option<Box<dyn RecorderInternalState>>,
    ) -> Result<Option<Box<dyn RecorderFinalResult>>, RecorderFinaliseError> {
        let mut internal = downcast_internal_state::<Internal>(internal_state);
        self.write_values(metric_set_states, &mut internal)?;
        Ok(None)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CsvLongFmtRecord {
    time_start: NaiveDateTime,
    time_end: NaiveDateTime,
    simulation_id: usize,
    label: String,
    metric_set: String,
    name: String,
    attribute: String,
    value: f64,
}

/// Output the values from a several [`crate::recorders::MetricSet`]s to a CSV file in long format.
///
/// The long format contains a row for each value produced by the metric set. This is useful
/// for analysis in tools like R or Python which can easily read long format data.
///
#[derive(Clone, Debug)]
pub struct CsvLongFmtOutput {
    meta: RecorderMeta,
    filename: PathBuf,
    metric_set_indices: Vec<MetricSetIndex>,
    decimal_places: Option<NonZeroU32>,
}

impl CsvLongFmtOutput {
    pub fn new<P: Into<PathBuf>>(
        name: &str,
        filename: P,
        metric_set_indices: &[MetricSetIndex],
        decimal_places: Option<NonZeroU32>,
    ) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            filename: filename.into(),
            metric_set_indices: metric_set_indices.to_vec(),
            decimal_places,
        }
    }

    fn write_values(
        &self,
        network: &Network,
        scenario_indices: &[ScenarioIndex],
        metric_set_states: &[Vec<MetricSetState>],
        internal: &mut Internal,
    ) -> Result<(), CsvError> {
        // Iterate through all the scenario's state
        for (scenario_index, ms_scenario_states) in scenario_indices.iter().zip(metric_set_states.iter()) {
            for metric_set_idx in self.metric_set_indices.iter() {
                let metric_set_state = ms_scenario_states
                    .get(*metric_set_idx.deref())
                    .ok_or(CsvError::MetricSetIndexNotFound { index: *metric_set_idx })?;

                if let Some(current_values) = metric_set_state.current_values() {
                    let metric_set = network
                        .get_metric_set(*metric_set_idx)
                        .ok_or(CsvError::MetricSetIndexNotFound { index: *metric_set_idx })?;

                    for (metric, value) in metric_set.iter_metrics().zip(current_values.iter()) {
                        let name = metric.name().to_string();
                        let attribute = metric.attribute().to_string();

                        let value_scaled = if let Some(decimal_places) = self.decimal_places {
                            let scale = 10.0_f64.powi(decimal_places.get() as i32);
                            (value.value * scale).round() / scale
                        } else {
                            value.value
                        };

                        let record = CsvLongFmtRecord {
                            time_start: value.start,
                            time_end: value.end(),
                            simulation_id: scenario_index.simulation_id(),
                            label: scenario_index.label(),
                            metric_set: metric_set.name().to_string(),
                            name,
                            attribute,
                            value: value_scaled,
                        };

                        internal.writer.serialize(record).map_err(|source| CsvError::CSVError {
                            path: self.filename.clone(),
                            source,
                        })?;
                    }
                }
            }
        }

        Ok(())
    }
}

impl Recorder for CsvLongFmtOutput {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }
    fn setup(
        &self,
        _domain: &ModelDomain,
        _network: &Network,
    ) -> Result<Option<Box<dyn RecorderInternalState>>, RecorderSetupError> {
        let writer = csv::Writer::from_path(&self.filename).map_err(|source| CsvError::CSVError {
            path: self.filename.clone(),
            source,
        })?;

        let internal = Internal { writer };

        Ok(Some(Box::new(internal)))
    }

    fn save(
        &self,
        _timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        network: &Network,
        _state: &[State],
        metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn RecorderInternalState>>,
    ) -> Result<(), RecorderSaveError> {
        let internal = downcast_internal_state_mut::<Internal>(internal_state);

        self.write_values(network, scenario_indices, metric_set_states, internal)?;

        Ok(())
    }

    fn finalise(
        &self,
        network: &Network,
        scenario_indices: &[ScenarioIndex],
        metric_set_states: &[Vec<MetricSetState>],
        internal_state: Option<Box<dyn RecorderInternalState>>,
    ) -> Result<Option<Box<dyn RecorderFinalResult>>, RecorderFinaliseError> {
        let mut internal = downcast_internal_state::<Internal>(internal_state);
        self.write_values(network, scenario_indices, metric_set_states, &mut internal)?;
        Ok(None)
    }
}
