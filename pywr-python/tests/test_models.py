import numpy as np
import pandas
import polars as pl
from polars.testing import assert_frame_equal
from pywr import ModelSchema, ModelResult, MultiNetworkModelSchema, ModelTimings
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

    output_fn = tmpdir / "outputs.h5"

    schema = ModelSchema.from_path(filename)
    model = schema.build(data_path=model_dir / "simple-timeseries", output_path=tmpdir)
    result = model.run("clp")

    assert isinstance(result, ModelResult)
    assert output_fn.exists()

    assert isinstance(result.timings, ModelTimings)
    assert result.timings.total_duration > 0.0
    assert result.timings.speed > 0.0

    expected_data = pandas.read_csv(
        model_dir / "simple-timeseries" / "expected.csv", index_col=0, header=[0, 1]
    )

    with h5py.File(output_fn, "r") as fh:
        for (node, attr), df in expected_data.items():
            simulated = np.squeeze(fh[f"{node}/{attr}"])
            np.testing.assert_allclose(simulated, df)

    with pytest.raises(RuntimeError):
        result.network_result.aggregated_value("nodes")

    df = result.network_result.to_dataframe("nodes")
    assert df.shape[0] == 365 * 3

    mean_flows = df.group_by(pl.col("name")).agg(pl.col("value").mean()).sort("name")
    assert mean_flows.shape[0] == 3

    expected_mean_flows = pl.DataFrame(
        {
            "name": ["input1", "link1", "output1"],
            "value": [8.520548, 8.520548, 8.520548],
        }
    )

    assert_frame_equal(mean_flows, expected_mean_flows)


# TODO these tests could be auto-discovered.
@pytest.mark.parametrize(
    "model_name",
    [
        "simple-timeseries",
        "simple-storage-timeseries",
        "simple-custom-parameter",
        "aggregated-node1",
        "piecewise-link1",
    ],
)
def test_model(model_dir: Path, tmpdir: Path, model_name: str):
    filename = model_dir / model_name / "model.json"
    output_fn = tmpdir / "outputs.h5"

    schema = ModelSchema.from_path(filename)
    model = schema.build(data_path=model_dir / model_name, output_path=tmpdir)
    model.run("clp")

    assert output_fn.exists()

    expected_fn = model_dir / model_name / "expected.csv"
    if not expected_fn.exists():
        expected_fn = model_dir / model_name / "expected.csv.gz"

    expected_data = pandas.read_csv(expected_fn, index_col=0, header=[0, 1]).astype(
        "float64"
    )

    with h5py.File(output_fn, "r") as fh:
        for (node, attr), df in expected_data.items():
            simulated = np.squeeze(fh[f"{node}/{attr}"])
            np.testing.assert_allclose(simulated, df)


@pytest.mark.parametrize(
    "model_name",
    [
        "multi1",
    ],
)
def test_multi_model(model_dir: Path, model_name: str):
    """Test the multi-network model"""
    filename = model_dir / model_name / "model.json"

    schema = MultiNetworkModelSchema.from_path(filename)
    model = schema.build(data_path=model_dir / model_name, output_path=None)
    model.run("clp")
