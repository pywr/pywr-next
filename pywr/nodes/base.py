from __future__ import annotations

from pathlib import Path
from typing import Optional, Dict, Iterable, Generator, Tuple, Any
from pydantic import BaseModel, Extra
from ..pywr import PyModel, ParameterNotFoundError  # type: ignore
from ..tables import TableCollection

node_registry = {}


class BaseNode(BaseModel, extra=Extra.forbid):
    name: str
    comment: Optional[str] = None
    position: Optional[Any] = None

    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        node_registry[cls.__name__.lower()] = cls

    def create_nodes(self, r_model: PyModel):
        raise NotImplementedError()

    def set_constraints(self, r_model: PyModel, path: Path, tables: TableCollection):
        raise NotImplementedError()

    def iter_input_connectors(self) -> Generator[Tuple[str, Optional[str]], None, None]:
        yield self.name, None

    def iter_output_connectors(
        self,
    ) -> Generator[Tuple[str, Optional[str]], None, None]:
        yield self.name, None

    def iter_contents(self) -> Generator[Tuple[str, Optional[str]], None, None]:
        yield self.name, None

    @classmethod
    def get_class(cls, node_type: str) -> BaseNode:
        return node_registry[node_type.lower() + "node"]

    @classmethod
    def from_data(cls, node_data) -> BaseNode:
        klass = cls.get_class(node_data.pop("type"))
        return klass(**node_data)

    @classmethod
    def __get_validators__(cls):
        yield cls.validate

    @classmethod
    def validate(cls, data):
        if not isinstance(data, dict):
            raise TypeError("dict required")
        if "type" not in data:
            raise ValueError('"type" key required')

        klass = cls.get_class(data.pop("type"))
        return klass(**data)


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

            node = BaseNode.from_data(node_data)
            if node.name in collection:
                raise ValueError(f'Node name "{node.name}" already defined.')
            collection[node.name] = node
        return collection

    def append(self, node: BaseNode):
        self._nodes[node.name] = node

    def extend(self, nodes: Iterable[BaseNode]):
        for node in nodes:
            self.append(node)
