# Time Series Data

Time series provide values that vary over a model's time domain. Define input datasets in
`network.time_series`, then reference them from a parameter, node attribute, or any other field
that accepts a metric. A dataset has a unique `meta.name`, a `type`, and (except for a
placeholder) a `path`.

For example, the following CSV has an ISO 8601 date column and one data column:

```csv,ignore
date,inflow
2021-01-01,12.5
2021-01-02,11.2
2021-01-03,10.8
```

Define it with the native Arrow provider:

```json,ignore
{
  "meta": {"name": "inflow-data"},
  "type": "Arrow",
  "path": "inflow.csv",
  "time_col": "date"
}
```

Then reference the `inflow` column where a metric is expected:

```json,ignore
{
  "type": "TimeSeries",
  "name": "inflow-data",
  "columns": {
    "type": "Column",
    "name": "inflow"
  }
}
```

Relative paths are resolved against the model's data path. Absolute paths are used as given.
All path-based providers can also specify a `checksum` to verify input data before it is loaded:

```json,ignore
"checksum": {
  "type": "SHA256",
  "hash": "<sha256 digest>"
}
```

## Time columns and alignment

Set `time_col` to the name of the column containing timestamps. If it is omitted, Pywr only
infers a time column when the first column has an Arrow temporal type. Specifying it explicitly
is recommended.

The time column is not a data column. A dataset with a time column and one data column can be
referenced directly. For a dataset with multiple data columns, select a named column as above,
or use its columns as scenario values:

```json,ignore
{
  "type": "TimeSeries",
  "name": "inflow-data",
  "columns": {
    "type": "Scenario",
    "name": "inflow-scenarios"
  }
}
```

In the scenario form, each non-time-series column supplies values for a scenario in the named
scenario group.

Pywr checks that the time values exactly contain the model time domain as a contiguous sequence.
It can select the matching portion of a longer input series, but it does **not** resample,
interpolate, aggregate, fill gaps, or otherwise change the input time resolution. Time values
must be non-null and must not be repeated. Ensure that the data has already been prepared at the
same timestep frequency and timestamps as the model.

> **Note**: This differs from Pywr v1.x, which automatically resampled data while loading it.
> Resampling is now the responsibility of the model author or their data-preparation workflow.

## Available providers and formats

The provider `type` values and the optional Arrow `format` values are case-sensitive.

### Arrow

`"type": "Arrow"` is the native Rust loader and does not require Python. It supports:

| Format         | `format` value | Recognised extension when `format` is omitted |
|----------------|----------------|-----------------------------------------------|
| CSV            | `"CSV"`        | `.csv`                                        |
| Arrow IPC file | `"IPC"`        | `.ipc` or `.arrow`                            |

Arrow CSV files must include a header row. Use ISO 8601 values for dates and timestamps so that
the Arrow CSV reader can infer temporal columns, for example `2021-01-01` or
`2021-01-01T00:00:00`. Set `format` when the filename has a non-standard extension.

```json,ignore
{
  "meta": {"name": "hourly-inflow"},
  "type": "Arrow",
  "path": "inflow.ipc",
  "format": "IPC",
  "time_col": "timestamp"
}
```

### Parquet

`"type": "Parquet"` is also a native Rust loader and does not require Python. It reads Apache
Parquet files through the Arrow/Parquet reader. There is no `format` field because the provider
always reads Parquet.

```json,ignore
{
  "meta": {"name": "inflow-data"},
  "type": "Parquet",
  "path": "inflow.parquet",
  "time_col": "date"
}
```

### Pandas

`"type": "Pandas"` uses a callback to the Python environment. It requires a Pywr build with
Python support, plus the Python packages `pandas` and `pyarrow` in the Python environment used
to run Pywr. The built-in loader supports `.csv`, `.xlsx`, and `.h5` files, using the appropriate
Pandas reader. `kwargs` are passed to that reader. Pywr supplies `"parse_dates": true` unless
you provide `parse_dates` yourself.

```json,ignore
{
  "meta": {"name": "inflow-data"},
  "type": "Pandas",
  "path": "inflow.xlsx",
  "time_col": "date",
  "kwargs": {"sheet_name": "inflow"}
}
```

### Polars

`"type": "Polars"` also uses the Python environment. It requires a Python-enabled Pywr build,
and the Python packages `polars` and `pyarrow`. The built-in loader supports `.csv`, `.parquet`,
and `.json` files. Its `kwargs` are forwarded to the selected Polars reader. Pywr supplies
`"try_parse_dates": true` unless it is explicitly provided.

```json,ignore
{
  "meta": {"name": "inflow-data"},
  "type": "Polars",
  "path": "inflow.json",
  "time_col": "date"
}
```

### Custom Python loader

`"type": "Python"` invokes a function you provide. It requires a Python-enabled Pywr build and
the `pyarrow` package. The function receives the resolved path and `time_col` as positional
arguments, receives `kwargs` as keyword arguments, and must return a PyArrow `RecordBatch`.
This is useful for formats or preprocessing that are not covered by the built-in providers.

```json,ignore
{
  "meta": {"name": "inflow-data"},
  "type": "Python",
  "module": "my_project.time_series",
  "function": "load_inflow",
  "path": "inflow.custom",
  "time_col": "date",
  "kwargs": {"source_timezone": "UTC"}
}
```

### Placeholder

`"type": "Placeholder"` reserves a time-series name for model composition. It has no path and
cannot load data by itself; replace it with a concrete provider when merging models.
