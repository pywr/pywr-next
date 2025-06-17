from datetime import datetime
from os import PathLike
from typing import Optional

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
    def is_leap_year(self) -> bool:
        """Returns true if the year of the timestep is a leap year."""

class Schema:
    @classmethod
    def from_path(cls, path: PathLike) -> "Schema":
        """Create a new schema object from a file path.

        Args:
            path: The path to the schema JSON file.
        """

    @classmethod
    def from_json_string(cls, json_string: str) -> "Schema":
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

class Metric: ...
class ComponentConversionError: ...
class ConversionError: ...

def convert_model_from_v1_json_string(data: str): ...
def convert_metric_from_v1_json_string(data: str): ...
