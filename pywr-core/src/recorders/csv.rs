use super::{MetricSetState, PywrError, Recorder, RecorderMeta, Timestep};
use crate::metric::Metric;
use crate::models::ModelDomain;
use crate::network::Network;
use crate::recorders::metric_set::MetricSetIndex;
use crate::scenario::ScenarioIndex;
use crate::state::State;
use std::any::Any;
use std::fs::File;
use std::ops::Deref;
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
    fn setup(&self, domain: &ModelDomain, network: &Network) -> Result<Option<Box<(dyn Any)>>, PywrError> {
        let mut writer = csv::Writer::from_path(&self.filename).map_err(|e| PywrError::CSVError(e.to_string()))?;

        let mut names = vec![];
        let mut sub_names = vec![];
        let mut attributes = vec![];

        let metric_set = network.get_metric_set(self.metric_set_idx)?;

        for metric in metric_set.iter_metrics() {
            let (name, sub_name, attribute) = match metric {
                Metric::NodeInFlow(idx) => {
                    let node = network.get_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "inflow".to_string())
                }
                Metric::NodeOutFlow(idx) => {
                    let node = network.get_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "outflow".to_string())
                }
                Metric::NodeVolume(idx) => {
                    let node = network.get_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "volume".to_string())
                }
                Metric::DerivedMetric(_idx) => {
                    todo!("Derived metrics are not yet supported in CSV recorders");
                }
                Metric::AggregatedNodeVolume(idx) => {
                    let node = network.get_aggregated_storage_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "volume".to_string())
                }
                Metric::EdgeFlow(_) => {
                    continue; // TODO
                }
                Metric::ParameterValue(idx) => {
                    let parameter = network.get_parameter(idx)?;
                    let name = parameter.name();
                    (name.to_string(), "".to_string(), "parameter".to_string())
                }
                Metric::VirtualStorageVolume(_) => {
                    continue; // TODO
                }
                Metric::Constant(_) => {
                    continue; // TODO
                }
                Metric::MultiParameterValue(_) => {
                    continue; // TODO
                }
                Metric::AggregatedNodeInFlow(idx) => {
                    let node = network.get_aggregated_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "inflow".to_string())
                }
                Metric::AggregatedNodeOutFlow(idx) => {
                    let node = network.get_aggregated_node(idx)?;
                    let (name, sub_name) = node.full_name();
                    let sub_name = sub_name.map_or("".to_string(), |sn| sn.to_string());

                    (name.to_string(), sub_name, "outflow".to_string())
                }
                Metric::MultiNodeInFlow { name, .. } => (name.to_string(), "".to_string(), "inflow".to_string()),
                Metric::MultiNodeOutFlow { name, .. } => (name.to_string(), "".to_string(), "outflow".to_string()),
                Metric::InterNetworkTransfer(_) => {
                    continue; // TODO
                }
            };

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
        if header_scenario_groups.len() > 0 {
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
        timestep: &Timestep,
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

        let mut row = vec![timestep.date.to_string()];

        // Iterate through all of the scenario's state
        for ms_scenario_states in metric_set_states.iter() {
            let metric_set_state = ms_scenario_states
                .get(*self.metric_set_idx.deref())
                .ok_or_else(|| PywrError::MetricSetIndexNotFound(self.metric_set_idx))?;

            if let Some(current_values) = metric_set_state.current_values() {
                let values = current_values
                    .iter()
                    .map(|v| format!("{:.2}", v.value))
                    .collect::<Vec<_>>();

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

    fn finalise(
        &self,
        _metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn Any>>,
    ) -> Result<(), PywrError> {
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
