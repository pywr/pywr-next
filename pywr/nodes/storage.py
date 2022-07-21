from typing import Optional

from pywr.pywr import PyModel

from pywr.nodes.base import BaseNode
from pywr.parameters import ParameterRef


class StorageNode(BaseNode):
    cost: Optional[ParameterRef] = None
    initial_volume: float = 0.0
    min_volume: Optional[ParameterRef] = None
    max_volume: Optional[ParameterRef] = None

    def create_nodes(self, r_model: PyModel):
        r_model.add_storage_node(self.name, None, self.initial_volume)

    def set_constraints(self, r_model: PyModel):
        if self.cost is not None:
            r_model.set_node_cost(self.name, None, self.cost)
        if self.max_volume is not None:
            r_model.set_node_constraint(self.name, None, "max_volume", self.max_volume)
