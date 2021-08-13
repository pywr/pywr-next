from pathlib import Path
from typing import List, Union

from pydantic import validator
from pywr.pywr import PyModel  # type: ignore
from pywr.tables import TableRef

from . import BaseParameter


class MonthlyProfileParameter(BaseParameter):
    values: Union[TableRef, List[float]]

    @validator("values")
    def check_length(cls, values):
        if isinstance(values, list) and len(values) != 12:
            raise ValueError("Number of values is not 12.")
        return values

    def create_parameter(self, r_model: PyModel, path: Path):
        pass
