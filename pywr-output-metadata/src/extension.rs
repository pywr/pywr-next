//! The [`MetricColumnExtension`] Arrow extension type, enabled by the `arrow` feature.
use crate::{METRIC_EXTENSION_NAME, MetricColumnMetadata};
use arrow_schema::extension::{EXTENSION_TYPE_METADATA_KEY, EXTENSION_TYPE_NAME_KEY, ExtensionType};
use arrow_schema::{ArrowError, DataType, Field};
use std::collections::HashMap;
use std::sync::Arc;

/// Arrow extension type used for Pywr metric columns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetricColumnExtension(MetricColumnMetadata);

impl MetricColumnExtension {
    pub fn new(metadata: MetricColumnMetadata) -> Self {
        Self(metadata)
    }

    /// Create the extension-typed Arrow field for this metric.
    pub fn field(&self, column_name: impl Into<String>, scenario_count: i32) -> Field {
        let mut metadata = HashMap::new();
        metadata.insert(EXTENSION_TYPE_NAME_KEY.to_string(), Self::NAME.to_string());
        metadata.insert(
            EXTENSION_TYPE_METADATA_KEY.to_string(),
            self.serialize_metadata()
                .expect("metric extension metadata is serializable"),
        );
        Field::new(
            column_name,
            DataType::FixedSizeList(Arc::new(Field::new("value", DataType::Float64, true)), scenario_count),
            false,
        )
        .with_metadata(metadata)
    }
}

impl ExtensionType for MetricColumnExtension {
    const NAME: &'static str = METRIC_EXTENSION_NAME;
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
        if matches!(data_type, DataType::FixedSizeList(field, size) if *size > 0 && field.data_type() == &DataType::Float64)
        {
            Ok(())
        } else {
            Err(ArrowError::InvalidArgumentError(format!(
                "Pywr metric extension requires FixedSizeList<Float64> storage, got {data_type:?}"
            )))
        }
    }

    fn try_new(data_type: &DataType, metadata: Self::Metadata) -> Result<Self, ArrowError> {
        let extension = Self(metadata);
        extension.supports_data_type(data_type)?;
        Ok(extension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_extension_round_trips_through_field_metadata() {
        let metadata = MetricColumnMetadata {
            metric_set: "outputs".to_string(),
            name: "reservoir".to_string(),
            attribute: "volume".to_string(),
            ty: "node".to_string(),
            sub_type: Some("storage".to_string()),
        };
        let field = MetricColumnExtension::new(metadata.clone()).field("reservoir", 2);
        assert_eq!(
            MetricColumnExtension::try_new_from_field_metadata(field.data_type(), field.metadata())
                .unwrap()
                .metadata(),
            &metadata
        );
    }
}
