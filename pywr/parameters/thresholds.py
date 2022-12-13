from pathlib import Path

from pywr.pywr import PyModel  # type: ignore

from . import BaseParameter
from ..tables import TableCollection
from .._metric import ComponentMetricRef, to_full_ref


class ThresholdParameter(BaseParameter):
    _metric: ComponentMetricRef
    threshold: ComponentMetricRef
    predicate: str
    ratchet: bool = False

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        _metric = to_full_ref(
            self._metric, f"{self.name}-_metric", r_model, path, tables
        )
        threshold = to_full_ref(
            self.threshold, f"{self.name}-threshold", r_model, path, tables
        )

        r_model.add_threshold_parameter(
            self.name, _metric, threshold, self.predicate, self.ratchet
        )
