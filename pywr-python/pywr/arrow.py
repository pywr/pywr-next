"""PyArrow support for Pywr Arrow IPC outputs."""

from __future__ import annotations

import json
from dataclasses import dataclass, fields
from dataclasses import field as dataclass_field
from typing import Any, ClassVar

import pyarrow as pa

METRIC_EXTENSION_NAME = "org.pywr.metric"


class _MetricSourceBase:
    """Shared (de)serialization of the metric source kinds.

    Each kind is written as a JSON object with a ``type`` key naming it, followed by its
    fields. Fields that may be ``None`` may also be left out.
    """

    __slots__ = ()
    type: ClassVar[str]

    def to_dict(self) -> dict[str, Any]:
        return {
            "type": self.type,
            **{f.name: getattr(self, f.name) for f in fields(self)},
        }

    @classmethod
    def _from_dict(cls, value: dict[str, Any]):
        known = {f.name: f for f in fields(cls)}
        unknown = [key for key in value if key != "type" and key not in known]
        if unknown:
            raise ValueError(
                f"Pywr {cls.type} metric source has unknown field(s): {', '.join(unknown)}"
            )
        kwargs = {}
        for name, field in known.items():
            optional = "None" in str(field.type)
            item = value.get(name)
            if item is None and not optional:
                raise ValueError(
                    f"Pywr {cls.type} metric source is missing required field {name!r}"
                )
            if item is not None and not isinstance(item, str):
                raise TypeError(
                    f"Pywr {cls.type} metric source field {name!r} must be a string"
                )
            kwargs[name] = item
        return cls(**kwargs)

    @property
    def label(self) -> str:
        """The single-string label Pywr gives this source, as in a CSV output."""
        return self.name  # type: ignore[attr-defined]


def _with_slot(node: str, slot: str | None) -> str:
    return node if slot is None else f"{node}[{slot}]"


@dataclass(frozen=True, slots=True)
class NodeSource(_MetricSourceBase):
    """A node of the schema."""

    type: ClassVar[str] = "Node"
    name: str
    node_type: str


@dataclass(frozen=True, slots=True)
class VirtualNodeSource(_MetricSourceBase):
    """A virtual node of the schema."""

    type: ClassVar[str] = "VirtualNode"
    name: str
    node_type: str


@dataclass(frozen=True, slots=True)
class CoreNodeSource(_MetricSourceBase):
    """A node of the solver's network, built by the schema node ``node``.

    ``sub_name`` distinguishes it from the other core nodes that node builds. These names
    are internal to Pywr and may change between versions.
    """

    type: ClassVar[str] = "CoreNode"
    node: str
    node_type: str
    sub_name: str | None = None

    @property
    def label(self) -> str:
        return _with_slot(self.node, self.sub_name)


@dataclass(frozen=True, slots=True)
class EdgeSource(_MetricSourceBase):
    """An edge between two nodes, with the slots it connects, if any."""

    type: ClassVar[str] = "Edge"
    from_node: str
    to_node: str
    from_slot: str | None = None
    to_slot: str | None = None

    @property
    def label(self) -> str:
        return f"{_with_slot(self.from_node, self.from_slot)}->{_with_slot(self.to_node, self.to_slot)}"


@dataclass(frozen=True, slots=True)
class ParameterSource(_MetricSourceBase):
    """A parameter; ``parent`` is the node a local parameter belongs to."""

    type: ClassVar[str] = "Parameter"
    name: str
    parameter_type: str
    parent: str | None = None

    @property
    def label(self) -> str:
        return self.name if self.parent is None else f"{self.parent}.{self.name}"


@dataclass(frozen=True, slots=True)
class TableSource(_MetricSourceBase):
    """A table."""

    type: ClassVar[str] = "Table"
    name: str


@dataclass(frozen=True, slots=True)
class TimeSeriesSource(_MetricSourceBase):
    """A time series."""

    type: ClassVar[str] = "TimeSeries"
    name: str


@dataclass(frozen=True, slots=True)
class InterNetworkTransferSource(_MetricSourceBase):
    """A transfer from another network."""

    type: ClassVar[str] = "InterNetworkTransfer"
    name: str


MetricSource = (
    NodeSource
    | VirtualNodeSource
    | CoreNodeSource
    | EdgeSource
    | ParameterSource
    | TableSource
    | TimeSeriesSource
    | InterNetworkTransferSource
)
"""What a metric reads from; one class per kind of source."""

_METRIC_SOURCE_KINDS: dict[str, type[_MetricSourceBase]] = {
    kind.type: kind
    for kind in (
        NodeSource,
        VirtualNodeSource,
        CoreNodeSource,
        EdgeSource,
        ParameterSource,
        TableSource,
        TimeSeriesSource,
        InterNetworkTransferSource,
    )
}


def metric_source_from_dict(value: dict[str, Any]) -> MetricSource:
    """Read a metric source from its JSON object, choosing the class by its ``type``."""
    if not isinstance(value, dict):
        raise TypeError("Pywr metric source must be a JSON object")
    kind = value.get("type")
    if not isinstance(kind, str):
        raise TypeError("Pywr metric source must have a string 'type'")
    try:
        cls = _METRIC_SOURCE_KINDS[kind]
    except KeyError:
        raise ValueError(f"Unknown Pywr metric source type {kind!r}") from None
    return cls._from_dict(value)


@dataclass(frozen=True, slots=True)
class MetricColumnMetadata:
    """Identity metadata attached to a Pywr Arrow metric column."""

    metric_set: str
    source: MetricSource
    attribute: str
    network: str | None = None
    """The metric's network in a multi-network model; ``None`` for a single network."""

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> MetricColumnMetadata:
        required_fields = ("metric_set", "source", "attribute")
        missing = [field for field in required_fields if field not in value]
        if missing:
            raise ValueError(
                f"Pywr metric metadata is missing required field(s): {', '.join(missing)}"
            )

        for field in ["metric_set", "attribute"]:
            if not isinstance(value[field], str):
                raise TypeError(
                    f"Pywr metric metadata field {field!r} must be a string"
                )

        if value.get("network") is not None and not isinstance(value["network"], str):
            raise TypeError(
                "Pywr metric metadata field 'network' must be a string or null"
            )
        return cls(
            metric_set=value["metric_set"],
            source=metric_source_from_dict(value["source"]),
            attribute=value["attribute"],
            network=value.get("network"),
        )

    def to_dict(self) -> dict[str, Any]:
        return {
            "metric_set": self.metric_set,
            "source": self.source.to_dict(),
            "attribute": self.attribute,
            **({"network": self.network} if self.network is not None else {}),
        }


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


@dataclass(frozen=True, slots=True)
class SourceFileMetadata:
    """A file a run's model was composed from, with the SHA-256 hash of its contents."""

    path: str
    sha256: str

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> SourceFileMetadata:
        for name in ("path", "sha256"):
            if not isinstance(value.get(name), str):
                raise TypeError(f"Pywr source file field {name!r} must be a string")
        return cls(path=value["path"], sha256=value["sha256"])


@dataclass(frozen=True, slots=True)
class ProjectRunMetadata:
    """The project definition a run's model was composed from.

    ``files`` holds the project's manifest followed by every file the model was composed
    from, in merge order, with paths relative to the project's directory. A file in a
    dependency outside that directory has a path starting with ``..``.
    """

    definition: str
    files: list[SourceFileMetadata]

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> ProjectRunMetadata:
        if not isinstance(value.get("definition"), str):
            raise TypeError(
                "Pywr project run metadata field 'definition' must be a string"
            )
        files = value.get("files")
        if not isinstance(files, list) or any(
            not isinstance(item, dict) for item in files
        ):
            raise TypeError(
                "Pywr project run metadata field 'files' must be a list of objects"
            )
        return cls(
            definition=value["definition"],
            files=[SourceFileMetadata.from_dict(item) for item in files],
        )


@dataclass(frozen=True, slots=True)
class RunMetadata:
    """Metadata describing the run that wrote an output."""

    run_name: str | None = None
    project: ProjectRunMetadata | None = None
    extra: dict[str, Any] = dataclass_field(default_factory=dict)

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> RunMetadata:
        run_name = value.get("run_name")
        if run_name is not None and not isinstance(run_name, str):
            raise TypeError(
                "Pywr run metadata field 'run_name' must be a string or null"
            )
        project = value.get("project")
        if project is not None and not isinstance(project, dict):
            raise TypeError(
                "Pywr run metadata field 'project' must be an object or null"
            )
        extra = value.get("extra", {})
        if not isinstance(extra, dict):
            raise TypeError("Pywr run metadata field 'extra' must be an object")
        return cls(
            run_name=run_name,
            project=ProjectRunMetadata.from_dict(project)
            if project is not None
            else None,
            extra=extra,
        )


def _metadata_json(schema: pa.Schema, key: bytes) -> Any | None:
    raw = (schema.metadata or {}).get(key)
    if raw is None:
        return None
    try:
        return json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f"{key.decode()} metadata must be UTF-8 JSON") from error


def _metadata_list(schema: pa.Schema, key: bytes) -> list[dict[str, Any]]:
    value = _metadata_json(schema, key)
    if value is None:
        raise ValueError(f"Pywr Arrow schema is missing {key.decode()} metadata")
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


def get_run_metadata(schema: pa.Schema) -> RunMetadata | None:
    """Read the run metadata from a Pywr Arrow schema, or ``None`` if the run had none."""
    value = _metadata_json(schema, b"PYWR_RUN")
    if value is None:
        return None
    if not isinstance(value, dict):
        raise TypeError("PYWR_RUN metadata must be an object")
    return RunMetadata.from_dict(value)


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
        return json.dumps(self._metadata.to_dict(), separators=(",", ":")).encode(
            "utf-8"
        )

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
                MetricColumnMetadata("", NodeSource("", ""), ""),
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
