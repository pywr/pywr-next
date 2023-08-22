use super::{PywrError, Recorder, RecorderMeta, Timestep};
use crate::metric::Metric;
use crate::model::Model;
use crate::recorders::metric_set::{MetricSet, MetricSetIndex};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use std::any::Any;
use std::fs::File;
use std::path::PathBuf;

/// Output the values from a [`MetricSet`] to a CSV file.
#[derive(Clone, Debug)]
pub struct CSVRecorder {
    meta: RecorderMeta,
    filename: PathBuf,
    metric_set_idx: MetricSetIndex,
}

struct Internal {
    writer: csv::Writer<File>,
}

impl CSVRecorder {
    pub fn new<P: Into<PathBuf>>(name: &str, filename: P, metric_set_idx: MetricSetIndex) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            filename: filename.into(),
            metric_set_idx,
        }
    }
}

impl Recorder for CSVRecorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }
    fn setup(
        &self,
        _timesteps: &[Timestep],
        scenario_indices: &[ScenarioIndex],
        model: &Model,
    ) -> Result<Option<Box<(dyn Any)>>, PywrError> {
        let mut writer = csv::Writer::from_path(&self.filename).map_err(|e| PywrError::CSVError(e.to_string()))?;

        let num_scenarios = scenario_indices.len();
        // TODO this could write a header row for each scenario group instead of the global index
        let scenario_headers = scenario_indices.iter().map(|si| format!("{}", si.index));

        // These are the header rows in the CSV file; we start each
        let mut header_name = vec!["node".to_string()];
        let mut header_sub_name = vec!["sub-node".to_string()];
        let mut header_scenario = vec!["global-scenario-index".to_string()];
        let mut header_attribute = vec!["attribute".to_string()];

        let metric_set = model.get_metric_set(self.metric_set_idx)?;

        for metric in metric_set.iter_metrics() {
            let (name, sub_name, attribute) = match metric {
                Metric::NodeInFlow(idx) => {
                    let node = model.get_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "inflow".to_string())
                }
                Metric::NodeOutFlow(idx) => {
                    let node = model.get_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "outflow".to_string())
                }
                Metric::NodeVolume(idx) => {
                    let node = model.get_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "volume".to_string())
                }
                Metric::NodeProportionalVolume(idx) => {
                    let node = model.get_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "proportional-volume".to_string())
                }
                Metric::AggregatedNodeVolume(idx) => {
                    let node = model.get_aggregated_storage_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "volume".to_string())
                }
                Metric::AggregatedNodeProportionalVolume(idx) => {
                    let node = model.get_aggregated_storage_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "proportional-volume".to_string())
                }
                Metric::EdgeFlow(_) => {
                    continue; // TODO
                }
                Metric::ParameterValue(idx) => {
                    let parameter = model.get_parameter(idx)?;
                    let name = parameter.name();
                    (name.to_string(), "".to_string(), "parameter".to_string())
                }
                Metric::VirtualStorageVolume(_) => {
                    continue; // TODO
                }
                Metric::VirtualStorageProportionalVolume(_) => {
                    continue; // TODO
                }
                Metric::Constant(_) => {
                    continue; // TODO
                }
                Metric::MultiParameterValue(_) => {
                    continue; // TODO
                }
                Metric::AggregatedNodeInFlow(idx) => {
                    let node = model.get_aggregated_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "inflow".to_string())
                }
                Metric::AggregatedNodeOutFlow(idx) => {
                    let node = model.get_aggregated_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "outflow".to_string())
                }
                Metric::NodeInFlowDeficit(idx) => {
                    let node = model.get_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "inflow-deficit".to_string())
                }
                Metric::VolumeBetweenControlCurves(_) => {
                    todo!("Recording VolumeBetweenControlCurves not implemented.")
                }
                Metric::MultiNodeInFlow { name, sub_name, .. } => (
                    name.to_string(),
                    sub_name.clone().unwrap_or("".to_string()),
                    "inflow".to_string(),
                ),
            };

            // Add entries for each scenario
            header_name.extend(vec![name; num_scenarios]);
            header_sub_name.extend(vec![sub_name; num_scenarios]);
            header_scenario.extend(scenario_headers.clone());
            header_attribute.extend(vec![attribute; num_scenarios]);
        }

        writer
            .write_record(header_name)
            .map_err(|e| PywrError::CSVError(e.to_string()))?;
        writer
            .write_record(header_sub_name)
            .map_err(|e| PywrError::CSVError(e.to_string()))?;
        writer
            .write_record(header_scenario)
            .map_err(|e| PywrError::CSVError(e.to_string()))?;
        writer
            .write_record(header_attribute)
            .map_err(|e| PywrError::CSVError(e.to_string()))?;

        let internal = Internal { writer };

        Ok(Some(Box::new(internal)))
    }

    fn save(
        &self,
        timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        model: &Model,
        state: &[State],
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
        let internal = match internal_state {
            Some(internal) => match internal.downcast_mut::<Internal>() {
                Some(pa) => pa,
                None => panic!("Internal state did not downcast to the correct type! :("),
            },
            None => panic!("No internal state defined when one was expected! :("),
        };

        let mut row = vec![timestep.date.to_string()];

        let metric_set = model.get_metric_set(self.metric_set_idx)?;

        for metric in metric_set.iter_metrics() {
            // Combine all the values for metric across all of the scenarios
            let values = scenario_indices
                .iter()
                .zip(state)
                .map(|(_, s)| metric.get_value(model, s).map(|v| format!("{:.2}", v)))
                .collect::<Result<Vec<_>, _>>()?;

            row.extend(values);
        }

        internal
            .writer
            .write_record(row)
            .map_err(|e| PywrError::CSVError(e.to_string()))?;
        Ok(())
    }

    fn finalise(&self, internal_state: &mut Option<Box<dyn Any>>) -> Result<(), PywrError> {
        // This will leave the internal state with a `None` because we need to take
        // ownership of the file handle in order to close it.
        match internal_state.take() {
            Some(internal) => {
                if let Ok(_internal) = internal.downcast::<Internal>() {
                    Ok(())
                } else {
                    panic!("Internal state did not downcast to the correct type! :(");
                }
            }
            None => panic!("No internal state defined when one was expected! :("),
        }
    }
}
