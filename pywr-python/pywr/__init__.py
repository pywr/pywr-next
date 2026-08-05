from pathlib import Path

from .pywr import (
    ComponentConversionError,
    ConversionError,
    Metric,
    Model,
    ModelResult,
    ModelSchema,
    ModelTimings,
    MultiNetworkModel,
    MultiNetworkModelSchema,
    MultiNetworkModelTimings,
    ParameterInfo,
    ScenarioIndex,
    Timestep,
    convert_metric_from_v1_json_string,
    convert_model_from_v1_json_string,
    export_schema,
)

__all__ = [
    "ComponentConversionError",
    "ConversionError",
    "Metric",
    "Model",
    "ModelResult",
    "ModelSchema",
    "ModelTimings",
    "MultiNetworkModel",
    "MultiNetworkModelSchema",
    "MultiNetworkModelTimings",
    "ParameterInfo",
    "ScenarioIndex",
    "Timestep",
    "convert_metric_from_v1_json_string",
    "convert_model_from_v1_json_string",
    "export_schema",
    "run_from_path",
]


def run_from_path(
        filename: Path,
        data_path: Path | None = None,
        output_path: Path | None = None,
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
