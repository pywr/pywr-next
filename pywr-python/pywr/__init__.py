from pathlib import Path
from typing import Optional

from .pywr import (
    ModelSchema,
    MultiNetworkModelSchema,
    Model,
    MultiNetworkModel,
    ModelResult,
    ModelTimings,
    MultiNetworkModelTimings,
    Timestep,
    ScenarioIndex,
    ParameterInfo,
    Metric,
    ComponentConversionError,
    ConversionError,
    convert_model_from_v1_json_string,
    convert_metric_from_v1_json_string,
    export_schema,
)

__all__ = [
    "ModelSchema",
    "MultiNetworkModelSchema",
    "Model",
    "MultiNetworkModel",
    "ModelResult",
    "ModelTimings",
    "MultiNetworkModelTimings",
    "Timestep",
    "ScenarioIndex",
    "ParameterInfo",
    "Metric",
    "ComponentConversionError",
    "ConversionError",
    "convert_model_from_v1_json_string",
    "convert_metric_from_v1_json_string",
    "run_from_path",
    "export_schema",
]


def run_from_path(
    filename: Path,
    data_path: Optional[Path] = None,
    output_path: Optional[Path] = None,
    solver: str = "clp",
):
    """Load and run a Pywr model from a file path.

    If the `data_path` and `output_path` are not specified, they will be set to the
    directory containing the model file.
    """

    if data_path is None:
        data_path = filename.parent
    if output_path is None:
        output_path = filename.parent

    schema = ModelSchema.from_path(filename)
    model = schema.build(data_path=data_path, output_path=output_path)
    model.run(solver)
