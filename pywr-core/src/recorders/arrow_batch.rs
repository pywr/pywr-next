//! The Arrow table written by the [`ArrowStreamOutput`](super::ArrowStreamOutput) and
//! [`ParquetOutput`](super::ParquetOutput) outputs: one row per timestep, and one
//! `FixedSizeList<Float64>` column per metric holding its value in each scenario.
use super::{MetricSetState, OutputMetric, jiff_datetime_to_arrow_timestamp_ms};
use crate::network::MetricSetIndex;
use crate::scenario::ScenarioDomain;
use arrow::array::{ArrayRef, FixedSizeListArray, Float64Array, TimestampMillisecondArray};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::error::ArrowError;
use arrow::record_batch::RecordBatch;
use arrow_schema::Metadata;
use jiff::civil::DateTime;
use pywr_output_metadata::{
    MetricColumnExtension, MetricColumnMetadata, SCENARIO_GROUPS_KEY, SCENARIOS_KEY, ScenarioGroupMetadata,
    ScenarioMetadata, VERSION_KEY,
};
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use thiserror::Error;

/// Errors building an Arrow record batch from a metric set's values.
#[derive(Debug, Error)]
pub enum MetricSetBatchError {
    #[error("Arrow error.")]
    Arrow(#[from] ArrowError),
    #[error("Metric set index `{index}` not found")]
    MetricSetIndexNotFound { index: MetricSetIndex },
    #[error("Metric set produced {actual} values, but its schema has {expected} metrics")]
    MetricCount { expected: usize, actual: usize },
    #[error("Expected {expected} scenarios, but received {actual}")]
    ScenarioCount { expected: usize, actual: usize },
    #[error("Metrics in one metric-set row have inconsistent reporting periods")]
    InconsistentPeriods,
    #[error("Could not convert a reporting period to an Arrow timestamp.")]
    Timestamp(#[from] jiff::Error),
}

#[derive(Clone, Debug)]
pub(super) struct MetricSetRow {
    pub(super) time_start: i64,
    pub(super) time_end: i64,
    pub(super) values: Vec<Option<f64>>,
}

/// Rows waiting to be written as one record batch.
#[derive(Debug, Default)]
pub(super) struct PendingBatch {
    pub(super) rows: Vec<MetricSetRow>,
    pub(super) timestep_count: usize,
}

impl PendingBatch {
    /// Appends a row of the metric set's current values in every scenario, if it has any.
    pub(super) fn append(
        &mut self,
        metric_set_idx: MetricSetIndex,
        metric_set_states: &[Vec<MetricSetState>],
        metric_count: usize,
        scenario_count: usize,
    ) -> Result<(), MetricSetBatchError> {
        let mut time_start_end: Option<(DateTime, DateTime)> = None;
        if metric_set_states.len() != scenario_count {
            return Err(MetricSetBatchError::ScenarioCount {
                expected: scenario_count,
                actual: metric_set_states.len(),
            });
        }
        let mut all_values: Vec<Option<f64>> = Vec::with_capacity(metric_count * scenario_count);

        for scenario_states in metric_set_states.iter() {
            let values = scenario_states
                .get(*metric_set_idx.deref())
                .ok_or(MetricSetBatchError::MetricSetIndexNotFound { index: metric_set_idx })?
                .current_values();
            match values {
                None => {
                    // If the metric set is missing for this scenario, fill in `None` values for all metrics.
                    all_values.extend(std::iter::repeat_n(None, metric_count));
                }
                Some(values) => {
                    if values.len() != metric_count {
                        return Err(MetricSetBatchError::MetricCount {
                            expected: metric_count,
                            actual: values.len(),
                        });
                    }
                    if let Some((start, end)) = time_start_end {
                        if values.iter().any(|value| value.start != start || value.end() != end) {
                            return Err(MetricSetBatchError::InconsistentPeriods);
                        }
                    } else {
                        let first = values
                            .first()
                            .ok_or(MetricSetBatchError::MetricCount { expected: 1, actual: 0 })?;

                        if values
                            .iter()
                            .any(|value| value.start != first.start || value.end() != first.end())
                        {
                            return Err(MetricSetBatchError::InconsistentPeriods);
                        }

                        time_start_end = Some((first.start, first.end()));
                    }

                    all_values.extend(values.iter().map(|value| Some(value.value)));
                }
            };
        }

        if let Some((start, end)) = time_start_end {
            self.rows.push(MetricSetRow {
                time_start: jiff_datetime_to_arrow_timestamp_ms(&start)?,
                time_end: jiff_datetime_to_arrow_timestamp_ms(&end)?,
                values: all_values,
            });
        }
        Ok(())
    }
}

pub(super) fn make_schema(metric_set_name: &str, scenario_domain: &ScenarioDomain, metrics: &[OutputMetric]) -> Schema {
    let mut fields = vec![
        Field::new("time_start", DataType::Timestamp(TimeUnit::Millisecond, None), false),
        Field::new("time_end", DataType::Timestamp(TimeUnit::Millisecond, None), false),
    ];
    let mut used_names = HashMap::<String, usize>::new();

    let scenario_count = i32::try_from(scenario_domain.len()).expect("scenario count must fit in Arrow list size");
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
        fields.push(MetricColumnExtension::new(metadata).field(column_name, scenario_count));
    }

    let mut metadata = Metadata::new();
    metadata.insert(VERSION_KEY, env!("CARGO_PKG_VERSION"));
    let scenarios: Vec<_> = scenario_domain
        .indices()
        .iter()
        .map(|s| ScenarioMetadata {
            simulation_id: s.simulation_id(),
            simulation_indices: s.simulation_indices().to_vec(),
            scenario_labels: s.labels().to_vec(),
        })
        .collect();
    metadata.insert(
        SCENARIOS_KEY,
        serde_json::to_string(&scenarios).expect("scenario metadata is serializable"),
    );

    let groups: Vec<_> = scenario_domain
        .groups()
        .iter()
        .map(|group| ScenarioGroupMetadata {
            name: group.name().to_string(),
            size: group.size(),
        })
        .collect();
    metadata.insert(
        SCENARIO_GROUPS_KEY,
        serde_json::to_string(&groups).expect("scenario group metadata is serializable"),
    );

    Schema::new_with_metadata(fields, metadata)
}

pub(super) fn record_batch(
    schema: Arc<Schema>,
    scenario_count: usize,
    pending: PendingBatch,
) -> Result<RecordBatch, MetricSetBatchError> {
    let metric_count = schema.fields().len() - 2; // Exclude time_start and time_end
    let row_count = pending.rows.len();
    let mut starts = Vec::with_capacity(row_count);
    let mut ends = Vec::with_capacity(row_count);
    let mut values = (0..metric_count)
        .map(|_| Vec::with_capacity(row_count * scenario_count))
        .collect::<Vec<_>>();

    for row in pending.rows {
        if row.values.len() != metric_count * scenario_count {
            return Err(MetricSetBatchError::MetricCount {
                expected: metric_count * scenario_count,
                actual: row.values.len(),
            });
        }
        starts.push(row.time_start);
        ends.push(row.time_end);
        for scenario_values in row.values.chunks_exact(metric_count) {
            for (column, value) in values.iter_mut().zip(scenario_values) {
                column.push(*value);
            }
        }
    }

    let mut columns: Vec<ArrayRef> = vec![
        Arc::new(TimestampMillisecondArray::from(starts)),
        Arc::new(TimestampMillisecondArray::from(ends)),
    ];
    for (field, column) in schema.fields().iter().skip(2).zip(values) {
        let DataType::FixedSizeList(child, size) = field.data_type() else {
            return Err(MetricSetBatchError::Arrow(ArrowError::SchemaError(
                "Expected fixed-size list metric".into(),
            )));
        };
        columns.push(Arc::new(FixedSizeListArray::try_new(
            Arc::clone(child),
            *size,
            Arc::new(Float64Array::from(column)),
            None,
        )?));
    }
    Ok(RecordBatch::try_new(schema, columns)?)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// Two metrics over two scenarios.
    pub(in crate::recorders) fn two_metric_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("time_start", DataType::Timestamp(TimeUnit::Millisecond, None), false),
            Field::new("time_end", DataType::Timestamp(TimeUnit::Millisecond, None), false),
            MetricColumnExtension::new(MetricColumnMetadata {
                metric_set: "outputs".to_string(),
                name: "flow".to_string(),
                attribute: "outflow".to_string(),
                ty: "node".to_string(),
                sub_type: None,
            })
            .field("flow", 2),
            MetricColumnExtension::new(MetricColumnMetadata {
                metric_set: "outputs".to_string(),
                name: "volume".to_string(),
                attribute: "volume".to_string(),
                ty: "node".to_string(),
                sub_type: None,
            })
            .field("volume", 2),
        ]))
    }

    /// Two timesteps of [`two_metric_schema`]'s metrics, the first missing its second scenario.
    pub(in crate::recorders) fn two_row_batch() -> PendingBatch {
        PendingBatch {
            timestep_count: 2,
            rows: vec![
                MetricSetRow {
                    time_start: 0,
                    time_end: 1,
                    values: vec![Some(1.0), Some(10.0), None, None],
                },
                MetricSetRow {
                    time_start: 1,
                    time_end: 2,
                    values: vec![Some(2.0), Some(20.0), Some(3.0), Some(30.0)],
                },
            ],
        }
    }
}
