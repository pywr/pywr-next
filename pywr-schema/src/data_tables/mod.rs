#[cfg(feature = "core")]
mod scalar;
#[cfg(feature = "core")]
mod vec;

use crate::ConversionError;
#[cfg(feature = "core")]
use crate::SchemaError;
use crate::parameters::TableIndex;
use pywr_schema_macros::PywrVisitAll;
use pywr_v1_schema::parameters::TableDataRef as TableDataRefV1;
#[cfg(feature = "core")]
use scalar::{
    LoadedScalarTable, load_csv_row_col_scalar_table_one, load_csv_row_scalar_table_one, load_csv_row2_scalar_table_one,
};
use schemars::JsonSchema;
#[cfg(feature = "core")]
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
use thiserror::Error;
#[cfg(feature = "core")]
use tracing::{debug, info};
#[cfg(feature = "core")]
use vec::{
    LoadedVecTable, load_csv_col1_vec_table_one, load_csv_col2_vec_table_two, load_csv_row_vec_table_one,
    load_csv_row2_vec_table_one,
};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, Display, EnumIter)]
pub enum DataTableValueType {
    Scalar,
    Array,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Display, EnumIter)]
pub enum DataTableFormat {
    CSV,
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
        match self {
            DataTable::CSV(tbl) => &tbl.name,
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
    Row { rows: usize },
    Col { cols: usize },
    Both { rows: usize, cols: usize },
}

/// An external table of data that can be referenced
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema)]
pub struct CsvDataTable {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: DataTableValueType,
    pub lookup: CsvDataTableLookup,
    pub url: PathBuf,
}

#[cfg(feature = "core")]
impl CsvDataTable {
    fn load_f64(&self, data_path: Option<&Path>) -> Result<LoadedTable, TableError> {
        match &self.ty {
            DataTableValueType::Scalar => match self.lookup {
                CsvDataTableLookup::Row { rows } => match rows {
                    1 => Ok(LoadedTable::FloatScalar(load_csv_row_scalar_table_one(
                        &self.url, data_path,
                    )?)),
                    2 => Ok(LoadedTable::FloatScalar(load_csv_row2_scalar_table_one(
                        &self.url, data_path,
                    )?)),
                    _ => Err(TableError::FormatNotSupported(
                        "CSV row scalar table with more than two index columns is not supported.".to_string(),
                    )),
                },
                CsvDataTableLookup::Col { .. } => todo!(),
                CsvDataTableLookup::Both { rows, cols } => match (rows, cols) {
                    (1, 1) => Ok(LoadedTable::FloatScalar(load_csv_row_col_scalar_table_one(
                        &self.url, data_path,
                    )?)),
                    _ => Err(TableError::FormatNotSupported(
                        "CSV row & col scalar table with more than one index is not supported.".to_string(),
                    )),
                },
            },
            DataTableValueType::Array => match self.lookup {
                CsvDataTableLookup::Row { rows } => match rows {
                    1 => Ok(LoadedTable::FloatVec(load_csv_row_vec_table_one(&self.url, data_path)?)),
                    2 => Ok(LoadedTable::FloatVec(load_csv_row2_vec_table_one(
                        &self.url, data_path,
                    )?)),
                    _ => Err(TableError::FormatNotSupported(
                        "CSV row array table with more than two index columns is not supported.".to_string(),
                    )),
                },
                CsvDataTableLookup::Col { cols } => match cols {
                    1 => Ok(LoadedTable::FloatVec(load_csv_col1_vec_table_one(
                        &self.url, data_path,
                    )?)),
                    2 => Ok(LoadedTable::FloatVec(load_csv_col2_vec_table_two(
                        &self.url, data_path,
                    )?)),
                    _ => Err(TableError::FormatNotSupported(
                        "CSV column array table with more than two index columns is not supported.".to_string(),
                    )),
                },
                CsvDataTableLookup::Both { .. } => todo!(),
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

#[derive(Error, Debug, PartialEq, Eq)]
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
    #[error("wrong table format: {0}")]
    WrongTableFormat(String),
    #[error("too many values for scalar table when loading data table from: {0}")]
    TooManyValues(PathBuf),
    #[error("table index out of bounds: {0}")]
    IndexOutOfBounds(usize),
    #[error("Table format invalid: {0}")]
    InvalidFormat(String),
}

#[cfg(feature = "core")]
pub enum LoadedTable {
    FloatVec(LoadedVecTable<f64>),
    FloatScalar(LoadedScalarTable<f64>),
}

#[cfg(feature = "core")]
impl LoadedTable {
    pub fn get_vec_f64(&self, key: &[&str]) -> Result<&Vec<f64>, TableError> {
        match self {
            LoadedTable::FloatVec(tbl) => tbl.get_vec(key),
            _ => Err(TableError::WrongTableFormat(
                "Array of values requested from non-array table.".to_string(),
            )),
        }
    }

    pub fn get_scalar_f64(&self, key: &[&str]) -> Result<f64, TableError> {
        match self {
            LoadedTable::FloatScalar(tbl) => tbl.get_scalar(key),
            _ => Err(TableError::WrongTableFormat(format!(
                "Scalar value with key \"{key:?}\" requested from non-scalar table."
            ))),
        }
    }
}

#[cfg(feature = "core")]
pub struct LoadedTableCollection {
    tables: HashMap<String, LoadedTable>,
}

#[cfg(feature = "core")]
impl LoadedTableCollection {
    pub fn from_schema(table_defs: Option<&[DataTable]>, data_path: Option<&Path>) -> Result<Self, SchemaError> {
        let mut tables = HashMap::new();
        if let Some(table_defs) = table_defs {
            for table_def in table_defs {
                let name = table_def.name().to_string();
                info!("Loading table: {}", &name);
                let table = table_def.load(data_path).map_err(|error| SchemaError::TableLoad {
                    table_def: table_def.clone(),
                    error,
                })?;
                // TODO handle duplicate table names!
                tables.insert(name, table);
            }
        }

        Ok(LoadedTableCollection { tables })
    }

    pub fn get_table(&self, name: &str) -> Result<&LoadedTable, TableError> {
        self.tables
            .get(name)
            .ok_or_else(|| TableError::TableNotFound(name.to_string()))
    }

    /// Return a single scalar value from a table collection.
    pub fn get_scalar_f64(&self, table_ref: &TableDataRef) -> Result<f64, TableError> {
        let tbl = self.get_table(&table_ref.table)?;
        let key = table_ref.key();
        tbl.get_scalar_f64(&key)
    }

    /// Return a single scalar value from a table collection.
    pub fn get_scalar_u64(&self, _table_ref: &TableDataRef) -> Result<u64, TableError> {
        // let tbl = self.get_table(&table_ref.table)?;
        todo!()
    }

    /// Return a single scalar value from a table collection.
    pub fn get_vec_f64(&self, table_ref: &TableDataRef) -> Result<&Vec<f64>, TableError> {
        debug!("Looking-up float array with reference: {:?}", table_ref);
        let tbl = self.get_table(&table_ref.table)?;
        let key = table_ref.key();
        tbl.get_vec_f64(&key)
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TableDataRef {
    pub table: String,
    pub column: Option<TableIndex>,
    pub index: Option<TableIndex>,
}

#[cfg(feature = "core")]
impl TableDataRef {
    fn key(&self) -> Vec<&str> {
        let mut key: Vec<&str> = Vec::new();
        if let Some(row_idx) = &self.index {
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
            index,
        })
    }
}

#[cfg(test)]
#[cfg(feature = "core")]
mod tests {
    use super::*;
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
                "name": "my-arrays",
                "type": "Array",
                "format": "CSV",
                "lookup": {{
                    "type": "Row",
                    "rows": 1
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

        let values: &Vec<f64> = tbl.get_vec_f64(&["my-reservoir"]).unwrap();

        assert_eq!(
            values,
            &vec![0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2, 0.2]
        );
    }
}
