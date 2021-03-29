from __future__ import annotations
from pathlib import Path
from typing import List, Optional, Dict, Union
from pydantic import BaseModel
from .pywr import PyModel  # type: ignore
from .parameters import ParameterCollection
from .recorders import RecorderCollection
import json
import yaml


_node_registry = {}
_output_registry = {}


class BaseNode(BaseModel):
    name: str
    comment: Optional[str] = None

    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        _node_registry[cls.__name__.lower()] = cls

    def create_nodes(self, r_model: PyModel):
        raise NotImplementedError()

    def set_constraints(self, r_model: PyModel):
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
    cost: Optional[Union[float, str]] = None
    min_flow: Optional[Union[float, str]] = None
    max_flow: Optional[Union[float, str]] = None

    def create_nodes(self, r_model: PyModel):
        r_model.add_input_node(self.name)

    def set_constraints(self, r_model: PyModel):
        if self.cost is not None:
            r_model.set_node_cost(self.name, self.cost)
        if self.max_flow is not None:
            r_model.set_node_constraint(self.name, self.max_flow)


class LinkNode(BaseNode):
    cost: Optional[Union[float, str]] = None
    min_flow: Optional[Union[float, str]] = None
    max_flow: Optional[Union[float, str]] = None

    def create_nodes(self, r_model: PyModel):
        r_model.add_link_node(self.name)

    def set_constraints(self, r_model: PyModel):
        if self.cost is not None:
            r_model.set_node_cost(self.name, self.cost)
        if self.max_flow is not None:
            r_model.set_node_constraint(self.name, self.max_flow)


class OutputNode(BaseNode):
    cost: Optional[Union[float, str]] = None
    min_flow: Optional[Union[float, str]] = None
    max_flow: Optional[Union[float, str]] = None

    def create_nodes(self, r_model: PyModel):
        r_model.add_output_node(self.name)

    def set_constraints(self, r_model: PyModel):
        if self.cost is not None:
            r_model.set_node_cost(self.name, self.cost)
        if self.max_flow is not None:
            r_model.set_node_constraint(self.name, self.max_flow)


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


class BaseOutput(BaseModel):
    name: str

    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        _output_registry[cls.__name__.lower()] = cls

    def create_output(self, r_model: PyModel):
        raise NotImplementedError


class HDF5Output(BaseOutput):
    filename: Path

    def create_output(self, r_model: PyModel):
        r_model.add_hdf5_output(self.name, str(self.filename))


class OutputCollection:
    def __init__(self):
        self._outputs: Dict[str, BaseOutput] = {}

    def __getitem__(self, item: str):
        return self._outputs[item]

    def __setitem__(self, key: str, value: BaseOutput):
        self._outputs[key] = value

    def __iter__(self):
        return iter(self._outputs.values())

    def __len__(self):
        return len(self._outputs)

    def __contains__(self, item):
        return item in self._outputs

    def insert(self, value: BaseOutput):
        self[value.name] = value

    @classmethod
    def __get_validators__(cls):
        yield cls.validate

    @classmethod
    def validate(cls, data):
        if not isinstance(data, list):
            raise TypeError("list required")

        collection = cls()
        for output_data in data:

            if "type" not in output_data:
                raise ValueError('"type" key required')

            klass_name = output_data.pop("type") + "output"
            klass = _node_registry[klass_name]
            output = klass(**output_data)
            if output.name in collection:
                raise ValueError(f'Output name "{output.name}" already defined.')
            collection[output.name] = output
        return collection


class Timestepper(BaseModel):
    start: str
    end: str
    timestep: int


class Model(BaseModel):
    timestepper: Timestepper
    nodes: NodeCollection
    edges: List[Edge]
    parameters: ParameterCollection = ParameterCollection()
    recorders: RecorderCollection = RecorderCollection()
    outputs: OutputCollection = OutputCollection()

    @classmethod
    def from_file(cls, filepath: Path) -> Model:
        """Load a model from a file. """

        ext = filepath.suffix.lower()
        if ext == ".json":
            model = cls.from_json(filepath)
        elif ext in (".yaml", ".yml"):
            model = cls.from_yaml(filepath)
        else:
            raise ValueError(f'Filetype "{ext}" not supported.')
        return model

    @classmethod
    def from_json(cls, filepath: Path) -> Model:
        """Load a model from a JSON file. """
        with open(filepath) as fh:
            data = json.load(fh)
        return cls(**data)

    @classmethod
    def from_yaml(cls, filepath: Path) -> Model:
        """Load a model from a YAML file. """
        with open(filepath) as fh:
            data = yaml.safe_load(fh)
        return cls(**data)

    def build(self) -> PyModel:
        """Construct a `PyModel`"""

        r_model = PyModel()
        for node in self.nodes:
            node.create_nodes(r_model)

        for edge in self.edges:
            edge.create_edge(r_model)

        for parameter in self.parameters:
            print(parameter)
            parameter.create_parameter(r_model)

        for output in self.outputs:
            output.create_output(r_model)

        for node in self.nodes:
            node.set_constraints(r_model)

        # r_model.add_python_recorder("a-recorder", "NodeInFlow", 0, PrintRecorder())

        return r_model

    def run(self):
        r_model = self.build()
        r_model.run(
            "clp",
            self.timestepper.start,
            self.timestepper.end,
            self.timestepper.timestep,
        )
