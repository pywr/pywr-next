from typing import List, Optional, Dict
from pydantic import BaseModel
from .pywr import PyModel  # type: ignore
from .parameters import ParameterCollection


_node_registry = {}


class BaseNode(BaseModel):
    name: str
    comment: Optional[str] = None

    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        _node_registry[cls.__name__.lower()] = cls

    def create_nodes(self, r_model: PyModel):
        raise NotImplementedError()

    @classmethod
    def __get_validators__(cls):
        yield cls.validate

    @classmethod
    def validate(cls, data):
        if not isinstance(data, dict):
            raise TypeError("dict required")
        if "type" not in data:
            raise ValueError('"type" key required')

        klass_name = data.pop("type") + "node"
        klass = _node_registry[klass_name]
        return klass(**data)


class InputNode(BaseNode):
    cost: Optional[str] = None
    min_flow: Optional[str] = None
    max_flow: Optional[str] = None

    def create_nodes(self, r_model: PyModel):
        r_model.add_input_node(self.name)


class LinkNode(BaseNode):
    cost: Optional[str] = None
    min_flow: Optional[str] = None
    max_flow: Optional[str] = None

    def create_nodes(self, r_model: PyModel):
        r_model.add_link_node(self.name)


class OutputNode(BaseNode):
    cost: Optional[str] = None
    min_flow: Optional[str] = None
    max_flow: Optional[str] = None

    def create_nodes(self, r_model: PyModel):
        r_model.add_output_node(self.name)


class Edge(BaseModel):
    from_node: str
    to_node: str

    def create_edge(self, r_model: PyModel):
        r_model.connect_nodes(self.from_node, self.to_node)


class NodeCollection:
    def __init__(self):
        self._nodes: Dict[str, BaseNode] = {}

    def __getitem__(self, item: str):
        return self._nodes[item]

    def __setitem__(self, key: str, value: BaseNode):
        self._nodes[key] = value

    def __iter__(self):
        return iter(self._nodes.values())

    def __len__(self):
        return len(self._nodes)

    def __contains__(self, item):
        return item in self._nodes

    @classmethod
    def __get_validators__(cls):
        yield cls.validate

    @classmethod
    def validate(cls, data):
        if not isinstance(data, list):
            raise TypeError("list required")

        collection = cls()
        for node_data in data:

            if "type" not in node_data:
                raise ValueError('"type" key required')

            klass_name = node_data.pop("type") + "node"
            klass = _node_registry[klass_name]
            node = klass(**node_data)
            if node.name in collection:
                raise ValueError(f'Node name "{node.name}" already defined.')
            collection[node.name] = node
        return collection


class PrintRecorder:
    def save(self, *args):
        print("saving", args)


class Model(BaseModel):
    nodes: NodeCollection
    edges: List[Edge]
    parameters: ParameterCollection

    def build(self) -> PyModel:
        """Construct a `PyModel`"""

        r_model = PyModel()
        for node in self.nodes:
            node.create_nodes(r_model)

        for edge in self.edges:
            edge.create_edge(r_model)

        for parameter in self.parameters:
            parameter.create_parameter(r_model)

        r_model.add_python_recorder("a-recorder", "NodeInFlow", 0, PrintRecorder())

        return r_model

    def run(self):
        r_model = self.build()
        r_model.run("clp")
