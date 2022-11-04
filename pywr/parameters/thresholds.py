from pathlib import Path

from pywr.pywr import PyModel  # type: ignore

from . import BaseParameter
from ..tables import TableCollection
from ..metric import ComponentMetricRef, to_full_ref


class ThresholdParameter(BaseParameter):
    metric: ComponentMetricRef
    threshold: ComponentMetricRef
    predicate: str
    ratchet: bool = False

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        metric = to_full_ref(self.metric, f"{self.name}-metric", r_model, path, tables)
        threshold = to_full_ref(
            self.threshold, f"{self.name}-threshold", r_model, path, tables
        )

        r_model.add_threshold_parameter(
            self.name, metric, threshold, self.predicate, self.ratchet
        )
