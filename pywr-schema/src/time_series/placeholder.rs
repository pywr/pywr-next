use crate::VisitPaths;
use crate::meta::NamedMeta;
use pywr_schema_macros::skip_serializing_none;
use schemars::JsonSchema;

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaceholderTimeSeries {
    pub meta: NamedMeta,
}

impl VisitPaths for PlaceholderTimeSeries {}

#[cfg(feature = "core")]
mod core {
    use super::PlaceholderTimeSeries;
    use crate::time_series::{LoadedTimeSeries, TimeSeriesError};

    impl PlaceholderTimeSeries {
        pub fn load(&self) -> Result<LoadedTimeSeries, TimeSeriesError> {
            Err(TimeSeriesError::PlaceholderTimeSeriesNotAllowed {
                name: self.meta.name.clone(),
            })
        }
    }
}
