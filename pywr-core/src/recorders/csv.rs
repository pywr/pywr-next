use super::{MetricSetState, PywrError, Recorder, RecorderMeta, Timestep};
use crate::models::ModelDomain;
use crate::network::Network;
use crate::recorders::metric_set::MetricSetIndex;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::fs::File;
use std::ops::Deref;
use std::path::PathBuf;

/// Output the values from a [`MetricSet`] to a CSV file.
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

    fn write_values(
        &self,
        metric_set_states: &[Vec<MetricSetState>],
        internal: &mut Internal,
    ) -> Result<(), PywrError> {
        let mut row = Vec::new();

        // Iterate through all scenario's state
        for ms_scenario_states in metric_set_states.iter() {
            let metric_set_state = ms_scenario_states
                .get(*self.metric_set_idx.deref())
                .ok_or(PywrError::MetricSetIndexNotFound(self.metric_set_idx))?;

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
            internal
                .writer
                .write_record(row)
                .map_err(|e| PywrError::CSVError(e.to_string()))?;
        }

        Ok(())
    }
}

impl Recorder for CsvWideFmtOutput {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }
    fn setup(&self, domain: &ModelDomain, network: &Network) -> Result<Option<Box<(dyn Any)>>, PywrError> {
        let mut writer = csv::Writer::from_path(&self.filename).map_err(|e| PywrError::CSVError(e.to_string()))?;

        let mut names = vec![];
        let mut sub_names = vec![];
        let mut attributes = vec![];

        let metric_set = network.get_metric_set(self.metric_set_idx)?;

        for metric in metric_set.iter_metrics() {
            let name = metric.name(network)?.to_string();
            let sub_name = metric
                .sub_name(network)?
                .map_or_else(|| "".to_string(), |s| s.to_string());
            let attribute = metric.attribute().to_string();

            // Add entries for each scenario
            names.push(name);
            sub_names.push(sub_name);
            attributes.push(attribute);
        }

        // These are the header rows in the CSV file; we start each
        let mut header_name = vec!["node".to_string()];
        let mut header_sub_name = vec!["sub-node".to_string()];
        let mut header_attribute = vec!["attribute".to_string()];
        let mut header_scenario = vec!["global-scenario-index".to_string()];

        // This is a vec of vec for each scenario group
        let mut header_scenario_groups = Vec::new();
        for group_name in domain.scenarios().group_names() {
            header_scenario_groups.push(vec![format!("scenario-group: {}", group_name)]);
        }

        for scenario_index in domain.scenarios().indices().iter() {
            // Repeat the names, sub-names and attributes for every scenario
            header_name.extend(names.clone());
            header_sub_name.extend(sub_names.clone());
            header_attribute.extend(attributes.clone());
            header_scenario.extend(vec![format!("{}", scenario_index.index); names.len()]);

            for (group_idx, idx) in scenario_index.indices.iter().enumerate() {
                header_scenario_groups[group_idx].extend(vec![format!("{}", idx); names.len()]);
            }
        }

        writer
            .write_record(header_name)
            .map_err(|e| PywrError::CSVError(e.to_string()))?;
        writer
            .write_record(header_sub_name)
            .map_err(|e| PywrError::CSVError(e.to_string()))?;
        writer
            .write_record(header_attribute)
            .map_err(|e| PywrError::CSVError(e.to_string()))?;
        writer
            .write_record(header_scenario)
            .map_err(|e| PywrError::CSVError(e.to_string()))?;

        // There could be no scenario groups defined
        if header_scenario_groups.is_empty() {
            for group in header_scenario_groups {
                writer
                    .write_record(group)
                    .map_err(|e| PywrError::CSVError(e.to_string()))?;
            }
        }

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
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        let internal = match internal_state {
            Some(internal) => match internal.downcast_mut::<Internal>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        self.write_values(metric_set_states, internal)?;

        Ok(())
    }

    fn finalise(
        &self,
        _network: &Network,
        metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        // This will leave the internal state with a `None` because we need to take
        // ownership of the file handle in order to close it.
        match internal_state.take() {
            Some(mut internal) => {
                if let Some(internal) = internal.downcast_mut::<Internal>() {
                    self.write_values(metric_set_states, internal)?;
                    Ok(())
                } else {
                    panic!("Internal state did not downcast to the correct type! :(");
                }
            }
            None => panic!("No internal state defined when one was expected! :("),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CsvLongFmtRecord {
    time_start: NaiveDateTime,
    time_end: NaiveDateTime,
    scenario_index: usize,
    metric_set: String,
    name: String,
    sub_name: String,
    attribute: String,
    value: f64,
}

/// Output the values from a several [`MetricSet`]s to a CSV file in long format.
///
/// The long format contains a row for each value produced by the metric set. This is useful
/// for analysis in tools like R or Python which can easily read long format data.
///
#[derive(Clone, Debug)]
pub struct CsvLongFmtOutput {
    meta: RecorderMeta,
    filename: PathBuf,
    metric_set_indices: Vec<MetricSetIndex>,
}

impl CsvLongFmtOutput {
    pub fn new<P: Into<PathBuf>>(name: &str, filename: P, metric_set_indices: &[MetricSetIndex]) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            filename: filename.into(),
            metric_set_indices: metric_set_indices.to_vec(),
        }
    }

    fn write_values(
        &self,
        network: &Network,
        metric_set_states: &[Vec<MetricSetState>],
        internal: &mut Internal,
    ) -> Result<(), PywrError> {
        // Iterate through all the scenario's state
        for (scenario_idx, ms_scenario_states) in metric_set_states.iter().enumerate() {
            for metric_set_idx in self.metric_set_indices.iter() {
                let metric_set_state = ms_scenario_states
                    .get(*metric_set_idx.deref())
                    .ok_or(PywrError::MetricSetIndexNotFound(*metric_set_idx))?;

                if let Some(current_values) = metric_set_state.current_values() {
                    let metric_set = network.get_metric_set(*metric_set_idx)?;

                    for (metric, value) in metric_set.iter_metrics().zip(current_values.iter()) {
                        let name = metric.name(network)?.to_string();
                        let sub_name = metric
                            .sub_name(network)?
                            .map_or_else(|| "".to_string(), |s| s.to_string());
                        let attribute = metric.attribute().to_string();

                        let record = CsvLongFmtRecord {
                            time_start: value.start,
                            time_end: value.end(),
                            scenario_index: scenario_idx,
                            metric_set: metric_set.name().to_string(),
                            name,
                            sub_name,
                            attribute,
                            value: value.value,
                        };

                        internal
                            .writer
                            .serialize(record)
                            .map_err(|e| PywrError::CSVError(e.to_string()))?;
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
    fn setup(&self, _domain: &ModelDomain, _network: &Network) -> Result<Option<Box<(dyn Any)>>, PywrError> {
        let writer = csv::Writer::from_path(&self.filename).map_err(|e| PywrError::CSVError(e.to_string()))?;

        let internal = Internal { writer };

        Ok(Some(Box::new(internal)))
    }

    fn save(
        &self,
        _timestep: &Timestep,
        _scenario_indices: &[ScenarioIndex],
        network: &Network,
        _state: &[State],
        metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        let internal = match internal_state {
            Some(internal) => match internal.downcast_mut::<Internal>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        self.write_values(network, metric_set_states, internal)?;

        Ok(())
    }

    fn finalise(
        &self,
        network: &Network,
        metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        // This will leave the internal state with a `None` because we need to take
        // ownership of the file handle in order to close it.
        match internal_state.take() {
            Some(mut internal) => {
                if let Some(internal) = internal.downcast_mut::<Internal>() {
                    self.write_values(network, metric_set_states, internal)?;
                    Ok(())
                } else {
                    panic!("Internal state did not downcast to the correct type! :(");
                }
            }
            None => panic!("No internal state defined when one was expected! :("),
        }
    }
}
