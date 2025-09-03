# Data tables

Providing data to your Pywr model is essential. While some information can be encoded as constants or other values in
the JSON, most real-world models require external data, such as time series or lookup tables. Pywr supports loading data
from CSV files using data tables, which can provide both scalar and array values to parameters and nodes. Data tables
allow flexible lookup schemes, including row-based, column-based, and combined row/column indexing.

## Scalar Data Tables

Scalar data tables provide single constant values indexed by rows and/or columns. Using a data table might allow
you to avoid hardcoding values in your model JSON, making it easier to update and manage. For example, you might have a
data table that provides asset capacities, and separate table for asset costs. By swapping out the CSV files, you can
easily change the model's parameters without modifying the JSON. However, this can make the model less transparent, as
the values are not directly visible in the JSON.

### Row-based Scalar Data Tables

Row-based scalar data tables use the row index to look up values. This is useful when you have a list of assets or
parameters, and you want to assign a specific value to each one. For example, consider the following CSV file, "
tbl-scalar-row.csv":

[//]: # (@formatter:off)

```csv,ignore
{{ #include ../../pywr-schema/tests/tbl-scalar-row.csv }}
```

[//]: # (@formatter:on)

This table has two columns: `key` and `value`. The `key` column contains the row index, which can be any string or
number. The `value` column contains the corresponding value for each key. To use this table in your model, you would
define a `table` in your JSON, and then reference it in a parameter, node, etc. For example, to load the above
table define the following in your model JSON:

[//]: # (@formatter:off)

```json,ignore
{{ #include ../../pywr-schema/tests/tbl-formats1.json:58:67 }}
```

[//]: # (@formatter:on)

The JSON snippet above defines a table named `scalar-row` that loads data from the CSV file. It specifies that the
table contains a single row index, and that the table is expected to return a single scalar value. The actual header
values in the CSV file are not important, as long as the first column is used for the row index and the second column
contains the values. The table assumes that the first row contains the header, and the data starts from the second row.

Once the table is defined, you can reference it in a parameter. For example, to use the `scalar-row` table to provide
a value for a `ConstantParameter`, you would reference it for the `value` field. A table reference like this can be
used anywhere a `Metric` or `ConstantValue` is expected.

[//]: # (@formatter:off)

```json,ignore
{{ #include ../../pywr-schema/tests/tbl-formats1.json:121:131 }}
```

[//]: # (@formatter:on)

It can be useful to organise the data in a table with multiple keys. For example, you might have a table that provides
different data for different assets. In this case, you can use one key for the asset and one key for the data type.
For example, consider the following CSV file, "tbl-scalar-row-row.csv":

[//]: # (@formatter:off)

```csv,ignore
{{ #include ../../pywr-schema/tests/tbl-scalar-row-row.csv }}
```

[//]: # (@formatter:on)

### Row & column-based Scalar Data Tables

Row & column-based scalar data tables use both row and column indices to look up values. This is useful when you have
a matrix of values, and you want to assign a specific value to each combination of row and column. For example, consider
the following CSV file, "tbl-scalar-row-col.csv":

[//]: # (@formatter:off)

```csv,ignore
{{ #include ../../pywr-schema/tests/tbl-scalar-row-col.csv }}
```

[//]: # (@formatter:on)

To use this table in your model, you would define a `table` in your JSON, and then reference it in a parameter, node,
etc. For example, to load the above table define the following in your model JSON:

[//]: # (@formatter:off)

```json,ignore
{{ #include ../../pywr-schema/tests/tbl-formats1.json:78:88 }}
```

[//]: # (@formatter:on)

When referencing a row & column-based table, you need to provide both the row and column keys. For example, to use the
`scalar-row-col` table to provide a value for a `ConstantParameter`, you would reference it for the `value` field, and
provide both
the row and column keys.

> **Note**: This example uses an emoji (🐍) as a column key. While this is valid, it may cause issues with some software
> and libraries, and must be encoded correctly in the JSON (as shown).


[//]: # (@formatter:off)

```json,ignore
{{ #include ../../pywr-schema/tests/tbl-formats1.json:146:157 }}
```

[//]: # (@formatter:on)

## Array Data Tables

Array data tables provide array values indexed by rows *or* columns. This is useful for certain types of parameters,
such as monthly or daily profiles, which require an array of values. The following example shows how to define
an array data table in CSV format with a single row index.

[//]: # (@formatter:off)

```csv,ignore
{{ #include ../../pywr-schema/tests/tbl-array-row.csv }}
```

[//]: # (@formatter:on)

To use this table in your model use `"type": "Array"` in the table definition in your JSON, as shown below.

[//]: # (@formatter:off)

```json,ignore
{{ #include ../../pywr-schema/tests/tbl-formats1.json:89:98 }}
```

[//]: # (@formatter:on)


The same data can be formatted with a column index instead of a row index, as shown below.

[//]: # (@formatter:off)

```csv,ignore
{{ #include ../../pywr-schema/tests/tbl-array-col.csv }}
```

[//]: # (@formatter:on)

And the corresponding table definition in JSON:

[//]: # (@formatter:off)

```json,ignore
{{ #include ../../pywr-schema/tests/tbl-formats1.json:109:118 }}
```

[//]: # (@formatter:on)