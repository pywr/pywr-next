from pathlib import Path

from ._pywr import (
    AggregationError,
    ComponentConversionError,
    ConversionError,
    FinaliseError,
    Metric,
    Model,
    ModelResult,
    ModelSchema,
    ModelTimings,
    MultiNetworkModel,
    MultiNetworkModelSchema,
    MultiNetworkModelTimings,
    MultiNetworkSchemaBuildError,
    NetworkBuildError,
    NetworkTransferError,
    ParameterInfo,
    PywrError,
    RecorderDoesNotSupportAggregation,
    ScenarioIndex,
    SchemaBuildError,
    SchemaReadError,
    SetupError,
    StepError,
    Timestep,
    convert_metric_from_v1_json_string,
    convert_model_from_v1_json_string,
    export_schema,
)
from .arrow import (
    METRIC_EXTENSION_NAME,
    CoreNodeSource,
    EdgeSource,
    InterNetworkTransferSource,
    MetricColumnExtensionType,
    MetricColumnMetadata,
    MetricSource,
    NodeSource,
    ParameterSource,
    ProjectRunMetadata,
    RunMetadata,
    ScenarioGroupMetadata,
    ScenarioMetadata,
    SourceFileMetadata,
    TableSource,
    TimeSeriesSource,
    VirtualNodeSource,
    get_run_metadata,
    get_scenario_groups,
    get_scenarios,
    metric_source_from_dict,
    register_metric_extension_type,
    scenario_to_pandas,
)

__all__ = [
    "METRIC_EXTENSION_NAME",
    "AggregationError",
    "ComponentConversionError",
    "ConversionError",
    "CoreNodeSource",
    "EdgeSource",
    "FinaliseError",
    "InterNetworkTransferSource",
    "Metric",
    "MetricColumnExtensionType",
    "MetricColumnMetadata",
    "MetricSource",
    "Model",
    "ModelResult",
    "ModelSchema",
    "ModelTimings",
    "MultiNetworkModel",
    "MultiNetworkModelSchema",
    "MultiNetworkModelTimings",
    "MultiNetworkSchemaBuildError",
    "NetworkBuildError",
    "NetworkTransferError",
    "NodeSource",
    "ParameterInfo",
    "ParameterSource",
    "ProjectRunMetadata",
    "PywrError",
    "RecorderDoesNotSupportAggregation",
    "RunMetadata",
    "ScenarioGroupMetadata",
    "ScenarioIndex",
    "ScenarioMetadata",
    "SchemaBuildError",
    "SchemaReadError",
    "SetupError",
    "SourceFileMetadata",
    "StepError",
    "TableSource",
    "TimeSeriesSource",
    "Timestep",
    "VirtualNodeSource",
    "convert_metric_from_v1_json_string",
    "convert_model_from_v1_json_string",
    "export_schema",
    "get_run_metadata",
    "get_scenario_groups",
    "get_scenarios",
    "metric_source_from_dict",
    "register_metric_extension_type",
    "run_from_path",
    "scenario_to_pandas",
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
