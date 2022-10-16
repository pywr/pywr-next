from pathlib import Path
from typing import Optional, Generator, Tuple, List
from pydantic import validator
from pywr.pywr import PyModel

from pywr.nodes.base import BaseNode
from pywr.parameters import ParameterRef, BaseParameter
from pywr.tables import TableCollection


class PiecewiseLinkNode(BaseNode):
    costs: Optional[List[Optional[ParameterRef]]] = None
    min_flows: Optional[List[Optional[ParameterRef]]] = None
    max_flows: Optional[List[Optional[ParameterRef]]] = None

    @validator("costs")
    def costs_length(cls, v, values, **kwargs):
        """Validate that any costs supplied match the length of min_flows and max_flows"""
        for attr in ("min_flows", "max_flows"):
            if v is not None and attr in values and len(v) != len(values[attr]):
                raise ValueError(
                    f"Length of costs ({len(v)}) does not match length of min_flows ({len(values[attr])})."
                )
        return v

    # TODO validators for min_flows and max_flows

    def iter_attributes(
        self,
    ) -> Generator[
        Tuple[
            str, Optional[ParameterRef], Optional[ParameterRef], Optional[ParameterRef]
        ],
        None,
        None,
    ]:

        n = None
        if self.costs is not None:
            n = len(self.costs)

        if self.min_flows is not None:
            if n is None:
                n = len(self.min_flows)
            else:
                assert n == len(
                    self.min_flows
                )  # TODO raise a more information exception.
        if self.max_flows is not None:
            if n is None:
                n = len(self.max_flows)
            else:
                assert n == len(
                    self.max_flows
                )  # TODO raise a more information exception.

        if n is None:
            # None of the attributes have been defined.
            raise ValueError(
                "Unable to determine the number of entries a PiecewiseLink because no attributes are defined."
            )

        for i in range(n):
            cost = self.costs[i] if self.costs is not None else None
            min_flow = self.min_flows[i] if self.min_flows is not None else None
            max_flow = self.max_flows[i] if self.max_flows is not None else None
            sub_name = f"link-{i:02d}"
            yield sub_name, cost, min_flow, max_flow

    def iter_input_connectors(self) -> Generator[Tuple[str, Optional[str]], None, None]:
        for sub_name, _, _, _ in self.iter_attributes():
            yield self.name, sub_name

    def iter_output_connectors(
        self,
    ) -> Generator[Tuple[str, Optional[str]], None, None]:
        for sub_name, _, _, _ in self.iter_attributes():
            yield self.name, sub_name

    def iter_contents(self) -> Generator[Tuple[str, Optional[str]], None, None]:
        for sub_name, _, _, _ in self.iter_attributes():
            yield self.name, sub_name

    def create_nodes(self, r_model: PyModel):
        for sub_name, _, _, _ in self.iter_attributes():
            r_model.add_link_node(self.name, sub_name)

    def set_constraints(self, r_model: PyModel, path: Path, tables: TableCollection):
        for sub_name, cost, _, max_flow in self.iter_attributes():
            # TODO min_flow
            if cost is not None:
                cost_name = BaseParameter.ref_to_name(
                    cost, f"{self.name}-{sub_name}-cost", r_model, path, tables
                )
                r_model.set_node_cost(self.name, sub_name, cost_name)
            if max_flow is not None:
                max_flow_name = BaseParameter.ref_to_name(
                    cost, f"{self.name}-{sub_name}-max-flow", r_model, path, tables
                )
                r_model.set_node_constraint(
                    self.name, sub_name, "max_flow", max_flow_name
                )
