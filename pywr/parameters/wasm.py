from pathlib import Path
from typing import List

from pywr.pywr import PyModel  # type: ignore

from . import BaseParameter, ParameterRef
from ..tables import TableCollection


class SimpleWasmParameter(BaseParameter):
    src: Path
    parameters: List[ParameterRef]

    def _load_wasm(self, path: Path) -> bytes:
        src = self.src
        if not src.is_absolute():
            src = path / src

        with open(src, "rb") as fh:
            return fh.read()

    def create_parameter(self, r_model: PyModel, path: Path, tables: TableCollection):
        data = self._load_wasm(path)

        r_model.add_simple_wasm_parameter(
            self.name,
            data,
            self.parameters,
        )
