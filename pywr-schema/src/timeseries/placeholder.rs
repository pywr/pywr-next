use crate::VisitPaths;
use crate::parameters::ParameterMeta;
use pywr_schema_macros::skip_serializing_none;
use schemars::JsonSchema;

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaceholderTimeseries {
    pub meta: ParameterMeta,
}

impl VisitPaths for PlaceholderTimeseries {}

#[cfg(feature = "core")]
mod core {
    use super::PlaceholderTimeseries;
    use crate::timeseries::TimeseriesError;
    use polars::frame::DataFrame;

    impl PlaceholderTimeseries {
        pub fn load(&self) -> Result<DataFrame, TimeseriesError> {
            Err(TimeseriesError::PlaceholderTimeseriesNotAllowed {
                name: self.meta.name.clone(),
            })
        }
    }
}
