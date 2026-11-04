use super::arrow_batch::{MetricSetBatchError, PendingBatch, make_schema, record_batch};
use super::{
    MetricSetState, Recorder, RecorderBuilder, RecorderBuilderError, RecorderFinalResult, RecorderFinaliseError,
    RecorderInternalState, RecorderMeta, RecorderSaveError, RecorderSetupError, Timestep, downcast_internal_state,
    downcast_internal_state_mut,
};
use crate::models::ModelDomain;
use crate::network::{MetricSetIndex, Network, ResolutionMaps};
use crate::scenario::ScenarioIndex;
use crate::state::State;
use arrow::datatypes::Schema;
use parquet::arrow::ArrowWriter;
use parquet::basic::{Compression, ZstdLevel};
use parquet::errors::ParquetError;
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;
use std::fs::{File, OpenOptions};
use std::io;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use thiserror::Error;

/// Errors produced by the Parquet output.
#[derive(Debug, Error)]
pub enum ParquetOutputError {
    #[error("I/O error with Parquet output at `{path}`.")]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("Parquet output already exists at `{path}`")]
    OutputAlreadyExists { path: PathBuf },
    #[error("Parquet error.")]
    Parquet(#[from] ParquetError),
    #[error("Error building an Arrow record batch.")]
    Batch(#[from] MetricSetBatchError),
    #[error("Parquet writer thread disconnected")]
    WorkerDisconnected,
    #[error("Parquet writer thread panicked")]
    WorkerPanicked,
}

#[derive(Debug)]
enum WorkerMessage {
    Batch(PendingBatch),
    Finish,
}

/// Writes each batch it receives as one row group, and the footer once told to finish.
///
/// A sender dropped without finishing leaves the file without its footer, so an
/// interrupted run never looks complete.
fn worker(
    receiver: Receiver<WorkerMessage>,
    mut writer: ArrowWriter<File>,
    schema: Arc<Schema>,
    scenario_count: usize,
) -> Result<(), ParquetOutputError> {
    while let Ok(message) = receiver.recv() {
        match message {
            WorkerMessage::Batch(pending) => {
                writer.write(&record_batch(Arc::clone(&schema), scenario_count, pending)?)?;
                writer.flush()?;
            }
            WorkerMessage::Finish => {
                writer.close()?;
                return Ok(());
            }
        }
    }
    Err(ParquetOutputError::WorkerDisconnected)
}

struct Internal {
    pending: PendingBatch,
    sender: SyncSender<WorkerMessage>,
    /// Taken once to collect the error of a writer thread that has stopped.
    worker: Option<JoinHandle<Result<(), ParquetOutputError>>>,
    metric_count: usize,
    scenario_count: usize,
}

impl Internal {
    /// Sends to the writer thread, returning its error if it has stopped.
    fn send(&mut self, message: WorkerMessage) -> Result<(), ParquetOutputError> {
        if self.sender.send(message).is_ok() {
            return Ok(());
        }
        Err(self.join().err().unwrap_or(ParquetOutputError::WorkerDisconnected))
    }

    fn join(&mut self) -> Result<(), ParquetOutputError> {
        match self.worker.take().map(JoinHandle::join) {
            Some(Ok(result)) => result,
            Some(Err(_)) => Err(ParquetOutputError::WorkerPanicked),
            None => Err(ParquetOutputError::WorkerDisconnected),
        }
    }
}

/// Output one metric set to a Parquet file, through a writer thread.
///
/// The file holds the same Arrow table as an [`ArrowStreamOutput`](super::ArrowStreamOutput),
/// with the Arrow schema stored in the file so Arrow readers restore it, including the
/// `org.pywr.metric` columns and scenario metadata. Each batch of timesteps is one row group.
/// The schema's metadata is also written as plain Parquet key-value metadata, for readers that
/// don't decode the Arrow schema.
///
/// The file is only complete, with its footer, once the recorder is finalised.
#[derive(Debug)]
pub struct ParquetOutput {
    meta: RecorderMeta,
    filename: PathBuf,
    metric_set_idx: MetricSetIndex,
    batch_size: NonZeroUsize,
    compression: Compression,
}

impl ParquetOutput {
    /// Creates the file and its writer.
    fn create_writer(&self, schema: &Arc<Schema>) -> Result<ArrowWriter<File>, ParquetOutputError> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.filename)
            .map_err(|source| {
                if source.kind() == io::ErrorKind::AlreadyExists {
                    ParquetOutputError::OutputAlreadyExists {
                        path: self.filename.clone(),
                    }
                } else {
                    ParquetOutputError::Io {
                        path: self.filename.clone(),
                        source,
                    }
                }
            })?;
        // Sorted so the file's bytes don't depend on hash map order.
        let mut key_value_metadata: Vec<_> = schema
            .metadata()
            .iter()
            .map(|(key, value)| KeyValue::new(key.clone(), value.clone()))
            .collect();
        key_value_metadata.sort_by(|a, b| a.key.cmp(&b.key));
        let properties = WriterProperties::builder()
            .set_compression(self.compression)
            .set_key_value_metadata(Some(key_value_metadata))
            .build();
        Ok(ArrowWriter::try_new(file, Arc::clone(schema), Some(properties))?)
    }

    fn queue_pending(&self, internal: &mut Internal, force: bool) -> Result<(), ParquetOutputError> {
        if internal.pending.rows.is_empty() || (!force && internal.pending.timestep_count < self.batch_size.get()) {
            return Ok(());
        }
        let pending = std::mem::take(&mut internal.pending);
        internal.send(WorkerMessage::Batch(pending))
    }

    fn append(
        &self,
        metric_set_states: &[Vec<MetricSetState>],
        internal: &mut Internal,
    ) -> Result<(), ParquetOutputError> {
        Ok(internal.pending.append(
            self.metric_set_idx,
            metric_set_states,
            internal.metric_count,
            internal.scenario_count,
        )?)
    }
}

impl Recorder for ParquetOutput {
    fn meta(&self) -> &RecorderMeta {
        &self.meta
    }

    fn setup(
        &self,
        domain: &ModelDomain,
        network: &Network,
    ) -> Result<Option<Box<dyn RecorderInternalState>>, RecorderSetupError> {
        let metric_set =
            network
                .get_metric_set(self.metric_set_idx)
                .ok_or(RecorderSetupError::MetricSetIndexNotFound {
                    index: self.metric_set_idx,
                })?;

        let schema = Arc::new(make_schema(metric_set.name(), domain.scenarios(), metric_set.metrics()));
        let scenario_count = domain.scenarios().len();
        let writer = self.create_writer(&schema)?;
        // Bounded, so a model that outpaces the encoder waits for it rather than queueing
        // the whole run in memory.
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || worker(receiver, writer, schema, scenario_count));
        Ok(Some(Box::new(Internal {
            pending: PendingBatch::default(),
            sender,
            worker: Some(worker),
            metric_count: metric_set.metrics().len(),
            scenario_count,
        })))
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
        self.append(metric_set_states, internal)?;
        internal.pending.timestep_count += 1;
        self.queue_pending(internal, false)?;
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
        self.append(metric_set_states, &mut internal)?;
        self.queue_pending(&mut internal, true)?;
        internal.send(WorkerMessage::Finish)?;
        internal.join()?;
        Ok(None)
    }
}

/// Builder for [`ParquetOutput`].
#[derive(Debug)]
pub struct ParquetOutputBuilder {
    meta: RecorderMeta,
    filename: PathBuf,
    metric_set: String,
    batch_size: NonZeroUsize,
    compression: Compression,
}

impl ParquetOutputBuilder {
    /// A builder writing `metric_set` to `filename`, `batch_size` timesteps to each row group,
    /// compressed with Zstandard.
    pub fn new(name: &str, filename: &Path, metric_set: &str, batch_size: NonZeroUsize) -> Self {
        Self {
            meta: RecorderMeta::new(name),
            filename: filename.to_path_buf(),
            metric_set: metric_set.to_string(),
            batch_size,
            compression: Compression::ZSTD(ZstdLevel::default()),
        }
    }

    /// Compress the file's pages with `compression` instead.
    pub fn compression(&mut self, compression: Compression) -> &mut Self {
        self.compression = compression;
        self
    }
}

impl RecorderBuilder for ParquetOutputBuilder {
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
        Ok(Box::new(ParquetOutput {
            meta: self.meta,
            filename: self.filename,
            metric_set_idx,
            batch_size: self.batch_size,
            compression: self.compression,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recorders::arrow_batch::tests::{two_metric_schema, two_row_batch};
    use arrow::array::{FixedSizeListArray, Float64Array};
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use parquet::file::reader::{FileReader, SerializedFileReader};
    use pywr_output_metadata::SCENARIOS_KEY;
    use std::fs;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("pywr-parquet-{name}-{}.parquet", std::process::id()))
    }

    /// A writer thread for [`two_metric_schema`] writing to `path`, as [`ParquetOutput::setup`] starts one.
    fn spawn_worker(path: &Path) -> Internal {
        let mut schema = two_metric_schema().as_ref().clone();
        schema.metadata.insert(SCENARIOS_KEY.to_string(), "[]".to_string());
        let schema = Arc::new(schema);
        let output = ParquetOutput {
            meta: RecorderMeta::new("results"),
            filename: path.to_path_buf(),
            metric_set_idx: MetricSetIndex::new(0),
            batch_size: NonZeroUsize::new(2).unwrap(),
            compression: Compression::ZSTD(ZstdLevel::default()),
        };
        let writer = output.create_writer(&schema).unwrap();
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || worker(receiver, writer, schema, 2));
        Internal {
            pending: PendingBatch::default(),
            sender,
            worker: Some(worker),
            metric_count: 2,
            scenario_count: 2,
        }
    }

    #[test]
    fn worker_writes_a_row_group_per_batch_and_keeps_the_arrow_schema() {
        let path = temp_path("batches");
        let mut internal = spawn_worker(&path);
        internal.send(WorkerMessage::Batch(two_row_batch())).unwrap();
        internal.send(WorkerMessage::Batch(two_row_batch())).unwrap();
        internal.send(WorkerMessage::Finish).unwrap();
        internal.join().unwrap();

        let file_reader = SerializedFileReader::new(File::open(&path).unwrap()).unwrap();
        assert_eq!(file_reader.metadata().num_row_groups(), 2);
        // The scenario metadata is readable without decoding the Arrow schema.
        let key_values = file_reader.metadata().file_metadata().key_value_metadata().unwrap();
        assert!(
            key_values
                .iter()
                .any(|kv| kv.key == SCENARIOS_KEY && kv.value.as_deref() == Some("[]"))
        );

        let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(&path).unwrap()).unwrap();
        let schema = builder.schema().clone();
        assert_eq!(schema.fields(), two_metric_schema().fields());
        assert_eq!(schema.metadata().get(SCENARIOS_KEY).map(String::as_str), Some("[]"));
        let batches: Vec<_> = builder.build().unwrap().collect::<Result<_, _>>().unwrap();
        assert_eq!(batches.iter().map(|batch| batch.num_rows()).sum::<usize>(), 4);
        let values = batches[0]
            .column(2)
            .as_any()
            .downcast_ref::<FixedSizeListArray>()
            .unwrap();
        // The reader joins both row groups into one batch.
        assert_eq!(
            values.values().as_any().downcast_ref::<Float64Array>().unwrap(),
            &Float64Array::from([Some(1.0), None, Some(2.0), Some(3.0)].repeat(2))
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_run_that_never_finishes_leaves_no_footer() {
        let path = temp_path("unfinished");
        let mut internal = spawn_worker(&path);
        internal.send(WorkerMessage::Batch(two_row_batch())).unwrap();
        drop(internal.sender);
        let result = internal.worker.take().unwrap().join().unwrap();
        assert!(matches!(result, Err(ParquetOutputError::WorkerDisconnected)));
        assert!(SerializedFileReader::new(File::open(&path).unwrap()).is_err());
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn an_existing_file_is_not_overwritten() {
        let path = temp_path("existing");
        fs::write(&path, b"keep").unwrap();
        let output = ParquetOutput {
            meta: RecorderMeta::new("results"),
            filename: path.clone(),
            metric_set_idx: MetricSetIndex::new(0),
            batch_size: NonZeroUsize::new(2).unwrap(),
            compression: Compression::UNCOMPRESSED,
        };
        let result = output.create_writer(&two_metric_schema());
        assert!(matches!(result, Err(ParquetOutputError::OutputAlreadyExists { .. })));
        assert_eq!(fs::read(&path).unwrap(), b"keep");
        fs::remove_file(path).unwrap();
    }
}
