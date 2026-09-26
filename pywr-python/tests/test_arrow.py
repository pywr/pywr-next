import pyarrow as pa
import pytest
from pywr import (
    METRIC_EXTENSION_NAME,
    MetricColumnExtensionType,
    MetricColumnMetadata,
    register_metric_extension_type,
)


def test_metric_extension_metadata_round_trips_through_ipc_stream():
    metadata = MetricColumnMetadata(
        metric_set="outputs",
        name="reservoir",
        attribute="volume",
        ty="node",
        sub_type="storage",
    )
    extension = MetricColumnExtensionType(metadata)
    field = pa.field(
        "outputs.reservoir.volume",
        pa.float64(),
        nullable=False,
        metadata={
            b"ARROW:extension:name": METRIC_EXTENSION_NAME.encode(),
            b"ARROW:extension:metadata": extension.__arrow_ext_serialize__(),
        },
    )
    schema = pa.schema([field])
    sink = pa.BufferOutputStream()
    with pa.ipc.new_stream(sink, schema) as writer:
        writer.write_batch(pa.record_batch([pa.array([1.5])], schema=schema))

    reader = pa.ipc.open_stream(sink.getvalue())
    result = reader.read_next_batch()
    result_type = result.schema.field("outputs.reservoir.volume").type

    assert isinstance(result_type, MetricColumnExtensionType)
    assert result_type.extension_name == METRIC_EXTENSION_NAME
    assert result_type.storage_type == pa.float64()
    assert result_type.metadata == metadata
    assert result.column(0).to_pylist() == [1.5]


def test_metric_extension_serialization_and_validation():
    metadata = MetricColumnMetadata("nodes", "demand", "outflow", "node")
    extension = MetricColumnExtensionType(metadata)

    assert (
        MetricColumnExtensionType.__arrow_ext_deserialize__(
            pa.float64(), extension.__arrow_ext_serialize__()
        ).metadata
        == metadata
    )

    with pytest.raises(TypeError, match="float64 storage"):
        MetricColumnExtensionType.__arrow_ext_deserialize__(pa.int64(), b"{}")
    with pytest.raises(ValueError, match="missing required"):
        MetricColumnExtensionType.__arrow_ext_deserialize__(pa.float64(), b"{}")


def test_metric_extension_registration_is_idempotent():
    register_metric_extension_type()
