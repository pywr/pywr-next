from pathlib import Path
from typing import Optional, Union, List, Dict
import pandas

from pydantic import BaseModel


class Table(BaseModel):
    name: str
    index_col: Optional[Union[int, List[int]]] = None
    column: Optional[str]
    index: Optional[str]
    url: str

    def _load_dataframe(self, path: Path) -> pandas.Series:
        url = Path(self.url)
        if not url.is_absolute():
            url = path / url
        df = pandas.read_csv(url, index_col=self.index_col)

        if self.column is not None:
            df = df[self.column]
        if self.index is not None:
            df = df.loc[self.index]
        return df


class TableRef(BaseModel):
    table: str
    index: Optional[str]
    column: Optional[str]


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
