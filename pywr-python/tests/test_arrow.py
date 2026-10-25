import json
from pathlib import Path

import pyarrow as pa
import pyarrow.parquet as pq
import pytest
from pywr import (
    METRIC_EXTENSION_NAME,
    MetricColumnExtensionType,
    MetricColumnMetadata,
    ModelSchema,
    ScenarioGroupMetadata,
    ScenarioMetadata,
    get_scenario_groups,
    get_scenarios,
    register_metric_extension_type,
    scenario_to_pandas,
)


def test_metric_extension_metadata_round_trips_through_ipc_stream():
    metadata = MetricColumnMetadata(
        metric_set="outputs",
        name="reservoir",
        attribute="volume",
        type="node",
        sub_type="storage",
    )
    extension = MetricColumnExtensionType(metadata, 2)
    field = pa.field(
        "outputs.reservoir.volume",
        extension.storage_type,
        nullable=False,
        metadata={
            b"ARROW:extension:name": METRIC_EXTENSION_NAME.encode(),
            b"ARROW:extension:metadata": extension.__arrow_ext_serialize__(),
        },
    )
    schema = pa.schema(
        [
            pa.field("time_start", pa.timestamp("ms")),
            pa.field("time_end", pa.timestamp("ms")),
            field,
        ],
        metadata={
            b"PYWR_SCENARIOS": b'[{"simulation_id":0,"simulation_indices":[0],"scenario_labels":["a"]},'
            b'{"simulation_id":1,"simulation_indices":[1],"scenario_labels":["b"]}]',
            b"PYWR_SCENARIO_GROUPS": b'[{"name":"case","size":2}]',
        },
    )
    sink = pa.BufferOutputStream()
    with pa.ipc.new_stream(sink, schema) as writer:
        writer.write_batch(
            pa.record_batch(
                [
                    pa.array([0, 1], type=pa.timestamp("ms")),
                    pa.array([1, 2], type=pa.timestamp("ms")),
                    pa.array([[1.5, None], [2.5, 3.5]], type=extension.storage_type),
                ],
                schema=schema,
            )
        )

    reader = pa.ipc.open_stream(sink.getvalue())
    result = reader.read_next_batch()
    result_type = result.schema.field("outputs.reservoir.volume").type

    assert isinstance(result_type, MetricColumnExtensionType)
    assert result_type.extension_name == METRIC_EXTENSION_NAME
    assert result_type.storage_type == extension.storage_type
    assert result_type.metadata == metadata
    assert result.column(2).storage.to_pylist() == [[1.5, None], [2.5, 3.5]]
    assert get_scenarios(result.schema) == [
        ScenarioMetadata(0, [0], ["a"]),
        ScenarioMetadata(1, [1], ["b"]),
    ]
    assert get_scenario_groups(result.schema) == [ScenarioGroupMetadata("case", 2)]
    frame = scenario_to_pandas(pa.Table.from_batches([result]), 1)
    assert frame["outputs.reservoir.volume"].isna().iloc[0]
    assert frame["outputs.reservoir.volume"].iloc[1] == 3.5
    with pytest.raises(IndexError, match="out of range"):
        scenario_to_pandas(pa.Table.from_batches([result]), 2)
    combined = scenario_to_pandas(pa.Table.from_batches([result, result]), 0)
    assert combined["outputs.reservoir.volume"].tolist() == [1.5, 2.5, 1.5, 2.5]


def test_metric_extension_serialization_and_validation():
    metadata = MetricColumnMetadata("nodes", "demand", "outflow", "node")
    extension = MetricColumnExtensionType(metadata, 2)

    assert (
        MetricColumnExtensionType.__arrow_ext_deserialize__(
            extension.storage_type, extension.__arrow_ext_serialize__()
        ).metadata
        == metadata
    )

    with pytest.raises(TypeError, match="float64 storage"):
        MetricColumnExtensionType.__arrow_ext_deserialize__(pa.int64(), b"{}")
    with pytest.raises(ValueError, match="missing required"):
        MetricColumnExtensionType.__arrow_ext_deserialize__(
            extension.storage_type, b"{}"
        )


def test_metric_extension_registration_is_idempotent():
    register_metric_extension_type()


@pytest.mark.parametrize(
    ("key", "value", "error"),
    [
        (b"PYWR_SCENARIOS", b'{"simulation_id":0}', "list of objects"),
        (b"PYWR_SCENARIOS", b'[{"simulation_id":0}]', "missing required"),
        (
            b"PYWR_SCENARIOS",
            b'[{"simulation_id":true,"simulation_indices":[0],"scenario_labels":["a"]}]',
            "nonnegative integer",
        ),
        (b"PYWR_SCENARIO_GROUPS", b'[{"name":"case","size":0}]', "positive integer"),
        (b"PYWR_SCENARIO_GROUPS", b"not json", "UTF-8 JSON"),
    ],
)
def test_scenario_metadata_rejects_invalid_values(key, value, error):
    reader = get_scenarios if key == b"PYWR_SCENARIOS" else get_scenario_groups
    with pytest.raises((TypeError, ValueError), match=error):
        reader(pa.schema([], metadata={key: value}))


def test_scenario_metadata_requires_schema_entries():
    with pytest.raises(ValueError, match="missing PYWR_SCENARIOS"):
        get_scenarios(pa.schema([]))
    with pytest.raises(ValueError, match="missing PYWR_SCENARIO_GROUPS"):
        get_scenario_groups(pa.schema([]))


def test_parquet_output_reads_as_the_arrow_stream_table(tmp_path: Path):
    output = {"metric_set": "nodes", "batch_size": 2}
    schema_data = {
        "metadata": {"title": "Arrow and Parquet outputs", "minimum_version": "0.1"},
        "scenarios": {
            "groups": [{"name": "case", "size": 2, "labels": ["dry", "wet"]}]
        },
        "time": {
            "start": "2015-01-01",
            "end": "2015-01-05",
            "timestep": {"type": "Days", "days": 1},
        },
        "network": {
            "nodes": [
                {
                    "meta": {"name": "supply"},
                    "type": "Input",
                    "max_flow": {"type": "Literal", "value": 15},
                },
                {
                    "meta": {"name": "demand"},
                    "type": "Output",
                    "max_flow": {"type": "Literal", "value": 10},
                },
            ],
            "edges": [{"from_node": "supply", "to_node": "demand"}],
            "metric_sets": [
                {
                    "meta": {"name": "nodes"},
                    "metrics": [
                        {"type": "Node", "name": "demand"},
                        {"type": "Node", "name": "supply"},
                    ],
                }
            ],
            "outputs": [
                {
                    "meta": {"name": "stream"},
                    "type": "ArrowStream",
                    "filename": "outputs.arrow",
                    **output,
                },
                {
                    "meta": {"name": "parquet"},
                    "type": "Parquet",
                    "filename": "outputs.parquet",
                    **output,
                },
            ],
        },
    }
    model = ModelSchema.from_json_string(json.dumps(schema_data)).build(
        data_path=tmp_path, output_path=tmp_path
    )
    model.run("clp")

    with pa.ipc.open_stream(tmp_path / "outputs.arrow") as reader:
        stream_table = reader.read_all()
    parquet_table = pq.read_table(tmp_path / "outputs.parquet")

    assert parquet_table.schema.equals(stream_table.schema, check_metadata=True)
    assert isinstance(parquet_table.schema.field(2).type, MetricColumnExtensionType)
    assert parquet_table.equals(stream_table)
    assert get_scenarios(parquet_table.schema) == get_scenarios(stream_table.schema)
    assert get_scenario_groups(parquet_table.schema) == [
        ScenarioGroupMetadata("case", 2)
    ]
    assert scenario_to_pandas(parquet_table, 1).equals(
        scenario_to_pandas(stream_table, 1)
    )
