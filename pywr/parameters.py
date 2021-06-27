from pathlib import Path
from typing import Optional, Dict, Any, List

import numpy as np
from pydantic import BaseModel
import pandas  # type: ignore
from .pywr import PyModel  # type: ignore

_parameter_registry = {}


class BaseParameter(BaseModel):
    name: str
    comment: Optional[str] = None

    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        _parameter_registry[cls.__name__.lower()] = cls

    def create_parameter(self, r_model: PyModel, path: Path):
        raise NotImplementedError()


class ConstantParameter(BaseParameter):
    value: float

    def create_parameter(self, r_model: PyModel, path: Path):
        r_model.add_constant(self.name, self.value)


class DataFrameParameter(BaseParameter):
    """Provides  """

    url: str
    column: Optional[str] = None

    def _load_dataframe(self, path: Path) -> pandas.Series:
        url = Path(self.url)
        if not url.is_absolute():
            url = path / url
        df = pandas.read_csv(url, parse_dates=True, index_col=0)
        df = df.astype(np.float64)
        if self.column is not None:
            df = df[self.column]
        return df

    def create_parameter(self, r_model: PyModel, path: Path):
        df = self._load_dataframe(path)
        r_model.add_array(self.name, df.values)


class AggregatedParameter(BaseParameter):
    agg_func: str  # TODO enum?
    parameters: List[str]

    def create_parameter(self, r_model: PyModel, path: Path):

        r_model.add_aggregated_parameter(self.name, self.parameters, self.agg_func)


class ParameterCollection:
    def __init__(self):
        self._parameters: Dict[str, BaseParameter] = {}

    def __getitem__(self, item: str):
        return self._parameters[item]

    def __setitem__(self, key: str, value: BaseParameter):
        self._parameters[key] = value

    def __iter__(self):
        return iter(self._parameters.values())

    def __len__(self):
        return len(self._parameters)

    def __contains__(self, item):
        return item in self._parameters

    @classmethod
    def __get_validators__(cls):
        yield cls.validate

    @classmethod
    def validate(cls, data):
        if not isinstance(data, list):
            raise TypeError("list required")

        collection = cls()
        for parameter_data in data:

            if "type" not in parameter_data:
                raise ValueError('"type" key required')

            klass_name = parameter_data.pop("type") + "parameter"
            klass = _parameter_registry[klass_name]
            parameter = klass(**parameter_data)
            if parameter.name in collection:
                raise ValueError(f"Parameter name {parameter.name} already defined.")
            collection[parameter.name] = parameter
        return collection
