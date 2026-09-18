import json
from pathlib import Path

import h5py
import numpy as np
import pandas
import polars as pl
import pytest
from polars.testing import assert_frame_equal
from pywr import (
    AggregationError,
    ModelResult,
    ModelSchema,
    ModelTimings,
    MultiNetworkModelSchema,
)


@pytest.fixture()
def test_dir() -> Path:
    return Path(__file__).parent


@pytest.fixture()
def model_dir(test_dir: Path):
    return test_dir / "models"


def test_simple_time_series(model_dir: Path, tmpdir: Path):
    """Test the simple model"""

    filename = model_dir / "simple-time-series" / "model.json"

    output_fn = tmpdir / "outputs.h5"

    schema = ModelSchema.from_path(filename)
    model = schema.build(data_path=model_dir / "simple-time-series", output_path=tmpdir)
    result = model.run("clp")

    assert isinstance(result, ModelResult)
    assert output_fn.exists()

    assert isinstance(result.timings, ModelTimings)
    assert result.timings.total_duration > 0.0
    assert result.timings.speed > 0.0

    expected_data = pandas.read_csv(
        model_dir / "simple-time-series" / "expected.csv", index_col=0, header=[0, 1]
    )

    with h5py.File(output_fn, "r") as fh:
        for (node, attr), df in expected_data.items():
            simulated = np.squeeze(fh[f"{node}/{attr}"])
            np.testing.assert_allclose(simulated, df)

    with pytest.raises(AggregationError):
        result.network_result.aggregated_value("nodes")

    rb = result.network_result.to_record_batch("nodes")
    assert rb.num_rows == 365 * 3

    df = pl.from_arrow(rb)
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
        "simple-time-series",
        "simple-storage-time-series",
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


@pytest.mark.parametrize(
    "backend,file_format,infer_time_col",
    [
        ("pandas", "csv", True),
        ("pandas", "csv", False),
        ("pandas", "xlsx", True),
        ("pandas", "xlsx", False),
        ("pandas", "h5", True),
        ("pandas", "h5", False),
        ("polars", "csv", True),
        ("polars", "csv", False),
        ("polars", "parquet", True),
        ("polars", "parquet", False),
        # ("polars", "json", True),  It is not possible to infer the time column from a JSON file, so this test is skipped.
        ("polars", "json", False),
    ],
)
def test_timeseries_backends(
    model_dir: Path, tmpdir: Path, backend: str, file_format: str, infer_time_col: bool
):
    """Test the simple model with different timeseries backends"""
    filename = model_dir / "time-series-formats" / "model.json"
    input_csv = model_dir / "time-series-formats" / "inflow.csv"

    output_fn = tmpdir / "outputs.h5"

    with open(filename) as fh:
        schema_data = json.load(fh)

    # Replace the placeholder time series with a concrete backend and format
    ts_data = {"meta": {"name": "inflow"}}
    match backend:
        case "pandas":
            input_df = pandas.read_csv(input_csv, index_col=0, parse_dates=True)
            match file_format:
                case "csv":
                    # Write the input CSV to the temporary directory
                    input_df.to_csv(tmpdir / "inflow.csv")
                    ts_data.update(
                        {
                            "type": "Pandas",
                            "path": "inflow.csv",
                        }
                    )
                    if infer_time_col:
                        # If we are not inferring the time column, we need to specify the index column explicitly
                        ts_data["kwargs"] = {"index_col": 0, "parse_dates": True}

                case "xlsx":
                    input_df.to_excel(tmpdir / "inflow.xlsx")
                    ts_data.update(
                        {
                            "type": "Pandas",
                            "path": "inflow.xlsx",
                        }
                    )
                case "h5":
                    input_df.to_hdf(tmpdir / "inflow.h5", key="data")
                    ts_data.update(
                        {
                            "type": "Pandas",
                            "path": "inflow.h5",
                        }
                    )
                case _:
                    raise ValueError(f"Unknown format: {file_format}")

        case "polars":
            input_df = pl.read_csv(input_csv, try_parse_dates=True)
            match file_format:
                case "csv":
                    input_df.write_csv(tmpdir / "inflow.csv")
                    ts_data.update(
                        {
                            "type": "Polars",
                            "path": "inflow.csv",
                        }
                    )
                case "parquet":
                    input_df.write_parquet(str(tmpdir / "inflow.parquet"))
                    ts_data.update(
                        {
                            "type": "Polars",
                            "path": "inflow.parquet",
                        }
                    )
                case "json":
                    input_df.write_json(tmpdir / "inflow.json")
                    ts_data.update(
                        {
                            "type": "Polars",
                            "path": "inflow.json",
                        }
                    )
                case _:
                    raise ValueError(f"Unknown format: {file_format}")

        case _:
            raise ValueError(f"Unknown backend: {backend}")

    if not infer_time_col:
        ts_data["time_col"] = "date"

    schema_data["network"]["time_series"] = [ts_data]

    schema = ModelSchema.from_json_string(json.dumps(schema_data))
    model = schema.build(data_path=tmpdir, output_path=tmpdir)
    result = model.run("clp")

    assert isinstance(result, ModelResult)
    assert output_fn.exists()

    expected_data = pandas.read_csv(
        model_dir / "time-series-formats" / "expected.csv", index_col=0, header=[0, 1]
    )

    with h5py.File(output_fn, "r") as fh:
        print(fh.keys())
        for (node, attr), df in expected_data.items():
            simulated = np.squeeze(fh[f"{node}/{attr}"])
            np.testing.assert_allclose(simulated, df)
