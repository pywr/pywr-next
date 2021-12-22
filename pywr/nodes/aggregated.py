from typing import Optional, List

from pywr.pywr import PyModel

from pywr.nodes.base import BaseNode
from pywr.parameters import ParameterRef


class AggregatedNode(BaseNode):
    nodes: List[str]
    min_flow: Optional[ParameterRef] = None
    max_flow: Optional[ParameterRef] = None

    def create_nodes(self, r_model: PyModel):
        r_model.add_aggregated_node(self.name, self.nodes)

    def set_constraints(self, r_model: PyModel):
        if self.min_flow is not None:
            raise NotImplementedError(
                "Minimum flow constraints not implemented for aggregated node."
            )
        if self.max_flow is not None:
            r_model.set_aggregated_node_constraint(self.name, "max_flow", self.max_flow)
