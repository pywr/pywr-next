import numpy as np
import pandas
from pywr.nodes import Model, HDF5Output
from pathlib import Path
import h5py
import pytest


@pytest.fixture()
def test_dir() -> Path:
    return Path(__file__).parent


@pytest.fixture()
def model_dir(test_dir: Path):
    return test_dir / "models"


def test_simple_timeseries(model_dir: Path, tmpdir: Path):
    """Test the simple model"""

    filename = model_dir / "simple-timeseries" / "model.json"
    model = Model.from_file(filename)

    output_fn = tmpdir / "output.h5"
    model.outputs.insert(HDF5Output(name="hdf5", filename=output_fn))

    model.run()

    assert output_fn.exists()

    expected_data = pandas.read_csv(model_dir / "simple-timeseries" / "expected.csv")

    with h5py.File(output_fn, "r") as fh:
        for node in model.nodes:
            np.testing.assert_allclose(
                np.squeeze(fh[node.name]), expected_data[node.name]
            )


# TODO these tests could be auto-discovered.
@pytest.mark.parametrize(
    "model_name",
    ["simple-timeseries", "simple-storage-timeseries", "simple-wasm"],
)
def test_model(model_dir: Path, tmpdir: Path, model_name: str):

    filename = model_dir / model_name / "model.json"
    model = Model.from_file(filename)

    output_fn = tmpdir / "output.h5"
    model.outputs.insert(HDF5Output(name="hdf5", filename=output_fn))

    model.run()

    assert output_fn.exists()

    expected_fn = model_dir / model_name / "expected.csv"
    if not expected_fn.exists():
        expected_fn = model_dir / model_name / "expected.csv.gz"

    expected_data = pandas.read_csv(expected_fn)

    with h5py.File(output_fn, "r") as fh:
        for node in model.nodes:
            np.testing.assert_allclose(
                np.squeeze(fh[node.name]), expected_data[node.name]
            )


@pytest.mark.parametrize(
    "model_name",
    ["simple-timeseries", "simple-storage-timeseries", "simple-wasm"],
)
def test_model_benchmark(benchmark, model_dir: Path, model_name: str):

    filename = model_dir / model_name / "model.json"
    model = Model.from_file(filename)

    r_model = model.build()

    benchmark(
        r_model.run,
        "clp",
        model.timestepper.start,
        model.timestepper.end,
        model.timestepper.timestep,
    )
