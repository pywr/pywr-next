from pathlib import Path
from typing import Union
import pandas as pd
import polars as pl


def load_pandas(path: str, index_col: Union[str, int], **kwargs) -> pl.DataFrame:
    """Helper function to load a pandas DataFrame from a file, and then convert it to a polars DataFrame.

    This function is used by the `load` function of the `PandasDataset` in the Rust extension.
    """
    suffix = Path(path).suffix.lower()
    match suffix:
        case ".csv":
            df = pd.read_csv(path, index_col=index_col, parse_dates=True, **kwargs)
        case ".xlsx":
            df = pd.read_excel(path, index_col=index_col, **kwargs)
        case ".h5":
            df = pd.read_hdf(path, **kwargs)
        case _:
            raise ValueError(f"Unsupported file format: {suffix}")
    return pl.from_pandas(df, include_index=True)
