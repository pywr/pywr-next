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
    ty: str
    sub_type: str | None = None

    @classmethod
    def from_dict(cls, value: dict[str, Any]) -> MetricColumnMetadata:
        required_fields = ("metric_set", "name", "attribute", "ty")
        missing = [field for field in required_fields if field not in value]
        if missing:
            raise ValueError(
                f"Pywr metric metadata is missing required field(s): {', '.join(missing)}"
            )

        for field in required_fields:
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


class MetricColumnExtensionType(pa.ExtensionType):
    """The ``org.pywr.metric`` Arrow extension type emitted by Pywr.

    The storage type is always :func:`pyarrow.float64`. Its ``metadata``
    property identifies the metric represented by a column.
    """

    def __init__(self, metadata: MetricColumnMetadata) -> None:
        self._metadata = metadata
        super().__init__(pa.float64(), METRIC_EXTENSION_NAME)

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
        if storage_type != pa.float64():
            raise TypeError(
                f"Pywr metric extension requires float64 storage, got {storage_type}"
            )
        try:
            value = json.loads(serialized.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise ValueError(
                "Pywr metric extension metadata must be UTF-8 JSON"
            ) from error
        if not isinstance(value, dict):
            raise TypeError("Pywr metric extension metadata must be a JSON object")
        return cls(MetricColumnMetadata.from_dict(value))


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
            )
        )
    except pa.ArrowKeyError:
        pass


register_metric_extension_type()
