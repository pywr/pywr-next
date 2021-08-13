from pathlib import Path
from typing import List, Union

from pydantic import validator
from pywr.pywr import PyModel  # type: ignore
from pywr.tables import TableRef

from . import BaseParameter, ParameterRef


class ControlCurveIndexParameter(BaseParameter):
    storage_node: str
    control_curves: List[ParameterRef]

    def create_parameter(self, r_model: PyModel, path: Path):
        r_model.add_control_curve_index_parameter(
            self.name, self.storage_node, self.control_curves
        )
