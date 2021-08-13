from pathlib import Path
from typing import List, Union

from pydantic import validator
from pywr.pywr import PyModel  # type: ignore
from pywr.tables import TableRef

from . import BaseParameter


class ParameterThresholdParameter(BaseParameter):
    parameter: str
    threshold: str
    predicate: str
    ratchet: bool = False

    def create_parameter(self, r_model: PyModel, path: Path):
        r_model.add_parameter_threshold_parameter(
            self.name, self.parameter, self.threshold, self.predicate, self.ratchet
        )
