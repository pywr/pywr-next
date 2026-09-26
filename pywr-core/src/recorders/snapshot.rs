use crate::models::ModelDomain;
use crate::network::{MetricSetIndex, Network, ResolutionMaps};
use crate::recorders::{
    MetricSetState, Recorder, RecorderBuilder, RecorderBuilderError, RecorderInternalState, RecorderMeta,
    RecorderSaveError, RecorderSetupError,
};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use crate::timestep::{TimeDomain, Timestep};
use std::collections::{HashMap, VecDeque};
use std::ops::Deref;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct SnapshotBuffer {
    meta: Arc<Mutex<Option<SnapshotMeta>>>,
    data_queue: Arc<Mutex<VecDeque<Snapshot>>>,
}

impl Default for SnapshotBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl SnapshotBuffer {
    pub fn new() -> Self {
        Self {
            meta: Arc::new(Mutex::new(None)),
            data_queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn push(&self, snapshot: Snapshot) {
        if let Ok(mut buffer) = self.data_queue.lock() {
            buffer.push_back(snapshot);
        }
    }

    pub fn drain(&self) -> Vec<Snapshot> {
        if let Ok(mut buffer) = self.data_queue.lock() {
            buffer.drain(..).collect()
        } else {
            Vec::new()
        }
    }

    pub fn meta(&self) -> Option<SnapshotMeta> {
        if let Ok(meta) = self.meta.lock() {
            meta.clone()
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct SnapshotMeta {
    /// The metadata for the metric sets in the snapshot
    pub metric_set_meta: SnapshotMetricSetMeta,
    /// Flattened list of scenarios in the snapshot, in the order they were added to the snapshot
    pub scenarios: Vec<ScenarioIndex>,
    pub time: TimeDomain,
}

#[derive(Debug, Clone)]
pub struct SnapshotMetricSetMeta {
    /// Name of the metric set
    pub names: Vec<String>,
    /// Mapping from metric set name to index in the snapshot data
    pub indices: HashMap<String, usize>,
    /// Mapping from metric set name to the list of metric names in the metric set
    pub contents: HashMap<String, Vec<SnapshotMetricSetItem>>,
}

#[derive(Debug, Clone)]
pub struct SnapshotMetricSetItem {
    pub name: String,
    pub attribute: String,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub timestep_index: usize,
    pub metric_sets: HashMap<String, SnapshotData>,
}

/// Data from a single metric set
#[derive(Debug, Clone)]
pub struct SnapshotData {
    pub data: Vec<Vec<f64>>,
}

#[derive(Debug)]
pub struct SnapshotRecorder {
    meta: RecorderMeta,
    // The metric sets that the snapshot recorder will record.
    metric_sets: Vec<MetricSetIndex>,
    buffer: SnapshotBuffer,
}

impl Recorder for SnapshotRecorder {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn name(&self) -> &str {
        &self.meta.name
    }

    fn setup(
        &self,
        domain: &ModelDomain,
        network: &Network,
    ) -> Result<Option<Box<dyn RecorderInternalState>>, RecorderSetupError> {
        // Populate the snapshot meta with the metric set names and indices
        let mut metric_set_names = Vec::new();
        let mut metric_set_indices = HashMap::new();
        let mut metric_set_contents = HashMap::new();

        for (i, metric_set_idx) in self.metric_sets.iter().enumerate() {
            let metric_set = network
                .metric_sets()
                .get(*metric_set_idx.deref())
                .ok_or(RecorderSetupError::MetricSetIndexNotFound { index: *metric_set_idx })?;

            metric_set_names.push(metric_set.name().to_string());
            metric_set_indices.insert(metric_set.name().to_string(), i);

            let contents = metric_set
                .iter_metrics()
                .map(|m| SnapshotMetricSetItem {
                    name: m.name().to_string(),
                    attribute: m.attribute().to_string(),
                })
                .collect::<Vec<_>>();

            metric_set_contents.insert(metric_set.name().to_string(), contents);
        }

        let metric_set_meta = SnapshotMetricSetMeta {
            names: metric_set_names,
            indices: metric_set_indices,
            contents: metric_set_contents,
        };

        // Store the metric set meta in the buffer
        if let Ok(mut meta) = self.buffer.meta.lock() {
            *meta = Some(SnapshotMeta {
                metric_set_meta,
                scenarios: domain.scenarios().indices().to_vec(),
                time: domain.time().clone(),
            });
        }

        Ok(None)
    }

    fn save(
        &self,
        timestep: &Timestep,
        _scenario_indices: &[ScenarioIndex],
        network: &Network,
        _state: &[State],
        metric_set_states: &[Vec<MetricSetState>],
        _internal_state: &mut Option<Box<dyn RecorderInternalState>>,
    ) -> Result<(), RecorderSaveError> {
        let mut data: HashMap<String, SnapshotData> = HashMap::new();

        for metric_set_idx in &self.metric_sets {
            let metric_set = network
                .metric_sets()
                .get(*metric_set_idx.deref())
                .ok_or(RecorderSaveError::MetricSetIndexNotFound { index: *metric_set_idx })?;

            let mut ms_data = Vec::new();

            // Iterate through all the scenario's state
            for ms_scenario_states in metric_set_states {
                let metric_set_state = ms_scenario_states
                    .get(*metric_set_idx.deref())
                    .ok_or(RecorderSaveError::MetricSetIndexNotFound { index: *metric_set_idx })?;

                if let Some(values) = metric_set_state.current_values() {
                    let values = values.iter().map(|v| v.value).collect::<Vec<f64>>();
                    ms_data.push(values);
                }
            }

            data.insert(metric_set.name().to_string(), SnapshotData { data: ms_data });
        }

        // If there is data send it!
        if !data.is_empty() {
            self.buffer.push(Snapshot {
                timestep_index: timestep.index,
                metric_sets: data,
            });
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct SnapshotRecorderBuilder {
    meta: RecorderMeta,
    metric_sets: Vec<String>,
    buffer: SnapshotBuffer,
}

impl SnapshotRecorderBuilder {
    pub fn new(name: &str, metric_sets: Vec<String>, buffer: SnapshotBuffer) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            metric_sets,
            buffer,
        }
    }
}

impl RecorderBuilder for SnapshotRecorderBuilder {
    fn name(&self) -> &str {
        &self.meta.name
    }

    fn build(self: Box<Self>, resolution_maps: &ResolutionMaps) -> Result<Box<dyn Recorder>, RecorderBuilderError> {
        let mut metric_set_indices = Vec::new();

        for metric_set in &self.metric_sets {
            let metric_set_idx =
                resolution_maps
                    .metric_sets
                    .get(metric_set)
                    .ok_or_else(|| RecorderBuilderError::MetricSetNotFound {
                        name: metric_set.clone(),
                    })?;

            metric_set_indices.push(*metric_set_idx);
        }

        Ok(Box::new(SnapshotRecorder {
            meta: self.meta,
            metric_sets: metric_set_indices,
            buffer: self.buffer.clone(),
        }))
    }
}
