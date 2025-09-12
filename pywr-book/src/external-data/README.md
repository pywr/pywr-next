# External data

Providing data to your Pywr model is essential. While some information can be encoded as constants or other values in
the JSON, most real-world models require external data, such as time series or lookup tables. Pywr supports loading data
from CSV files using data tables, which can provide both scalar and array values to parameters and nodes. Data tables
allow flexible lookup schemes, including row-based, column-based, and combined row/column indexing.


