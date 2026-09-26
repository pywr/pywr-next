use super::{
    MetricSetState, OutputMetric, Recorder, RecorderBuilder, RecorderBuilderError, RecorderFinalResult,
    RecorderFinaliseError, RecorderInternalState, RecorderMeta, RecorderSaveError, RecorderSetupError, Timestep,
    downcast_internal_state, downcast_internal_state_mut, jiff_datetime_to_arrow_timestamp_ms,
};
use crate::models::ModelDomain;
use crate::network::{MetricSetIndex, Network, ResolutionMaps};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use arrow::array::{ArrayRef, Float64Array, StringArray, TimestampMillisecondArray, UInt64Array};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::error::ArrowError;
use arrow::ipc::writer::StreamWriter;
use arrow::record_batch::RecordBatch;
use arrow_schema::extension::{EXTENSION_TYPE_METADATA_KEY, EXTENSION_TYPE_NAME_KEY, ExtensionType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::num::NonZeroUsize;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use thiserror::Error;

/// Metadata identifying a metric column in an [`ArrowStreamOutput`] schema.
///
/// The physical Arrow type remains `Float64`; this metadata is encoded as the
/// `org.pywr.metric` Arrow extension type on the corresponding field.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct MetricColumnMetadata {
    pub metric_set: String,
    pub name: String,
    pub attribute: String,
    pub ty: String,
    pub sub_type: Option<String>,
}

/// Arrow extension type used for Pywr metric columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricColumnExtension(MetricColumnMetadata);

impl MetricColumnExtension {
    pub fn new(metadata: MetricColumnMetadata) -> Self {
        Self(metadata)
    }

    /// Create the extension-typed Arrow field for this metric.
    pub fn field(&self, column_name: impl Into<String>) -> Field {
        let mut metadata = HashMap::new();
        metadata.insert(EXTENSION_TYPE_NAME_KEY.to_string(), Self::NAME.to_string());
        metadata.insert(
            EXTENSION_TYPE_METADATA_KEY.to_string(),
            self.serialize_metadata()
                .expect("metric extension metadata is serializable"),
        );
        Field::new(column_name, DataType::Float64, false).with_metadata(metadata)
    }
}

impl ExtensionType for MetricColumnExtension {
    const NAME: &'static str = "org.pywr.metric";
    type Metadata = MetricColumnMetadata;

    fn metadata(&self) -> &Self::Metadata {
        &self.0
    }

    fn serialize_metadata(&self) -> Option<String> {
        serde_json::to_string(&self.0).ok()
    }

    fn deserialize_metadata(metadata: Option<&str>) -> Result<Self::Metadata, ArrowError> {
        let metadata = metadata
            .ok_or_else(|| ArrowError::InvalidArgumentError("Pywr metric extension metadata is missing".to_string()))?;
        serde_json::from_str(metadata).map_err(|error| {
            ArrowError::InvalidArgumentError(format!("Invalid Pywr metric extension metadata: {error}"))
        })
    }

    fn supports_data_type(&self, data_type: &DataType) -> Result<(), ArrowError> {
        if matches!(data_type, DataType::Float64) {
            Ok(())
        } else {
            Err(ArrowError::InvalidArgumentError(format!(
                "Pywr metric extension requires Float64 storage, got {data_type:?}"
            )))
        }
    }

    fn try_new(data_type: &DataType, metadata: Self::Metadata) -> Result<Self, ArrowError> {
        let extension = Self(metadata);
        extension.supports_data_type(data_type)?;
        Ok(extension)
    }
}

/// A notification sent after a complete record batch has been written and flushed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrowStreamCommit {
    /// Zero-based record batch sequence number.
    pub batch_index: u64,
    /// Number of scenario rows in this record batch.
    pub row_count: usize,
    /// Exclusive byte offset of the flushed data in the IPC stream.
    pub byte_offset: u64,
}

/// Errors produced by the Arrow IPC stream output.
#[derive(Debug, Error)]
pub enum ArrowStreamError {
    #[error("I/O error with Arrow stream at `{path}`.")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Arrow stream output already exists at `{path}`")]
    OutputAlreadyExists { path: PathBuf },
    #[error("Arrow IPC error.")]
    Arrow(#[from] ArrowError),
    #[error("Metric set index `{index}` not found")]
    MetricSetIndexNotFound { index: MetricSetIndex },
    #[error("Metric set produced {actual} values, but its schema has {expected} metrics")]
    MetricCount { expected: usize, actual: usize },
    #[error("Metrics in one metric-set row have inconsistent reporting periods")]
    InconsistentPeriods,
    #[error("Arrow stream writer thread terminated: {0}")]
    WorkerFailed(String),
    #[error("Arrow stream writer thread disconnected")]
    WorkerDisconnected,
    #[error("Arrow stream writer thread panicked")]
    WorkerPanicked,
}

#[derive(Clone, Debug)]
struct ArrowStreamRow {
    time_start: i64,
    time_end: i64,
    simulation_id: u64,
    label: String,
    values: Vec<f64>,
}

#[derive(Debug, Default)]
struct PendingBatch {
    rows: Vec<ArrowStreamRow>,
    timestep_count: usize,
}

#[derive(Debug)]
enum WorkerMessage {
    Batch(PendingBatch),
    Flush(Sender<()>),
    Finish,
}

#[derive(Debug)]
enum WorkerStatus {
    Failed(String),
}

#[derive(Debug)]
struct CountingWriter<W> {
    inner: W,
    position: u64,
}

impl<W> CountingWriter<W> {
    fn new(inner: W) -> Self {
        Self { inner, position: 0 }
    }
}

impl<W: Write> Write for CountingWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(bytes)?;
        self.position += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn make_schema(metric_set_name: &str, metrics: impl Iterator<Item = OutputMetric>) -> Schema {
    let mut fields = vec![
        Field::new("time_start", DataType::Timestamp(TimeUnit::Millisecond, None), false),
        Field::new("time_end", DataType::Timestamp(TimeUnit::Millisecond, None), false),
        Field::new("simulation_id", DataType::UInt64, false),
        Field::new("label", DataType::Utf8, false),
    ];
    let mut used_names = HashMap::<String, usize>::new();

    for metric in metrics {
        let base_name = format!("{}.{}", metric_set_name, metric.fully_qualified_name());
        let count = used_names.entry(base_name.clone()).or_default();
        let column_name = if *count == 0 {
            base_name
        } else {
            format!("{base_name}_{count}")
        };
        *count += 1;
        let metadata = MetricColumnMetadata {
            metric_set: metric_set_name.to_string(),
            name: metric.name().to_string(),
            attribute: metric.attribute().to_string(),
            ty: metric.ty().to_string(),
            sub_type: metric.sub_type().map(ToString::to_string),
        };
        fields.push(MetricColumnExtension::new(metadata).field(column_name));
    }
    Schema::new(fields)
}

fn record_batch(schema: Arc<Schema>, pending: PendingBatch) -> Result<RecordBatch, ArrowStreamError> {
    let metric_count = schema.fields().len() - 4;
    let row_count = pending.rows.len();
    let mut starts = Vec::with_capacity(row_count);
    let mut ends = Vec::with_capacity(row_count);
    let mut simulation_ids = Vec::with_capacity(row_count);
    let mut labels = Vec::with_capacity(row_count);
    let mut values = (0..metric_count)
        .map(|_| Vec::with_capacity(row_count))
        .collect::<Vec<_>>();

    for row in pending.rows {
        if row.values.len() != metric_count {
            return Err(ArrowStreamError::MetricCount {
                expected: metric_count,
                actual: row.values.len(),
            });
        }
        starts.push(row.time_start);
        ends.push(row.time_end);
        simulation_ids.push(row.simulation_id);
        labels.push(row.label);
        for (column, value) in values.iter_mut().zip(row.values) {
            column.push(value);
        }
    }

    let mut columns: Vec<ArrayRef> = vec![
        Arc::new(TimestampMillisecondArray::from(starts)),
        Arc::new(TimestampMillisecondArray::from(ends)),
        Arc::new(UInt64Array::from(simulation_ids)),
        Arc::new(StringArray::from(labels)),
    ];
    columns.extend(
        values
            .into_iter()
            .map(|column| Arc::new(Float64Array::from(column)) as ArrayRef),
    );
    Ok(RecordBatch::try_new(schema, columns)?)
}

fn worker(
    receiver: Receiver<WorkerMessage>,
    status_sender: Sender<WorkerStatus>,
    file: File,
    schema: Arc<Schema>,
    commits: Option<Sender<ArrowStreamCommit>>,
) -> Result<(), ArrowStreamError> {
    let mut writer = StreamWriter::try_new_buffered(CountingWriter::new(file), &schema)?;
    let mut batch_index = 0;
    while let Ok(message) = receiver.recv() {
        match message {
            WorkerMessage::Batch(pending) if pending.rows.is_empty() => {}
            WorkerMessage::Batch(pending) => {
                let row_count = pending.rows.len();
                let batch = record_batch(Arc::clone(&schema), pending)?;
                writer.write(&batch)?;
                writer.flush()?;
                let commit = ArrowStreamCommit {
                    batch_index,
                    row_count,
                    byte_offset: writer.get_ref().get_ref().position,
                };
                batch_index += 1;
                if let Some(commits) = &commits {
                    let _ = commits.send(commit);
                }
            }
            WorkerMessage::Flush(response) => {
                // Channel ordering makes this a barrier for all earlier batches.
                // Each batch is flushed before its commit is sent, so acknowledging
                // the barrier also guarantees commit publication.
                let _ = response.send(());
            }
            WorkerMessage::Finish => {
                writer.finish()?;
                return Ok(());
            }
        }
    }
    let error = ArrowStreamError::WorkerDisconnected;
    let _ = status_sender.send(WorkerStatus::Failed(error.to_string()));
    Err(error)
}

#[derive(Debug)]
struct Internal {
    pending: PendingBatch,
    sender: Sender<WorkerMessage>,
    status_receiver: Receiver<WorkerStatus>,
    worker: Option<JoinHandle<Result<(), ArrowStreamError>>>,
}

/// Output one metric set as a batched Arrow IPC stream on a worker thread.
#[derive(Debug)]
pub struct ArrowStreamOutput {
    meta: RecorderMeta,
    filename: PathBuf,
    metric_set_idx: MetricSetIndex,
    batch_size: NonZeroUsize,
    commits: Option<Sender<ArrowStreamCommit>>,
}

impl ArrowStreamOutput {
    fn check_worker(internal: &Internal) -> Result<(), ArrowStreamError> {
        match internal.status_receiver.try_recv() {
            Ok(WorkerStatus::Failed(message)) => Err(ArrowStreamError::WorkerFailed(message)),
            Err(TryRecvError::Disconnected) if internal.worker.as_ref().is_some_and(JoinHandle::is_finished) => {
                Err(ArrowStreamError::WorkerDisconnected)
            }
            Err(_) => Ok(()),
        }
    }

    fn queue_pending(&self, internal: &mut Internal, force: bool) -> Result<(), ArrowStreamError> {
        if internal.pending.rows.is_empty() || (!force && internal.pending.timestep_count < self.batch_size.get()) {
            return Ok(());
        }
        let pending = std::mem::take(&mut internal.pending);
        internal
            .sender
            .send(WorkerMessage::Batch(pending))
            .map_err(|_| ArrowStreamError::WorkerDisconnected)
    }

    fn append_values(
        &self,
        scenario_indices: &[ScenarioIndex],
        metric_set_states: &[Vec<MetricSetState>],
        internal: &mut Internal,
    ) -> Result<(), ArrowStreamError> {
        for (scenario_index, scenario_states) in scenario_indices.iter().zip(metric_set_states) {
            let metric_set_state =
                scenario_states
                    .get(*self.metric_set_idx.deref())
                    .ok_or(ArrowStreamError::MetricSetIndexNotFound {
                        index: self.metric_set_idx,
                    })?;
            let Some(values) = metric_set_state.current_values() else {
                continue;
            };
            let first = values
                .first()
                .ok_or(ArrowStreamError::MetricCount { expected: 1, actual: 0 })?;
            if values
                .iter()
                .any(|value| value.start != first.start || value.end() != first.end())
            {
                return Err(ArrowStreamError::InconsistentPeriods);
            }
            internal.pending.rows.push(ArrowStreamRow {
                time_start: jiff_datetime_to_arrow_timestamp_ms(&first.start)
                    .map_err(|error| ArrowStreamError::WorkerFailed(error.to_string()))?,
                time_end: jiff_datetime_to_arrow_timestamp_ms(&first.end())
                    .map_err(|error| ArrowStreamError::WorkerFailed(error.to_string()))?,
                simulation_id: scenario_index.simulation_id() as u64,
                label: scenario_index.label(),
                values: values.iter().map(|value| value.value).collect(),
            });
        }
        Ok(())
    }
}

impl Recorder for ArrowStreamOutput {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn setup(
        &self,
        _domain: &ModelDomain,
        network: &Network,
    ) -> Result<Option<Box<dyn RecorderInternalState>>, RecorderSetupError> {
        let metric_set =
            network
                .get_metric_set(self.metric_set_idx)
                .ok_or(ArrowStreamError::MetricSetIndexNotFound {
                    index: self.metric_set_idx,
                })?;
        let schema = Arc::new(make_schema(metric_set.name(), metric_set.iter_metrics().cloned()));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.filename)
            .map_err(|source| {
                if source.kind() == io::ErrorKind::AlreadyExists {
                    ArrowStreamError::OutputAlreadyExists {
                        path: self.filename.clone(),
                    }
                } else {
                    ArrowStreamError::Io {
                        path: self.filename.clone(),
                        source,
                    }
                }
            })?;
        let (sender, receiver) = mpsc::channel();
        let (status_sender, status_receiver) = mpsc::channel();
        let commits = self.commits.clone();
        let worker = thread::spawn(move || {
            let result = worker(receiver, status_sender.clone(), file, schema, commits);
            if let Err(error) = &result {
                let _ = status_sender.send(WorkerStatus::Failed(error.to_string()));
            }
            result
        });
        Ok(Some(Box::new(Internal {
            pending: PendingBatch::default(),
            sender,
            status_receiver,
            worker: Some(worker),
        })))
    }

    fn save(
        &self,
        _timestep: &Timestep,
        scenario_indices: &[ScenarioIndex],
        _network: &Network,
        _state: &[State],
        metric_set_states: &[Vec<MetricSetState>],
        internal_state: &mut Option<Box<dyn RecorderInternalState>>,
    ) -> Result<(), RecorderSaveError> {
        let internal = downcast_internal_state_mut::<Internal>(internal_state);
        Self::check_worker(internal)?;
        self.append_values(scenario_indices, metric_set_states, internal)?;
        internal.pending.timestep_count += 1;
        self.queue_pending(internal, false)?;
        Ok(())
    }

    fn flush(&self, internal_state: &mut Option<Box<dyn RecorderInternalState>>) -> Result<(), RecorderSaveError> {
        let internal = downcast_internal_state_mut::<Internal>(internal_state);
        Self::check_worker(internal)?;
        self.queue_pending(internal, true)?;
        let (response_sender, response_receiver) = mpsc::channel();
        internal
            .sender
            .send(WorkerMessage::Flush(response_sender))
            .map_err(|_| ArrowStreamError::WorkerDisconnected)?;
        response_receiver
            .recv()
            .map_err(|_| ArrowStreamError::WorkerDisconnected)?;
        Self::check_worker(internal)?;
        Ok(())
    }

    fn finalise(
        &self,
        _network: &Network,
        scenario_indices: &[ScenarioIndex],
        metric_set_states: &[Vec<MetricSetState>],
        internal_state: Option<Box<dyn RecorderInternalState>>,
    ) -> Result<Option<Box<dyn RecorderFinalResult>>, RecorderFinaliseError> {
        let mut internal = downcast_internal_state::<Internal>(internal_state);
        Self::check_worker(&internal)?;
        self.append_values(scenario_indices, metric_set_states, &mut internal)?;
        self.queue_pending(&mut internal, true)?;
        internal
            .sender
            .send(WorkerMessage::Finish)
            .map_err(|_| ArrowStreamError::WorkerDisconnected)?;
        let worker = internal.worker.take().expect("Arrow stream worker must be present");
        worker.join().map_err(|_| ArrowStreamError::WorkerPanicked)??;
        Ok(None)
    }
}

/// Builder for [`ArrowStreamOutput`].
#[derive(Debug)]
pub struct ArrowStreamOutputBuilder {
    meta: RecorderMeta,
    filename: PathBuf,
    metric_set: String,
    batch_size: NonZeroUsize,
    commits: Option<Sender<ArrowStreamCommit>>,
}

impl ArrowStreamOutputBuilder {
    pub fn new<P: Into<PathBuf>>(name: &str, filename: P, metric_set: &str, batch_size: NonZeroUsize) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            filename: filename.into(),
            metric_set: metric_set.to_string(),
            batch_size,
            commits: None,
        }
    }

    /// Send a best-effort commit notification after every flushed record batch.
    ///
    /// Dropping the receiver never stops model execution or Arrow output.
    pub fn commit_sender(&mut self, sender: Sender<ArrowStreamCommit>) -> &mut Self {
        self.commits = Some(sender);
        self
    }
}

impl RecorderBuilder for ArrowStreamOutputBuilder {
    fn name(&self) -> &str {
        &self.meta.name
    }

    fn build(self: Box<Self>, resolution_maps: &ResolutionMaps) -> Result<Box<dyn Recorder>, RecorderBuilderError> {
        let metric_set_idx = resolution_maps
            .metric_sets
            .get(&self.metric_set)
            .copied()
            .ok_or_else(|| RecorderBuilderError::MetricSetNotFound {
                name: self.metric_set.clone(),
            })?;
        Ok(Box::new(ArrowStreamOutput {
            meta: self.meta,
            filename: self.filename,
            metric_set_idx,
            batch_size: self.batch_size,
            commits: self.commits,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::ipc::reader::StreamReader;
    use std::fs;

    #[test]
    fn metric_extension_round_trips_through_field_metadata() {
        let metadata = MetricColumnMetadata {
            metric_set: "outputs".to_string(),
            name: "reservoir".to_string(),
            attribute: "volume".to_string(),
            ty: "node".to_string(),
            sub_type: Some("storage".to_string()),
        };
        let field = MetricColumnExtension::new(metadata.clone()).field("reservoir");
        assert_eq!(
            MetricColumnExtension::try_new_from_field_metadata(field.data_type(), field.metadata())
                .unwrap()
                .metadata(),
            &metadata
        );
    }

    #[test]
    fn worker_writes_batched_rows_and_commits_after_flush() {
        let path = std::env::temp_dir().join(format!("pywr-arrow-stream-{}.arrow", std::process::id()));
        let schema = Arc::new(Schema::new(vec![
            Field::new("time_start", DataType::Timestamp(TimeUnit::Millisecond, None), false),
            Field::new("time_end", DataType::Timestamp(TimeUnit::Millisecond, None), false),
            Field::new("simulation_id", DataType::UInt64, false),
            Field::new("label", DataType::Utf8, false),
            MetricColumnExtension::new(MetricColumnMetadata {
                metric_set: "outputs".to_string(),
                name: "flow".to_string(),
                attribute: "outflow".to_string(),
                ty: "node".to_string(),
                sub_type: None,
            })
            .field("flow"),
        ]));
        let (sender, receiver) = mpsc::channel();
        let (status_sender, _status_receiver) = mpsc::channel();
        let (commit_sender, commit_receiver) = mpsc::channel();
        let file = File::create(&path).unwrap();
        let worker_schema = Arc::clone(&schema);
        let handle = thread::spawn(move || worker(receiver, status_sender, file, worker_schema, Some(commit_sender)));
        sender
            .send(WorkerMessage::Batch(PendingBatch {
                timestep_count: 2,
                rows: vec![
                    ArrowStreamRow {
                        time_start: 0,
                        time_end: 1,
                        simulation_id: 0,
                        label: "a".to_string(),
                        values: vec![1.0],
                    },
                    ArrowStreamRow {
                        time_start: 1,
                        time_end: 2,
                        simulation_id: 0,
                        label: "a".to_string(),
                        values: vec![2.0],
                    },
                ],
            }))
            .unwrap();
        let (flush_sender, flush_receiver) = mpsc::channel();
        sender.send(WorkerMessage::Flush(flush_sender)).unwrap();
        flush_receiver.recv().unwrap();
        let commit = commit_receiver.recv().unwrap();
        assert_eq!(commit.batch_index, 0);
        assert_eq!(commit.row_count, 2);
        assert!(commit.byte_offset > 0);
        assert_eq!(fs::metadata(&path).unwrap().len(), commit.byte_offset);
        sender.send(WorkerMessage::Finish).unwrap();
        handle.join().unwrap().unwrap();

        let mut reader = StreamReader::try_new_buffered(File::open(&path).unwrap(), None).unwrap();
        let batch = reader.next().unwrap().unwrap();
        assert_eq!(batch.num_rows(), 2);
        assert!(reader.next().is_none());
        fs::remove_file(path).unwrap();
    }
}
