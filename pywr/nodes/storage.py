from pathlib import Path
from typing import Optional

from pywr.pywr import PyModel

from pywr.nodes.base import BaseNode
from pywr.parameters import ParameterRef, BaseParameter
from pywr.tables import TableCollection


class StorageNode(BaseNode):
    max_volume: float
    cost: Optional[ParameterRef] = None
    initial_volume: float = 0.0
    min_volume: float = 0.0

    def create_nodes(self, r_model: PyModel):
        r_model.add_storage_node(self.name, None, self.initial_volume)

    def set_constraints(self, r_model: PyModel, path: Path, tables: TableCollection):
        if self.cost is not None:
            cost_name = BaseParameter.ref_to_name(
                self.cost, f"{self.name}-cost", r_model, path, tables
            )
            r_model.set_node_cost(self.name, None, cost_name)
        if self.max_volume is not None:
            # max_volume_name = BaseParameter.ref_to_name(self.max_volume, f"{self.name}-max-volume", r_model, path, tables)
            r_model.set_node_constraint(self.name, None, "max_volume", self.max_volume)
        if self.min_volume is not None:
            # min_volume_name = BaseParameter.ref_to_name(self.min_volume, f"{self.name}-min-volume", r_model, path, tables)
            r_model.set_node_constraint(self.name, None, "min_volume", self.min_volume)
