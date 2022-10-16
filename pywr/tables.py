from pathlib import Path
from typing import Optional, Union, List, Dict, Tuple
import pandas

from pydantic import BaseModel


def _load_dataframe(
    url: Path,
    path: Path,
    index_col: Optional[Union[int, List[int]]] = None,
    header: Optional[Union[int, List[int]]] = None,
    column: Optional[str] = None,
    index: Optional[str] = None,
) -> pandas.Series:
    if not url.is_absolute():
        url = path / url

    pandas_kwargs = {}
    if index_col is not None:
        pandas_kwargs["index_col"] = index_col
    if header is not None:
        pandas_kwargs["header"] = header

    df = pandas.read_csv(url, **pandas_kwargs)

    if column is not None:
        df = df[column]
    if index is not None:
        df = df.loc[index]
    return df


class Table(BaseModel):
    name: str
    index_col: Optional[Union[int, List[int]]] = None
    header: Optional[Union[int, List[int]]] = None
    column: Optional[str]
    index: Optional[str]
    url: str

    def get_values(
        self,
        path: Path,
        index=Optional[Union[str, Tuple[str, ...]]],
        column=Optional[Union[str, Tuple[str, ...]]],
    ):
        df = _load_dataframe(
            Path(self.url),
            path,
            index_col=self.index_col,
            header=self.header,
            column=self.column,
            index=self.index,
        )
        if column is not None:
            df = df[column]
        if index is not None:
            df = df.loc[index]
        return df


class TableRef(BaseModel):
    url: Optional[str]
    index_col: Optional[Union[int, List[int]]] = None
    header: Optional[Union[int, List[int]]] = None
    table: Optional[str]
    index: Optional[Union[str, Tuple[str, ...]]]
    column: Optional[Union[str, Tuple[str, ...]]]


class TableCollection:
    def __init__(self):
        self._tables: Dict[str, Table] = {}

    def __getitem__(self, item: str):
        return self._tables[item]

    def __setitem__(self, key: str, value: Table):
        self._tables[key] = value

    def __iter__(self):
        return iter(self._tables.values())

    def __len__(self):
        return len(self._tables)

    def __contains__(self, item):
        return item in self._tables

    @classmethod
    def __get_validators__(cls):
        yield cls.validate

    @classmethod
    def validate(cls, data):
        if not isinstance(data, list):
            raise TypeError("list required")

        collection = cls()
        for table_data in data:
            table = Table(**table_data)
            if table.name in collection:
                raise ValueError(f"Table name {table.name} already defined.")
            collection[table.name] = table
        return collection

    def get_table_by_name(self, name: str) -> Table:
        return self._tables[name]

    def get_value(self, ref: TableRef, path: Path) -> float:
        # TODO actually look-up a value
        print(ref)
        if ref.table is not None:
            table = self.get_table_by_name(ref.table)
            values = table.get_values(path, index=ref.index, column=ref.column)
        elif ref.url is not None:
            df = _load_dataframe(
                Path(ref.url),
                path,
                index_col=ref.index_col,
                header=ref.header,
                column=ref.column,
                index=ref.index,
            )
            values = df.values
        else:
            raise ValueError("Reference needs either `url` or `table` defining.")
        return float(values)

    def get_values(self, ref: TableRef, path: Path) -> List[float]:
        # TODO actually look-up a value
        print(ref)
        if ref.table is not None:
            table = self.get_table_by_name(ref.table)
            values = table.get_values(path, index=ref.index, column=ref.column)
        elif ref.url is not None:
            url = Path(ref.url)
            if not url.is_absolute():
                url = path / url

            suffix = url.suffix.lower()
            if suffix == ".h5":
                df = pandas.read_hdf(url)
            elif suffix == ".csv":

                df = pandas.read_csv(url, index_col=ref.index_col)
            else:
                raise NotImplementedError(f'File suffix "{suffix}" not supported.')
            if ref.column is not None:
                df = df[ref.column]
            if ref.index is not None:
                df = df.loc[ref.index, :]
            values = df.values
        else:
            raise ValueError("Reference needs either `url` or `table` defining.")
        return values
