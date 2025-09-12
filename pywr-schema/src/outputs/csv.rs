#[cfg(feature = "core")]
use crate::error::SchemaError;
#[cfg(feature = "core")]
use pywr_core::recorders::{CsvLongFmtOutput, CsvWideFmtOutput, Recorder};
use pywr_schema_macros::{PywrVisitPaths, skip_serializing_none};
use schemars::JsonSchema;
#[cfg(feature = "core")]
use std::num::NonZeroU32;
#[cfg(feature = "core")]
use std::path::Path;
use std::path::PathBuf;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};

#[derive(
    serde::Deserialize, serde::Serialize, Debug, Clone, Default, JsonSchema, PywrVisitPaths, Display, EnumIter,
)]
pub enum CsvFormat {
    Wide,
    #[default]
    Long,
}

#[derive(
    serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitPaths, Display, EnumDiscriminants,
)]
#[serde(untagged)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(CsvMetricSetType))]
pub enum CsvMetricSet {
    Single(String),
    Multiple(Vec<String>),
}

/// Output data to a CSV file.
///
/// This output will write the output data to a CSV file. The output data is written in either
/// wide or long format. The wide format will write each metric to a separate column, while the
/// long format will write each metric to a separate row. The wide format is useful for small
/// numbers of metrics or scenarios, while the long format is useful for large numbers of metrics
/// or scenarios. For more details see the [`CsvLongFmtOutput`] and [`CsvWideFmtOutput`] types.
///
/// The long format supports either a single metric set or a list of metric sets. However,
/// the wide format only supports a single metric set.
///
#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitPaths)]
pub struct CsvOutput {
    pub name: String,
    pub filename: PathBuf,
    pub format: CsvFormat,
    pub metric_set: CsvMetricSet,
    pub decimal_places: Option<u32>,
}

#[cfg(feature = "core")]
impl CsvOutput {
    pub fn add_to_model(
        &self,
        network: &mut pywr_core::network::Network,
        output_path: Option<&Path>,
    ) -> Result<(), SchemaError> {
        let filename = match (output_path, self.filename.is_relative()) {
            (Some(odir), true) => odir.join(&self.filename),
            _ => self.filename.to_path_buf(),
        };

        let recorder: Box<dyn Recorder> = match self.format {
            CsvFormat::Wide => match &self.metric_set {
                CsvMetricSet::Single(metric_set) => {
                    let metric_set_idx = network.get_metric_set_index_by_name(metric_set)?;
                    Box::new(CsvWideFmtOutput::new(&self.name, filename, metric_set_idx))
                }
                CsvMetricSet::Multiple(_) => {
                    return Err(SchemaError::MissingMetricSet(
                        "Wide format CSV output requires a single `metric_set`".to_string(),
                    ));
                }
            },
            CsvFormat::Long => {
                let metric_set_indices = match &self.metric_set {
                    CsvMetricSet::Single(metric_set) => vec![network.get_metric_set_index_by_name(metric_set)?],
                    CsvMetricSet::Multiple(metric_sets) => metric_sets
                        .iter()
                        .map(|ms| network.get_metric_set_index_by_name(ms))
                        .collect::<Result<Vec<_>, _>>()?,
                };

                Box::new(CsvLongFmtOutput::new(
                    &self.name,
                    filename,
                    &metric_set_indices,
                    self.decimal_places.and_then(NonZeroU32::new),
                ))
            }
        };

        network.add_recorder(recorder)?;

        Ok(())
    }
}
