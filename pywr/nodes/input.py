from typing import Optional

from pywr.pywr import PyModel

from .base import BaseNode
from ..parameters import ParameterRef


class InputNode(BaseNode):
    cost: Optional[ParameterRef] = None
    min_flow: Optional[ParameterRef] = None
    max_flow: Optional[ParameterRef] = None

    def create_nodes(self, r_model: PyModel):
        r_model.add_input_node(self.name)

    def set_constraints(self, r_model: PyModel):
        if self.cost is not None:
            r_model.set_node_cost(self.name, None, self.cost)
        if self.max_flow is not None:
            r_model.set_node_constraint(self.name, None, "max_flow", self.max_flow)
        if self.min_flow is not None:
            r_model.set_node_constraint(self.name, None, "min_flow", self.min_flow)
