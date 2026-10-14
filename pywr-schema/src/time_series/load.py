from pathlib import Path

import pyarrow as pa


def load_pandas(path: str, **kwargs) -> pa.RecordBatch:
    """Helper function to load a pandas DataFrame from a file, and then convert it to a pyarrow RecordBatch.

    This function is used by the `load` function of the `PandasTimeSeries` in the Rust extension.
    """
    import pandas as pd

    suffix = Path(path).suffix.lower()
    match suffix:
        case ".csv":
            if "parse_dates" not in kwargs:
                kwargs["parse_dates"] = True
            df = pd.read_csv(path, **kwargs)
        case ".xlsx":
            if "parse_dates" not in kwargs:
                kwargs["parse_dates"] = True
            df = pd.read_excel(path, **kwargs)
        case ".h5":
            df = pd.read_hdf(path, **kwargs)
        case _:
            raise ValueError(f"Unsupported file format: {suffix}")
    # The index is reset to ensure any time column is at the front of the RecordBatch.
    if not isinstance(df.index, pd.RangeIndex):
        df = df.reset_index()
    return pa.RecordBatch.from_pandas(df)


def load_polars(path: str, **kwargs) -> pa.RecordBatch:
    """Helper function to load a polars DataFrame from a file, and then convert it to a pyarrow RecordBatch.

    This function is used by the `load` function of the `PolarsDataset` in the Rust extension.
    """
    import polars as pl

    suffix = Path(path).suffix.lower()
    match suffix:
        case ".csv":
            if "try_parse_dates" not in kwargs:
                kwargs["try_parse_dates"] = True
            df = pl.read_csv(path, **kwargs)
        case ".parquet":
            df = pl.read_parquet(path, **kwargs)
        case ".json":
            df = pl.read_json(path, **kwargs)
        case _:
            raise ValueError(f"Unsupported file format: {suffix}")

    batches = df.to_arrow().to_batches()
    return pa.concat_batches(batches)
