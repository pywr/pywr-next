from __future__ import annotations
import json
from pathlib import Path
from typing import Dict, List, Optional

import yaml
from pydantic import BaseModel
from pywr.pywr import PyModel, ParameterNotFoundError

from pywr.edge import Edge
from pywr.nodes import node_registry
from pywr.nodes.base import NodeCollection
from pywr.parameters import ParameterCollection
from pywr.recorders import RecorderCollection
from pywr.tables import TableCollection, TableRef
from pywr.virtual_storage import VirtualStorageNodeCollection

_output_registry = {}


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
            klass = node_registry[klass_name]
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
    nodes: NodeCollection = NodeCollection()
    virtual_nodes: VirtualStorageNodeCollection = VirtualStorageNodeCollection()
    edges: List[Edge] = []
    parameters: ParameterCollection = ParameterCollection()
    recorders: RecorderCollection = RecorderCollection()
    tables: TableCollection = TableCollection()
    outputs: OutputCollection = OutputCollection()
    path: Optional[Path] = None  # TODO not sure about this one.

    @classmethod
    def from_file(cls, filepath: Path) -> Model:
        """Load a model from a file."""

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
        """Load a model from a JSON file."""
        with open(filepath) as fh:
            data = json.load(fh)
        return cls(path=filepath.parent, **data)

    @classmethod
    def from_yaml(cls, filepath: Path) -> Model:
        """Load a model from a YAML file."""
        with open(filepath) as fh:
            data = yaml.safe_load(fh)
        return cls(path=filepath.parent, **data)

    def create_edge(self, from_node: str, to_node: str, r_model: PyModel):
        for out_node_name, out_node_sub_name in self.nodes[
            from_node
        ].iter_output_connectors():
            for in_node_name, in_node_sub_name in self.nodes[
                to_node
            ].iter_input_connectors():
                r_model.connect_nodes(
                    out_node_name, out_node_sub_name, in_node_name, in_node_sub_name
                )

    def get_table_value(self, ref: TableRef) -> float:
        # TODO actually read the data
        return 0.0

    def build(self) -> PyModel:
        """Construct a `PyModel`"""

        r_model = PyModel()
        for node in self.nodes:
            node.create_nodes(r_model)

        for node in self.virtual_nodes:
            node.create_nodes(r_model)

        for edge in self.edges:
            self.create_edge(edge.from_node, edge.to_node, r_model)

        # Build the parameters ...
        remaining_parameters = [p for p in self.parameters]
        while len(remaining_parameters) > 0:
            failed_parameters = []  # Collection for parameters that fail to load
            for parameter in remaining_parameters:
                try:
                    parameter.create_parameter(r_model, self.path, self.tables)
                except ParameterNotFoundError:
                    # Parameter failed due to not finding another parameter.
                    # This is likely a dependency that is not yet loaded.
                    failed_parameters.append(parameter)

            if len(failed_parameters) >= len(remaining_parameters):
                for p in failed_parameters:
                    print(p.name, type(p), p)
                raise RuntimeError(
                    "Failed to load parameters due to a cycle in the dependency tree."
                )
            remaining_parameters = failed_parameters

        for recorder in self.recorders:
            recorder.create_recorder(r_model)

        for output in self.outputs:
            output.create_output(r_model)

        for node in self.nodes:
            node.set_constraints(r_model, self.path, self.tables)

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
