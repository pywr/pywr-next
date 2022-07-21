from pydantic import BaseModel
from pywr.pywr import PyModel


class Edge(BaseModel):
    from_node: str
    to_node: str
