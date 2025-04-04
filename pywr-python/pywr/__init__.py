from pathlib import Path
from typing import Optional

from .pywr import *  # type: ignore


def run_from_path(
    filename: Path,
    data_path: Optional[Path] = None,
    output_path: Optional[Path] = None,
    solver: str = "clp",
):
    """Load and run a Pywr model from a file path.

    If the `data_path` and `output_path` are not specified, they will be set to the
    directory containing the model file.
    """

    if data_path is None:
        data_path = filename.parent
    if output_path is None:
        output_path = filename.parent

    schema = Schema.from_path(filename)
    model = schema.build(data_path=data_path, output_path=output_path)
    model.run(solver)
