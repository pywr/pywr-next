from pathlib import Path
from typing import List, Union

from pydantic import validator
from pywr.pywr import PyModel  # type: ignore
from pywr.tables import TableRef, TableCollection

from . import BaseParameter


class MonthlyProfileParameter(BaseParameter):
    values: Union[TableRef, List[float]]

    @validator("values")
    def check_length(cls, values):
        if isinstance(values, list) and len(values) != 12:
            raise ValueError("Number of values is not 12.")
        return values

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        if isinstance(self.values, TableRef):
            values = tables.get_values(self.values, path)
        else:
            values = self.values
        r_model.add_monthly_profile_parameter(self.name, values)


class DailyProfileParameter(BaseParameter):
    values: Union[TableRef, List[float]]

    @validator("values")
    def check_length(cls, values):
        if isinstance(values, list) and len(values) != 366:
            raise ValueError("Number of values is not 366.")
        return values

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        if isinstance(self.values, TableRef):
            values = tables.get_values(self.values, path)
        else:
            values = self.values
        r_model.add_daily_profile_parameter(self.name, values)


class UniformDrawdownProfileParameter(BaseParameter):
    reset_day: int = 1
    reset_month: int = 1
    residual_days: int = 0

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        r_model.add_uniform_drawdown_profile_parameter(
            self.name, self.reset_day, self.reset_month, self.residual_days
        )
