"""PyArrow support for Pywr Arrow IPC outputs."""

from __future__ import annotations

import json
from dataclasses import asdict, dataclass
from typing import Any

import pyarrow as pa

METRIC_EXTENSION_NAME = "org.pywr.metric"


@dataclass(frozen=True, slots=True)
class MetricColumnMetadata:
    """Identity metadata attached to a Pywr Arrow metric column."""

    metric_set: str
    name: str
    attribute: str
    type: str
    sub_type: str | None = None

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> MetricColumnMetadata:
        required_fields = (
            "metric_set",
            "name",
            "attribute",
            "type",
        )
        missing = [field for field in required_fields if field not in value]
        if missing:
            raise ValueError(
                f"Pywr metric metadata is missing required field(s): {', '.join(missing)}"
            )

        for field in ["metric_set", "name", "attribute", "type"]:
            if not isinstance(value[field], str):
                raise TypeError(
                    f"Pywr metric metadata field {field!r} must be a string"
                )

        if value.get("sub_type") is not None and not isinstance(value["sub_type"], str):
            raise TypeError(
                "Pywr metric metadata field 'sub_type' must be a string or null"
            )
        return cls(
            **{field: value.get(field) for field in (*required_fields, "sub_type")}
        )


@dataclass(frozen=True, slots=True)
class ScenarioMetadata:
    """Identity and group positions for one simulated scenario."""

    simulation_id: int
    simulation_indices: list[int]
    scenario_labels: list[str]

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> ScenarioMetadata:
        required = ("simulation_id", "simulation_indices", "scenario_labels")
        missing = [field for field in required if field not in value]
        if missing:
            raise ValueError(
                f"Pywr scenario metadata is missing required field(s): {', '.join(missing)}"
            )
        if type(value["simulation_id"]) is not int or value["simulation_id"] < 0:
            raise TypeError("Pywr scenario simulation_id must be a nonnegative integer")
        if not isinstance(value["simulation_indices"], list) or any(
            type(index) is not int or index < 0 for index in value["simulation_indices"]
        ):
            raise TypeError(
                "Pywr scenario simulation_indices must be a list of nonnegative integers"
            )
        if not isinstance(value["scenario_labels"], list) or any(
            not isinstance(label, str) for label in value["scenario_labels"]
        ):
            raise TypeError("Pywr scenario scenario_labels must be a list of strings")
        return cls(**{field: value[field] for field in required})


@dataclass(frozen=True, slots=True)
class ScenarioGroupMetadata:
    """Name and size of a scenario group in the output schema."""

    name: str
    size: int

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> ScenarioGroupMetadata:
        missing = [field for field in ("name", "size") if field not in value]
        if missing:
            raise ValueError(
                f"Pywr scenario group metadata is missing required field(s): {', '.join(missing)}"
            )
        if not isinstance(value["name"], str):
            raise TypeError("Pywr scenario group name must be a string")
        if type(value["size"]) is not int or value["size"] < 1:
            raise TypeError("Pywr scenario group size must be a positive integer")
        return cls(name=value["name"], size=value["size"])


def _metadata_list(schema: pa.Schema, key: bytes) -> list[dict[str, Any]]:
    raw = (schema.metadata or {}).get(key)
    if raw is None:
        raise ValueError(f"Pywr Arrow schema is missing {key.decode()} metadata")
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f"{key.decode()} metadata must be UTF-8 JSON") from error
    if not isinstance(value, list) or any(not isinstance(item, dict) for item in value):
        raise TypeError(f"{key.decode()} metadata must be a list of objects")
    return value


def get_scenarios(schema: pa.Schema) -> list[ScenarioMetadata]:
    """Read the ordered scenario table from a Pywr Arrow schema."""
    return [
        ScenarioMetadata.from_dict(item)
        for item in _metadata_list(schema, b"PYWR_SCENARIOS")
    ]


def get_scenario_groups(schema: pa.Schema) -> list[ScenarioGroupMetadata]:
    """Read the scenario groups from a Pywr Arrow schema."""
    return [
        ScenarioGroupMetadata.from_dict(item)
        for item in _metadata_list(schema, b"PYWR_SCENARIO_GROUPS")
    ]


class MetricColumnExtensionType(pa.ExtensionType):
    """The ``org.pywr.metric`` Arrow extension type emitted by Pywr.

    The storage type is a fixed-size list of float64 values, one per scenario.
    Its ``metadata`` property identifies the metric represented by a column.
    """

    def __init__(self, metadata: MetricColumnMetadata, scenario_count: int) -> None:
        self._metadata = metadata
        super().__init__(
            pa.list_(pa.field("value", pa.float64(), nullable=True), scenario_count),
            METRIC_EXTENSION_NAME,
        )

    @property
    def metadata(self) -> MetricColumnMetadata:
        """The Pywr metric metadata carried by this column."""
        return self._metadata

    def __arrow_ext_serialize__(self) -> bytes:
        return json.dumps(asdict(self._metadata), separators=(",", ":")).encode("utf-8")

    @classmethod
    def __arrow_ext_deserialize__(
        cls, storage_type: pa.DataType, serialized: bytes
    ) -> MetricColumnExtensionType:
        if (
            not pa.types.is_fixed_size_list(storage_type)
            or storage_type.value_type != pa.float64()
            or storage_type.list_size < 1
        ):
            raise TypeError(
                f"Pywr metric extension requires fixed-size list of float64 storage, got {storage_type}"
            )
        try:
            value = json.loads(serialized.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise ValueError(
                "Pywr metric extension metadata must be UTF-8 JSON"
            ) from error
        if not isinstance(value, dict):
            raise TypeError("Pywr metric extension metadata must be a JSON object")
        return cls(MetricColumnMetadata.from_dict(value), storage_type.list_size)


def register_metric_extension_type() -> None:
    """Register the Pywr metric extension type with PyArrow.

    Registration occurs automatically when this module is imported. This
    function is idempotent and is provided for code that explicitly manages
    PyArrow extension registrations.
    """

    try:
        pa.register_extension_type(
            MetricColumnExtensionType(
                MetricColumnMetadata("", "", "", ""),
                1,
            )
        )
    except pa.ArrowKeyError:
        pass


register_metric_extension_type()


def scenario_to_pandas(table: pa.Table, scenario: int):
    """Return a DataFrame of timestamps and scalar metrics for one scenario.

    The scenario index is the position in ``table.schema.metadata[b"PYWR_SCENARIOS"]``.
    Null metric values become NaN. Requires the optional pandas dependency.
    """
    import pandas as pd

    scenarios = get_scenarios(table.schema)
    if not scenarios:
        raise ValueError("PYWR_SCENARIOS must be a nonempty scenario table")
    if not isinstance(scenario, int) or not 0 <= scenario < len(scenarios):
        raise IndexError(f"Scenario index {scenario!r} is out of range")

    frame = table.select(["time_start", "time_end"]).to_pandas()
    for name in table.column_names[2:]:
        column = table.column(name).combine_chunks()
        if not isinstance(column.type, MetricColumnExtensionType):
            raise TypeError(f"Column {name!r} is not a Pywr metric extension")
        if column.type.storage_type.list_size != len(scenarios):
            raise ValueError(f"Column {name!r} scenario count differs from metadata")
        matrix = (
            column.storage.flatten()
            .to_numpy(zero_copy_only=False)
            .reshape(-1, len(scenarios))
        )
        frame[name] = pd.Series(matrix[:, scenario], index=frame.index)
    return frame
