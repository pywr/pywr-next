//! # Data Tables in Pywr
//!
//! Data tables provide a flexible mechanism for loading external data into Pywr models.
//! They are used to supply scalar or array values to parameters and nodes, typically from CSV files.
//! Data tables support different lookup formats, such as row-based, column-based, or both,
//! allowing for a variety of indexing schemes.
//!
//! The main supported formats are:
//! - **CSV**: The most common format, supporting both scalar and array data.
//!     - **Row-based lookup**: Index by one or more row keys.
//!     - **Column-based lookup**: Index by one or more column keys (for arrays).
//!     - **Row & column lookup**: Index by both row and column keys (for scalars).
//!
//! For more details and advanced usage, see the [Pywr Book](https://pywr.org/book/external_data.html).

#[cfg(feature = "core")]
mod scalar;
#[cfg(feature = "core")]
mod vec;

use crate::ConversionError;
use crate::digest::{Checksum, ChecksumError};
use crate::parameters::TableIndex;
#[cfg(feature = "pyo3")]
use pyo3::pyclass;
use pywr_schema_macros::{PywrVisitAll, skip_serializing_none};
use pywr_v1_schema::parameters::TableDataRef as TableDataRefV1;
#[cfg(feature = "core")]
use scalar::LoadedScalarTable;
use schemars::JsonSchema;
#[cfg(feature = "core")]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
use thiserror::Error;
#[cfg(feature = "core")]
use tracing::{debug, info};
#[cfg(feature = "core")]
use vec::LoadedVecTable;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, Display, EnumIter)]
pub enum DataTableValueType {
    Scalar,
    Array,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default, JsonSchema, PywrVisitAll)]
pub struct TableMeta {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, Display, EnumDiscriminants)]
#[serde(tag = "format")]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(DataTableType))]
pub enum DataTable {
    CSV(CsvDataTable),
}

impl DataTable {
    pub fn name(&self) -> &str {
        self.meta().name.as_str()
    }

    pub fn meta(&self) -> &TableMeta {
        match self {
            DataTable::CSV(tbl) => &tbl.meta,
        }
    }

    #[cfg(feature = "core")]
    pub fn load(&self, data_path: Option<&Path>) -> Result<LoadedTable, TableError> {
        match self {
            DataTable::CSV(tbl) => tbl.load_f64(data_path),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, Display, EnumDiscriminants)]
#[serde(tag = "type", deny_unknown_fields)]
#[strum_discriminants(derive(Display, IntoStaticStr, EnumString, EnumIter))]
#[strum_discriminants(name(CsvDataTableLookupType))]
pub enum CsvDataTableLookup {
    Row { cols: usize },
    Col { rows: usize },
    Both { rows: usize, cols: usize },
}

/// An external table of data that can be referenced
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
pub struct CsvDataTable {
    pub meta: TableMeta,
    #[serde(rename = "type")]
    pub ty: DataTableValueType,
    pub lookup: CsvDataTableLookup,
    pub url: PathBuf,
    pub checksum: Option<Checksum>,
}

#[cfg(feature = "core")]
impl CsvDataTable {
    fn load_f64(&self, data_path: Option<&Path>) -> Result<LoadedTable, TableError> {
        let fp = make_path(&self.url, data_path);

        if let Some(checksum) = &self.checksum {
            checksum.check(&fp)?;
        }

        match &self.ty {
            DataTableValueType::Scalar => match self.lookup {
                CsvDataTableLookup::Row { cols: rows } => {
                    Ok(LoadedTable::FloatScalar(LoadedScalarTable::from_csv_row(&fp, rows)?))
                }
                CsvDataTableLookup::Col { rows: cols } => {
                    Ok(LoadedTable::FloatScalar(LoadedScalarTable::from_csv_col(&fp, cols)?))
                }
                CsvDataTableLookup::Both { rows, cols } => Ok(LoadedTable::FloatScalar(
                    LoadedScalarTable::from_csv_row_col(&fp, rows, cols)?,
                )),
            },
            DataTableValueType::Array => match self.lookup {
                CsvDataTableLookup::Row { cols: rows } => {
                    Ok(LoadedTable::FloatVec(LoadedVecTable::from_csv_row(&fp, rows)?))
                }
                CsvDataTableLookup::Col { rows: cols } => {
                    Ok(LoadedTable::FloatVec(LoadedVecTable::from_csv_col(&fp, cols)?))
                }
                CsvDataTableLookup::Both { .. } => Err(TableError::FormatNotSupported(
                    "CSV row & column array table is not supported. Use either row or column based format.".to_string(),
                )),
            },
        }
    }
}

/// Make a finalised path for reading data from.
///
/// If `table_path` is relative and `data_path` is some path then join `table_path` to `data_path`.
/// Otherwise just return `table_path`.
// TODO make this a general utility function
pub fn make_path(table_path: &Path, data_path: Option<&Path>) -> PathBuf {
    if table_path.is_relative() {
        if let Some(dp) = data_path {
            dp.join(table_path)
        } else {
            table_path.to_path_buf()
        }
    } else {
        table_path.to_path_buf()
    }
}

#[derive(Error, Debug)]
pub enum TableLoadError {}

#[derive(Error, Debug)]
pub enum TableError {
    #[error("table not found: {0}")]
    TableNotFound(String),
    #[error("entry not found")]
    EntryNotFound,
    #[error("wrong key size; expected: {0}; given: {1}")]
    WrongKeySize(usize, usize),
    #[error("failed to get or parse key")]
    KeyParse,
    #[error("I/O error: {0}")]
    IO(String),
    #[error("CSV error: {0}")]
    Csv(String),
    #[error("Format not supported: {0}")]
    FormatNotSupported(String),
    #[error("Failed to parse: {0}")]
    ParseFloatError(#[from] std::num::ParseFloatError),
    #[error("Wrong table format: {0}")]
    WrongTableFormat(String),
    #[error(
        "Too many columns for scalar table. Expected {expected} columns (one more than the size of the index), found {found} columns."
    )]
    TooManyColumns { expected: usize, found: usize },
    #[error(
        "Too many rows for scalar table. Expected {expected} rows (one more than the size of the index), found {found} rows."
    )]
    TooManyRows { expected: usize, found: usize },
    #[error("Table index out of bounds: {0}")]
    IndexOutOfBounds(usize),
    #[error("Table format invalid: {0}")]
    InvalidFormat(String),
    #[error("Could not convert to u64 without loss of precision. Index values must be positive whole numbers.")]
    U64ConversionError,
    #[error("Checksum error: {0}")]
    ChecksumError(#[from] ChecksumError),
}

#[cfg(feature = "core")]
pub enum LoadedTable {
    FloatVec(LoadedVecTable<f64>),
    FloatScalar(LoadedScalarTable),
}

#[cfg(feature = "core")]
impl LoadedTable {
    pub fn get_vec_f64(&self, key: &[&str]) -> Result<&[f64], TableError> {
        match self {
            LoadedTable::FloatVec(tbl) => tbl.get_vec(key),
            _ => Err(TableError::WrongTableFormat(
                "Array of values requested from non-array table.".to_string(),
            )),
        }
    }

    pub fn get_scalar_f64(&self, key: &[&str]) -> Result<f64, TableError> {
        match self {
            LoadedTable::FloatScalar(tbl) => Ok(tbl.get_scalar(key)?.as_f64()),
            _ => Err(TableError::WrongTableFormat(format!(
                "Scalar value with key \"{key:?}\" requested from non-scalar table."
            ))),
        }
    }

    pub fn get_scalar_u64(&self, key: &[&str]) -> Result<u64, TableError> {
        match self {
            LoadedTable::FloatScalar(tbl) => Ok(tbl
                .get_scalar(key)?
                .try_as_u64()
                .ok_or(TableError::U64ConversionError)?),
            _ => Err(TableError::WrongTableFormat(format!(
                "Scalar value with key \"{key:?}\" requested from non-scalar table."
            ))),
        }
    }
}

#[cfg(feature = "core")]
#[derive(Error, Debug)]
pub enum TableCollectionLoadError {
    #[error("Failed to load table `{name}`: {source}")]
    TableError {
        name: String,
        #[source]
        source: TableError,
    },
    #[error("Table with name `{name}` already exists in the collection.")]
    DuplicateTableName { name: String },
}

#[cfg(feature = "core")]
#[derive(Error, Debug)]
pub enum TableCollectionError {
    #[error("Failed to get value from table `{name}`: {source}")]
    TableError {
        name: String,
        #[source]
        source: TableError,
    },
    #[error("Table with name `{name}` not found.")]
    TableNotFound { name: String },
}

#[cfg(feature = "core")]
pub struct LoadedTableCollection {
    tables: HashMap<String, LoadedTable>,
}

#[cfg(feature = "core")]
impl LoadedTableCollection {
    pub fn from_schema(
        table_defs: Option<&[DataTable]>,
        data_path: Option<&Path>,
    ) -> Result<Self, TableCollectionLoadError> {
        let mut tables = HashMap::new();
        if let Some(table_defs) = table_defs {
            for table_def in table_defs {
                let name = table_def.name().to_string();
                info!("Loading table: {}", &name);
                let table = table_def
                    .load(data_path)
                    .map_err(|source| TableCollectionLoadError::TableError {
                        name: name.clone(),
                        source,
                    })?;

                if tables.contains_key(&name) {
                    return Err(TableCollectionLoadError::DuplicateTableName { name });
                }

                tables.insert(name, table);
            }
        }

        Ok(LoadedTableCollection { tables })
    }

    pub fn get_table(&self, name: &str) -> Result<&LoadedTable, TableCollectionError> {
        self.tables
            .get(name)
            .ok_or_else(|| TableCollectionError::TableNotFound { name: name.to_string() })
    }

    /// Return a single scalar value from a table collection.
    pub fn get_scalar_f64(&self, table_ref: &TableDataRef) -> Result<f64, TableCollectionError> {
        let tbl = self.get_table(&table_ref.table)?;
        let key = table_ref.key();
        tbl.get_scalar_f64(&key)
            .map_err(|source| TableCollectionError::TableError {
                name: table_ref.table.clone(),
                source,
            })
    }

    /// Return a single scalar value from a table collection.
    pub fn get_scalar_u64(&self, table_ref: &TableDataRef) -> Result<u64, TableCollectionError> {
        let tbl = self.get_table(&table_ref.table)?;
        let key = table_ref.key();
        tbl.get_scalar_u64(&key)
            .map_err(|source| TableCollectionError::TableError {
                name: table_ref.table.clone(),
                source,
            })
    }

    /// Return a single scalar value from a table collection.
    pub fn get_vec_f64(&self, table_ref: &TableDataRef) -> Result<&[f64], TableCollectionError> {
        debug!("Looking-up float array with reference: {:?}", table_ref);
        let tbl = self.get_table(&table_ref.table)?;
        let key = table_ref.key();
        tbl.get_vec_f64(&key)
            .map_err(|source| TableCollectionError::TableError {
                name: table_ref.table.clone(),
                source,
            })
    }
}

#[skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll, PartialEq)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "pyo3", pyclass)]
pub struct TableDataRef {
    pub table: String,
    pub column: Option<TableIndex>,
    pub row: Option<TableIndex>,
}

#[cfg(feature = "core")]
impl TableDataRef {
    pub fn key(&self) -> Vec<&str> {
        let mut key: Vec<&str> = Vec::new();
        if let Some(row_idx) = &self.row {
            match row_idx {
                TableIndex::Single(k) => key.push(k),
                TableIndex::Multi(k) => key.extend(k.iter().map(|s| s.as_str())),
            }
        }
        if let Some(col_idx) = &self.column {
            match col_idx {
                TableIndex::Single(k) => key.push(k),
                TableIndex::Multi(k) => key.extend(k.iter().map(|s| s.as_str())),
            }
        }
        key
    }
}

impl TryFrom<TableDataRefV1> for TableDataRef {
    type Error = ConversionError;
    fn try_from(v1: TableDataRefV1) -> Result<Self, Self::Error> {
        let column = match v1.column {
            None => None,
            Some(c) => Some(c.try_into().map_err(|e| ConversionError::TableRef {
                attr: "column".to_string(),
                name: v1.table.clone(),
                error: e,
            })?),
        };
        let index = match v1.index {
            None => None,
            Some(i) => Some(i.try_into().map_err(|e| ConversionError::TableRef {
                attr: "index".to_string(),
                name: v1.table.clone(),
                error: e,
            })?),
        };
        Ok(Self {
            table: v1.table,
            column,
            row: index,
        })
    }
}

#[cfg(test)]
#[cfg(feature = "core")]
mod tests {
    use super::*;
    use std::fs;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_dataframe_row_filter() {
        let dir = tempdir().unwrap();

        // Temporary file name
        let my_data_fn = dir.path().join("my-data.csv");
        // Serialise using serde to do cross-platform character escaping correctly.
        let my_data_fn = serde_json::to_string(&my_data_fn).unwrap();

        let table_def = format!(
            r#"
            {{
                "meta": {{
                    "name": "my-arrays"
                }},
                "type": "Array",
                "format": "CSV",
                "lookup": {{
                    "type": "Row",
                    "cols": 1
                }},
                "url": {my_data_fn}
            }}"#,
        );

        // Create the temporary data
        {
            let data = r"reservoir,1,2,3,4,5,6,7,8,9,10,11,12
a-reservoir,0.1,0.1,0.1,0.1,0.1,0.1,0.1,0.1,0.1,0.1,0.1,0.1
my-reservoir,0.2,0.2,0.2,0.2,0.2,0.2,0.2,0.2,0.2,0.2,0.2,0.2";
            let file_path = dir.path().join("my-data.csv");
            let mut file = File::create(file_path).unwrap();
            file.write_all(data.as_bytes()).unwrap();
        }

        // Deserialize the representation
        let tbl: DataTable = serde_json::from_str(&table_def).unwrap();
        // Load the table definition
        let tbl = tbl.load(None).unwrap();

        let values: Vec<f64> = tbl.get_vec_f64(&["my-reservoir"]).unwrap().to_vec();

        assert_eq!(values, vec![0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2]);
    }

    /// Test all the documentation examples successfully deserialize.
    #[test]
    fn test_doc_examples() {
        let mut doc_examples = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        doc_examples.push("src/data_tables/doc_examples");

        for entry in fs::read_dir(doc_examples).unwrap() {
            let p = entry.unwrap().path();
            if p.is_file() {
                let data = fs::read_to_string(&p).unwrap_or_else(|_| panic!("Failed to read file: {p:?}",));

                let value: serde_json::Value =
                    serde_json::from_str(&data).unwrap_or_else(|_| panic!("Failed to deserialize: {p:?}",));

                match value {
                    serde_json::Value::Object(_) => {
                        let _ = serde_json::from_value::<DataTable>(value)
                            .unwrap_or_else(|e| panic!("Failed to deserialize `{p:?}`: {e}",));
                    }
                    _ => panic!("Expected JSON object or array: {p:?}",),
                }
            }
        }
    }
}
