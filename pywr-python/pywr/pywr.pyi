from datetime import datetime
from os import PathLike
from typing import Optional, List, Tuple
import polars as pl

class ParameterInfo:
    """Provides data for a custom Pywr parameter.

    This is a read-only object that provides information that can be used for custom parameters in Pywr. It
    is passed as the first argument to the `calc` and `after` methods of custom parameter objects.
    """

    @property
    def timestep(self) -> "Timestep":
        """Returns the current time-step object."""

    @property
    def scenario_index(self) -> "ScenarioIndex":
        """Returns the current scenario index object."""

    def get_metric(self, name: str) -> float:
        """Returns a metric by name.

        Args:
            name: The name of the metric to retrieve.
        """

    def get_index(self, name: str) -> int:
        """Returns the index of a component by name.

        Args:
            name: The name of the component to retrieve the index for.
        """

class Timestep:
    """Represents a single time-step in a simulation.

    This is a read-only object that provides information about the current time-step.
    """

    @property
    def is_first(self) -> bool:
        """Returns true if this is the first time-step."""

    @property
    def days(self) -> float:
        """Returns the duration of the time-step in number of days including any fractional part."""

    @property
    def date(self) -> datetime:
        """Returns the date of the time-step."""

    @property
    def day(self) -> int:
        """Returns the day of the time-step."""

    @property
    def month(self) -> int:
        """Returns the month of the time-step."""

    @property
    def year(self) -> int:
        """Returns the year of the time-step."""

    @property
    def index(self) -> int:
        """Returns the current time-step index."""

    @property
    def day_of_year(self) -> int:
        """Returns the day of the year index of the timestep.

        The day of the year is one-based, meaning January 1st is day 1 and December 31st is day 365 (or 366 in leap years).
        """

    @property
    def day_of_year_index(self) -> int:
        """Returns the day of the year index of the timestep.

        The index is zero-based and accounts for leaps days. In non-leap years, 1 i to the index for
        days after Feb 28th.
        """

    @property
    def fractional_day_of_year(self) -> float:
        """Returns the fraction day of the year of the timestep.

        The index is zero-based and accounts for leaps days. In non-leap years, 1 is added to the index for
        days after Feb 28th. The fractional part is the fraction of the day that has passed since midnight
        (calculated to the nearest second).
        """

    @property
    def is_leap_year(self) -> bool:
        """Returns true if the year of the timestep is a leap year."""

class ScenarioIndex:
    """Represents a scenario index in a Pywr model.

    This is a read-only object that provides information about the current scenario index.
    """

    @property
    def simulation_id(self) -> int:
        """Returns the current simulation id."""

    @property
    def simulation_indices(self) -> List[int]:
        """Returns indices for each scenario group for this simulation."""

class ModelSchema:
    @classmethod
    def from_path(cls, path: PathLike) -> "ModelSchema":
        """Create a new schema object from a file path.

        Args:
            path: The path to the schema JSON file.
        """

    @classmethod
    def from_json_string(cls, json_string: str) -> "ModelSchema":
        """Create a new schema object from a JSON string.

        Args:
            json_string: The JSON string representing the schema.
        """

    def to_json_string(self) -> str:
        """Serialize the schema to a JSON string."""

    def build(
        self, data_path: Optional[PathLike], output_path: Optional[PathLike]
    ) -> "Model":
        """Build the schema in to a Pywr model."""

class MultiNetworkModelSchema:
    @classmethod
    def from_path(cls, path: PathLike) -> "ModelSchema":
        """Create a new schema object from a file path.

        Args:
            path: The path to the schema JSON file.
        """

    @classmethod
    def from_json_string(cls, json_string: str) -> "ModelSchema":
        """Create a new schema object from a JSON string.

        Args:
            json_string: The JSON string representing the schema.
        """

    def to_json_string(self) -> str:
        """Serialize the schema to a JSON string."""

    def build(
        self, data_path: Optional[PathLike], output_path: Optional[PathLike]
    ) -> "Model":
        """Build the schema in to a Pywr model."""

class Model:
    def run(self, solver_name: str, solver_kwargs: Optional[dict] = None):
        """Run the model using the specified solver.

        Args:
            solver_name: The name of the solver to use.
            solver_kwargs: Optional keyword arguments to pass to the solver.
        """

class MultiNetworkModel:
    def run(self, solver_name: str, solver_kwargs: Optional[dict] = None):
        """Run the model using the specified solver.

        Args:
            solver_name: The name of the solver to use.
            solver_kwargs: Optional keyword arguments to pass to the solver.
        """

class ModelResult:
    @property
    def network_result(self) -> "NetworkResult":
        """Returns the network result object."""

    @property
    def timings(self) -> "ModelTimings":
        """Returns the model timings object."""

class MultiNetworkModelResult:
    def network_results(self, name: str) -> "NetworkResult":
        """Get the network result for a specific network by name.

        Args:
            name: The name of the network to retrieve the results for.
        """

    @property
    def timings(self) -> "MultiNetworkModelTimings":
        """Returns the model timings object."""

class NetworkResult:
    def aggregated_value(self, name: str) -> float:
        """Get the aggregated value of a recorder by name, if it exists and can be aggregated.

        Args:
            name: The name of the output to retrieve.
        """

    def to_dataframe(self, name: str) -> pl.DataFrame:
        """Get the output of a recorder by name as a polars DataFrame.

        Args:
            name: The name of the output to retrieve.
        """

    def output_names(self) -> list[str]:
        """Get a list of all available output names."""

class ModelTimings:
    @property
    def total_duration(self) -> float:
        """Total duration of the model run in seconds."""

    @property
    def speed(self) -> float:
        """Model speed in timesteps per second."""

class MultiNetworkModelTimings:
    @property
    def total_duration(self) -> float:
        """Total duration of the model run in seconds."""

    @property
    def speed(self) -> float:
        """Model speed in timesteps per second."""

class Metric: ...
class ComponentConversionError: ...
class ConversionError: ...

def convert_model_from_v1_json_string(
    data: str,
) -> Tuple[ModelSchema, List[ComponentConversionError]]: ...
def convert_metric_from_v1_json_string(data: str) -> Metric: ...
def export_schema(out_path: PathLike): ...
