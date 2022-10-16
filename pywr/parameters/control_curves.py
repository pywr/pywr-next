from pathlib import Path
from typing import List, Union, Tuple, Optional

from pydantic import validator
from pywr.pywr import PyModel  # type: ignore
from pywr.tables import TableRef, TableCollection

from . import BaseParameter, ParameterRef
from .base import NodeRef, node_ref_to_names
from ..metric import ComponentMetricRef


class ControlCurvePiecewiseInterpolatedParameter(BaseParameter):
    metric: ComponentMetricRef
    values: List[Tuple[float, float]]
    control_curves: List[ParameterRef] = []
    maximum: float = 1.0
    minimum: float = 0.0

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):

        if self.metric.name == "Celyn and Brenig":
            storage_node = "Celyn"
        else:
            storage_node = self.metric.name
        self.metric.name = storage_node

        control_curve_names = BaseParameter.refs_to_name(
            self.control_curves, f"{self.name}-control-curves", r_model, path, tables
        )

        r_model.add_piecewise_control_curve(
            self.name,
            self.metric,
            control_curve_names,
            self.values,
            self.maximum,
            self.minimum,
        )


class ControlCurveIndexParameter(BaseParameter):
    metric: ComponentMetricRef
    control_curves: List[ParameterRef]

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        # TODO remove hack
        if self.metric.name == "Celyn and Brenig":
            storage_node = "Celyn"
        else:
            storage_node = self.metric.name
        if storage_node == "Haweswater and Thirlmere":
            storage_node = "Haweswater"
        if storage_node == "Pennines Total Storage":
            storage_node = "Longdendale"
        self.metric.name = storage_node
        control_curve_names = BaseParameter.refs_to_name(
            self.control_curves, f"{self.name}-control-curves", r_model, path, tables
        )
        r_model.add_control_curve_index_parameter(
            self.name, self.metric, control_curve_names
        )


class ControlCurveInterpolatedParameter(BaseParameter):
    metric: ComponentMetricRef
    control_curves: List[ParameterRef]
    values: List[float]

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        # TODO use sub-name
        control_curve_names = BaseParameter.refs_to_name(
            self.control_curves, f"{self.name}-control-curves", r_model, path, tables
        )
        r_model.add_control_curve_interpolated_parameter(
            self.name, self.metric, control_curve_names, self.values
        )


class ControlCurveParameter(BaseParameter):
    metric: ComponentMetricRef
    values: List[ParameterRef]
    control_curves: List[ParameterRef] = []

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        # TODO use sub-name
        control_curve_names = BaseParameter.refs_to_name(
            self.control_curves, f"{self.name}-control-curves", r_model, path, tables
        )
        value_names = BaseParameter.refs_to_name(
            self.values, f"{self.name}-values", r_model, path, tables
        )
        r_model.add_control_curve_parameter(
            self.name, self.metric, control_curve_names, value_names
        )
