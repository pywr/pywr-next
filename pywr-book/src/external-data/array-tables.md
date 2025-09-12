# Array Data Tables

Array data tables provide array values indexed by rows *or* columns. This is useful for certain types of parameters,
such as monthly or daily profiles, which require an array of values. The following example shows how to define
an array data table in CSV format with a single row index.

> **Note**: Currently, Pywr supports up to 4 keys for array data tables. This means you can have up to 4 row or column
> indices.


[//]: # (@formatter:off)

```csv,ignore
{{ #include ../../../pywr-schema/tests/tbl-array-row.csv }}
```

[//]: # (@formatter:on)

To use this table in your model use `"type": "Array"` in the table definition in your JSON, as shown below.

[//]: # (@formatter:off)

```json,ignore
{{ #include ../../../pywr-schema/tests/tbl-formats1.json:99:108 }}
```

[//]: # (@formatter:on)


The same data can be formatted with a column index instead of a row index, as shown below.

[//]: # (@formatter:off)

```csv,ignore
{{ #include ../../../pywr-schema/tests/tbl-array-col.csv }}
```

[//]: # (@formatter:on)

And the corresponding table definition in JSON:

[//]: # (@formatter:off)

```json,ignore
{{ #include ../../../pywr-schema/tests/tbl-formats1.json:119:128 }}
```

[//]: # (@formatter:on)
