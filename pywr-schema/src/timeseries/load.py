from pathlib import Path

import pyarrow as pa


def load_pandas(path: str, index_col: str | int, **kwargs) -> pa.RecordBatch:
    """Helper function to load a pandas DataFrame from a file, and then convert it to a pyarrow RecordBatch.

    This function is used by the `load` function of the `PandasTimeseries` in the Rust extension.
    """
    import pandas as pd

    suffix = Path(path).suffix.lower()
    match suffix:
        case ".csv":
            df = pd.read_csv(path, index_col=index_col, **kwargs)
        case ".xlsx":
            df = pd.read_excel(path, index_col=index_col, **kwargs)
        case ".h5":
            df = pd.read_hdf(path, **kwargs)
        case _:
            raise ValueError(f"Unsupported file format: {suffix}")

    return pa.RecordBatch.from_pandas(df)


def load_polars(path: str, _index_col: str | int, **kwargs) -> pa.RecordBatch:
    """Helper function to load a polars DataFrame from a file, and then convert it to a pyarrow RecordBatch.

    This function is used by the `load` function of the `PolarsDataset` in the Rust extension.
    """
    import polars as pl

    suffix = Path(path).suffix.lower()
    match suffix:
        case ".csv":
            df = pl.read_csv(path, **kwargs)
        case ".parquet":
            df = pl.read_parquet(path, **kwargs)
        case ".json":
            df = pl.read_json(path, **kwargs)
        case _:
            raise ValueError(f"Unsupported file format: {suffix}")

    batches = df.to_arrow().to_batches()
    return pa.concat_batches(batches)
