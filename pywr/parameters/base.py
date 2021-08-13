from pathlib import Path
from typing import Optional, Dict, List, Tuple, TypeVar, Iterable, Union

import numpy as np
from pydantic import BaseModel
import pandas  # type: ignore
from pywr.pywr import PyModel  # type: ignore

from pywr.tables import TableRef

_parameter_registry = {}


ParameterRef = TypeVar("ParameterRef", float, str, Dict)


class BaseParameter(BaseModel):
    name: str
    comment: Optional[str] = None

    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        _parameter_registry[cls.__name__.lower()] = cls

    def create_parameter(self, r_model: PyModel, path: Path):
        raise NotImplementedError()


class ConstantParameter(BaseParameter):
    value: Union[TableRef, float]

    def create_parameter(self, r_model: PyModel, path: Path):
        r_model.add_constant(self.name, self.value)


class DataFrameParameter(BaseParameter):
    """Provides"""

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


class ArrayIndexParameter(BaseParameter):
    values: List[float]

    def create_parameter(self, r_model: PyModel, path: Path):
        r_model.add_array(self.name, np.asarray(self.values))


class AsymmetricSwitchIndexParameter(BaseParameter):
    on_index_parameter: ParameterRef
    off_index_parameter: ParameterRef

    def create_parameter(self, r_model: PyModel, path: Path):
        r_model.add_asymmetric_index_parameter(
            self.name, self.on_index_parameter, self.off_index_parameter
        )


class IndexedArrayParameter(BaseParameter):
    index_parameter: ParameterRef
    parameters: List[ParameterRef]

    def create_parameter(self, r_model: PyModel, path: Path):
        r_model.add_indexed_array_parameter(
            self.name, self.index_parameter, self.parameters
        )


class AggregatedParameter(BaseParameter):
    agg_func: str  # TODO enum or typing.Literal (requires Python 3.8+)
    parameters: List[ParameterRef]

    def create_parameter(self, r_model: PyModel, path: Path):
        r_model.add_aggregated_parameter(self.name, self.parameters, self.agg_func)


class AggregatedIndexParameter(BaseParameter):
    agg_func: str  # TODO enum or typing.Literal (requires Python 3.8+)
    parameters: List[ParameterRef]

    def create_parameter(self, r_model: PyModel, path: Path):
        r_model.add_aggregated_index_parameter(
            self.name, self.parameters, self.agg_func
        )


class ControlCurvePiecewiseInterpolatedParameter(BaseParameter):
    storage_node: str
    control_curves: List[str]
    values: List[Tuple[float, float]]
    maximum: float = 1.0
    minimum: float = 0.0

    def create_parameter(self, r_model: PyModel, path: Path):
        r_model.add_piecewise_control_curve(
            self.name,
            self.storage_node,
            self.control_curves,
            self.values,
            self.maximum,
            self.minimum,
        )


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
            print(parameter_data)

            if "type" not in parameter_data:
                raise ValueError('"type" key required')

            klass_name = parameter_data.pop("type").lower() + "parameter"
            klass = _parameter_registry[klass_name]
            parameter = klass(**parameter_data)
            if parameter.name in collection:
                raise ValueError(f"Parameter name {parameter.name} already defined.")
            collection[parameter.name] = parameter
        return collection

    def append(self, parameter: BaseParameter):
        self._parameters[parameter.name] = parameter

    def extend(self, parameters: Iterable[BaseParameter]):
        for parameter in parameters:
            self.append(parameter)
