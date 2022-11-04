from pathlib import Path
from typing import Optional, Dict, List, Tuple, TypeVar, Iterable, Union

import numpy as np
from pydantic import BaseModel
import pandas  # type: ignore
from pywr.pywr import PyModel  # type: ignore

from pywr.metric import ComponentMetricRef, to_full_ref
from pywr.tables import TableRef, TableCollection

_parameter_registry = {}


ParameterRef = TypeVar("ParameterRef", float, str, Dict)


class NodeFullRef(BaseModel):
    """A full reference to a node and its optional sub-component."""

    node: str
    component: Optional[str] = None


NodeRef = TypeVar("NodeRef", str, NodeFullRef)


def node_ref_to_names(ref: NodeRef) -> Tuple[str, Optional[str]]:
    """Convenience function to convert a `NodeRef` to a tuple of node name and sub-component name."""
    if isinstance(ref, str):
        return ref, None
    elif isinstance(ref, NodeFullRef):
        return ref.node, ref.component
    else:
        raise ValueError(f"Node reference of type f{type(ref)} not supported.")


class BaseParameter(BaseModel):
    name: str
    comment: Optional[str] = None

    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        _parameter_registry[cls.__name__.lower()] = cls

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        raise NotImplementedError()

    @classmethod
    def from_dict(cls, **data) -> "BaseParameter":
        klass_name = data.pop("type").lower() + "parameter"
        if klass_name.lower() in (
            "vyrnwyreleaseparameter",
            "vyrnwydeadwaterparameter",
            "parsonageabstractionparameter",
            "parsonagereleaseparameter",
            "brenigregulationparameter",
            "wyreabstractionparameter",
            "swindaleabstractionparameter",
        ):
            parameter = ConstantParameter(name=data["name"], value=0)
        else:
            klass = _parameter_registry[klass_name]
            parameter = klass(**data)
        return parameter

    @classmethod
    def ref_to_name(
        cls,
        ref: ParameterRef,
        name: str,
        r_model: PyModel,
        path: Path,
        tables: TableCollection,
    ) -> str:
        """Convert a `ParameterRef` to a Parameter name.

        If the reference is a float this will create a `ConstantParameter` with name `name`. The reference is a
        string it is assumed that a parameter of this name is defined already. If the reference is a dictionary
        it is loaded as an inline parameter.
        """
        if isinstance(ref, float):
            # Create a constant for the literal float
            p = ConstantParameter(name=name, value=ref)
            p.create_parameter(r_model, path, tables)
            return p.name
        elif isinstance(ref, str):
            return ref
        elif isinstance(ref, dict):
            # Create a parameter
            p = cls.from_dict(**ref)
            p.create_parameter(r_model, path, tables)
            return p.name
        else:
            raise ValueError(f"Parameter reference of type {type(ref)} not supported.")

    @classmethod
    def refs_to_name(
        cls,
        refs: List[ParameterRef],
        name: str,
        r_model: PyModel,
        path: Path,
        tables: TableCollection,
    ) -> List[str]:
        return [
            cls.ref_to_name(ref, f"{name}-{i:02d}", r_model, path, tables)
            for i, ref in enumerate(refs)
        ]


class ConstantParameter(BaseParameter):
    value: Union[TableRef, float]

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        if isinstance(self.value, TableRef):
            value = tables.get_value(self.value, path)
        else:
            value = self.value
        r_model.add_constant(self.name, value)


class MaxParameter(BaseParameter):
    metric: ComponentMetricRef
    threshold: float = 0.0

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        metric = to_full_ref(self.metric, f"{self.name}-metric", r_model, path, tables)
        r_model.add_max_parameter(self.name, metric, self.threshold)


class NegativeParameter(BaseParameter):
    metric: ComponentMetricRef

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        metric = to_full_ref(self.metric, f"{self.name}-metric", r_model, path, tables)
        r_model.add_negative_parameter(self.name, metric)


class DataFrameParameter(BaseParameter):
    """Provides"""

    url: str
    column: Optional[str] = None

    def _load_dataframe(self, path: Path) -> pandas.Series:
        url = Path(self.url)
        if not url.is_absolute():
            url = path / url
        print(url)
        ext = url.suffix.lower()

        if ext == ".csv":
            df = pandas.read_csv(url, parse_dates=True, index_col=0)
        elif ext == ".h5":
            df = pandas.read_hdf(url)
        else:
            raise NotImplementedError(
                f"DataframeParameter does not support {ext} files."
            )
        df = df.astype(np.float64)
        if self.column is not None:
            df = df[self.column]
        return df

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        df = self._load_dataframe(path)
        r_model.add_array(self.name, df.values)


class ArrayIndexParameter(BaseParameter):
    values: List[float]

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        r_model.add_array(self.name, np.asarray(self.values))


class AsymmetricSwitchIndexParameter(BaseParameter):
    on_index_parameter: ParameterRef
    off_index_parameter: ParameterRef

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        r_model.add_asymmetric_index_parameter(
            self.name, self.on_index_parameter, self.off_index_parameter
        )


class IndexedArrayParameter(BaseParameter):
    index_parameter: ParameterRef
    parameters: List[ParameterRef]

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        index_parameter_name = BaseParameter.ref_to_name(
            self.index_parameter, f"{self.name}-index-parameter", r_model, path, tables
        )
        parameter_names = BaseParameter.refs_to_name(
            self.parameters, f"{self.name}-parameters", r_model, path, tables
        )

        r_model.add_indexed_array_parameter(
            self.name, index_parameter_name, parameter_names
        )


class AggregatedParameter(BaseParameter):
    agg_func: str  # TODO enum or typing.Literal (requires Python 3.8+)
    parameters: List[ParameterRef]

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        parameter_names = BaseParameter.refs_to_name(
            self.parameters, f"{self.name}-parameters", r_model, path, tables
        )
        r_model.add_aggregated_parameter(self.name, parameter_names, self.agg_func)


class AggregatedIndexParameter(BaseParameter):
    agg_func: str  # TODO enum or typing.Literal (requires Python 3.8+)
    parameters: List[ParameterRef]

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        parameter_names = BaseParameter.refs_to_name(
            self.parameters, f"{self.name}-parameters", r_model, path, tables
        )
        r_model.add_aggregated_index_parameter(
            self.name, parameter_names, self.agg_func
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

            parameter = BaseParameter.from_dict(**parameter_data)

            if parameter.name in collection:
                raise ValueError(f"Parameter name {parameter.name} already defined.")
            collection[parameter.name] = parameter
        return collection

    def append(self, parameter: BaseParameter):
        self._parameters[parameter.name] = parameter

    def extend(self, parameters: Iterable[BaseParameter]):
        for parameter in parameters:
            self.append(parameter)
