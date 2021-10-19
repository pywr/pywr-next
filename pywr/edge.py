from pydantic import BaseModel
from pywr.pywr import PyModel


class Edge(BaseModel):
    from_node: str
    to_node: str

    def create_edge(self, r_model: PyModel):
        r_model.connect_nodes(self.from_node, self.to_node)
