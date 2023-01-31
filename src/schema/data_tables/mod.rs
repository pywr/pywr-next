use crate::schema::parameters::TableIndex;
use log::{debug, info};
use pywr_schema::parameters::TableDataRef as TableDataRefV1;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use thiserror::Error;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum DataTableType {
    Scalar,
    Array,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub enum DataTableFormat {
    CSV,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(tag = "format", rename_all = "lowercase")]
pub enum DataTable {
    CSV(CsvDataTable),
}

impl DataTable {
    pub fn name(&self) -> &str {
        match self {
            DataTable::CSV(tbl) => &tbl.name,
        }
    }

    pub fn load(&self, data_path: Option<&Path>) -> Result<LoadedTable, TableError> {
        match self {
            DataTable::CSV(tbl) => tbl.load_f64(data_path),
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum CsvDataTableLookup {
    Row(usize),
    Col(usize),
    Both(usize, usize),
}

/// An external table of data that can be referenced
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct CsvDataTable {
    name: String,
    #[serde(rename = "type")]
    ty: DataTableType,
    lookup: CsvDataTableLookup,
    url: PathBuf,
}

impl CsvDataTable {
    fn load_f64(&self, data_path: Option<&Path>) -> Result<LoadedTable, TableError> {
        match &self.ty {
            DataTableType::Scalar => match self.lookup {
                CsvDataTableLookup::Row(i) => match i {
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
                CsvDataTableLookup::Col(_) => todo!(),
                CsvDataTableLookup::Both(nrows, ncols) => match (nrows, ncols) {
                    (1, 1) => Ok(LoadedTable::FloatScalar(load_csv_row_col_scalar_table_one(
                        &self.url, data_path,
                    )?)),
                    _ => Err(TableError::FormatNotSupported(
                        "CSV row & col scalar table with more than one index is not supported.".to_string(),
                    )),
                },
            },
            DataTableType::Array => match self.lookup {
                CsvDataTableLookup::Row(i) => match i {
                    1 => Ok(LoadedTable::FloatVec(load_csv_row_vec_table_one(&self.url, data_path)?)),
                    2 => Ok(LoadedTable::FloatVec(load_csv_row2_vec_table_one(
                        &self.url, data_path,
                    )?)),
                    _ => Err(TableError::FormatNotSupported(
                        "CSV row array table with more than two index columns is not supported.".to_string(),
                    )),
                },
                CsvDataTableLookup::Col(_) => todo!(),
                CsvDataTableLookup::Both(_, _) => todo!(),
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

/// Load a CSV file with looks for each rows & columns
fn load_csv_row_col_scalar_table_one<T>(
    table_path: &Path,
    data_path: Option<&Path>,
) -> Result<LoadedScalarTable<T>, TableError>
where
    T: FromStr + Copy,
    TableError: From<T::Err>,
{
    let path = make_path(table_path, data_path);

    let file = File::open(path).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| TableError::Csv(e.to_string()))?
        .iter()
        .skip(1)
        .map(|s| s.to_string())
        .collect();

    let tbl: HashMap<(String, String), T> = rdr
        .records()
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let key = record.get(0).ok_or(TableError::KeyParse)?.to_string();

            let values: Vec<T> = record.iter().skip(1).map(|v| v.parse()).collect::<Result<_, _>>()?;

            let values: Vec<((String, String), T)> = values
                .into_iter()
                .zip(&headers)
                .map(|(v, col)| ((key.clone(), col.to_string()), v))
                .collect();

            Ok(values)
        })
        .collect::<Result<Vec<_>, TableError>>()?
        .into_iter()
        .flatten()
        .collect();

    Ok(LoadedScalarTable::Two(tbl))
}

fn load_csv_row_scalar_table_one<T>(
    table_path: &Path,
    data_path: Option<&Path>,
) -> Result<LoadedScalarTable<T>, TableError>
where
    T: FromStr + Copy,
    TableError: From<T::Err>,
{
    let path = make_path(table_path, data_path);

    let file = File::open(path.clone()).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    let tbl: HashMap<String, T> = rdr
        .records()
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let key = record.get(0).ok_or(TableError::KeyParse)?.to_string();

            let values: Vec<T> = record.iter().skip(1).map(|v| v.parse()).collect::<Result<_, _>>()?;

            if values.len() > 1 {
                return Err(TableError::TooManyValues(path.clone()));
            }

            Ok((key, values[0]))
        })
        .collect::<Result<_, TableError>>()?;

    Ok(LoadedScalarTable::One(tbl))
}

fn load_csv_row2_scalar_table_one<T>(
    table_path: &Path,
    data_path: Option<&Path>,
) -> Result<LoadedScalarTable<T>, TableError>
where
    T: FromStr + Copy,
    TableError: From<T::Err>,
{
    let path = make_path(table_path, data_path);

    let file = File::open(path.clone()).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    let tbl: HashMap<(String, String), T> = rdr
        .records()
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let key = (
                record.get(0).ok_or(TableError::KeyParse)?.to_string(),
                record.get(1).ok_or(TableError::KeyParse)?.to_string(),
            );

            let values: Vec<T> = record.iter().skip(2).map(|v| v.parse()).collect::<Result<_, _>>()?;

            if values.len() > 1 {
                return Err(TableError::TooManyValues(path.clone()));
            }

            Ok((key, values[0]))
        })
        .collect::<Result<_, TableError>>()?;

    Ok(LoadedScalarTable::Two(tbl))
}

fn load_csv_row_vec_table_one<T>(table_path: &Path, data_path: Option<&Path>) -> Result<LoadedVecTable<T>, TableError>
where
    T: FromStr,
    TableError: From<T::Err>,
{
    let path = make_path(table_path, data_path);

    let file = File::open(path).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    let tbl: HashMap<String, Vec<T>> = rdr
        .records()
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let key = record.get(0).ok_or(TableError::KeyParse)?.to_string();

            let values: Vec<T> = record.iter().skip(1).map(|v| v.parse()).collect::<Result<_, _>>()?;

            Ok((key, values))
        })
        .collect::<Result<_, TableError>>()?;

    Ok(LoadedVecTable::One(tbl))
}

fn load_csv_row2_vec_table_one<T>(table_path: &Path, data_path: Option<&Path>) -> Result<LoadedVecTable<T>, TableError>
where
    T: FromStr,
    TableError: From<T::Err>,
{
    let path = make_path(table_path, data_path);

    let file = File::open(path).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    let tbl: HashMap<(String, String), Vec<T>> = rdr
        .records()
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let key = (
                record.get(0).ok_or(TableError::KeyParse)?.to_string(),
                record.get(1).ok_or(TableError::KeyParse)?.to_string(),
            );

            let values: Vec<T> = record.iter().skip(2).map(|v| v.parse()).collect::<Result<_, _>>()?;

            Ok((key, values))
        })
        .collect::<Result<_, TableError>>()?;

    Ok(LoadedVecTable::Two(tbl))
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum TableError {
    #[error("table not found: {0}")]
    TableNotFound(String),
    #[error("entry not found")]
    EntryNotFound,
    #[error("wrong key size; expected: {0}; given: {0}")]
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
}

pub enum LoadedTable {
    FloatVec(LoadedVecTable<f64>),
    FloatScalar(LoadedScalarTable<f64>),
}

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

pub enum LoadedScalarTable<T> {
    One(HashMap<String, T>),
    Two(HashMap<(String, String), T>),
    Three(HashMap<(String, String, String), T>),
}

impl<T> LoadedScalarTable<T>
where
    T: Copy,
{
    fn get_scalar(&self, key: &[&str]) -> Result<T, TableError> {
        match self {
            LoadedScalarTable::One(tbl) => {
                if key.len() == 1 {
                    tbl.get(key[0]).ok_or(TableError::EntryNotFound).copied()
                } else {
                    Err(TableError::WrongKeySize(1, key.len()))
                }
            }
            LoadedScalarTable::Two(tbl) => {
                if key.len() == 2 {
                    // I think this copies the strings and is not very efficient.
                    let k = (key[0].to_string(), key[1].to_string());
                    tbl.get(&k).ok_or(TableError::EntryNotFound).copied()
                } else {
                    Err(TableError::WrongKeySize(2, key.len()))
                }
            }
            LoadedScalarTable::Three(tbl) => {
                if key.len() == 3 {
                    // I think this copies the strings and is not very efficient.
                    let k = (key[0].to_string(), key[1].to_string(), key[2].to_string());
                    tbl.get(&k).ok_or(TableError::EntryNotFound).copied()
                } else {
                    Err(TableError::WrongKeySize(3, key.len()))
                }
            }
        }
    }
}

pub enum LoadedVecTable<T> {
    One(HashMap<String, Vec<T>>),
    Two(HashMap<(String, String), Vec<T>>),
    Three(HashMap<(String, String, String), Vec<T>>),
}

impl<T> LoadedVecTable<T>
where
    T: Copy,
{
    fn get_vec(&self, key: &[&str]) -> Result<&Vec<T>, TableError> {
        match self {
            LoadedVecTable::One(tbl) => {
                if key.len() == 1 {
                    tbl.get(key[0]).ok_or(TableError::EntryNotFound)
                } else {
                    Err(TableError::WrongKeySize(1, key.len()))
                }
            }
            LoadedVecTable::Two(tbl) => {
                if key.len() == 2 {
                    // I think this copies the strings and is not very efficient.
                    let k = (key[0].to_string(), key[1].to_string());
                    tbl.get(&k).ok_or(TableError::EntryNotFound)
                } else {
                    Err(TableError::WrongKeySize(2, key.len()))
                }
            }
            LoadedVecTable::Three(tbl) => {
                if key.len() == 3 {
                    // I think this copies the strings and is not very efficient.
                    let k = (key[0].to_string(), key[1].to_string(), key[2].to_string());
                    tbl.get(&k).ok_or(TableError::EntryNotFound)
                } else {
                    Err(TableError::WrongKeySize(3, key.len()))
                }
            }
        }
    }
}

pub struct LoadedTableCollection {
    tables: HashMap<String, LoadedTable>,
}

impl LoadedTableCollection {
    pub fn from_schema(table_defs: Option<&[DataTable]>, data_path: Option<&Path>) -> Result<Self, TableError> {
        let mut tables = HashMap::new();
        if let Some(table_defs) = table_defs {
            for table_def in table_defs {
                let name = table_def.name().to_string();
                info!("Loading table: {}", &name);
                let table = table_def.load(data_path)?;
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
        debug!("Looking-up float scalar with reference: {:?}", table_ref);
        let tbl = self.get_table(&table_ref.table)?;
        let key = table_ref.key();
        tbl.get_scalar_f64(&key)
    }

    /// Return a single scalar value from a table collection.
    pub fn get_scalar_usize(&self, _table_ref: &TableDataRef) -> Result<usize, TableError> {
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

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct TableDataRef {
    table: String,
    column: Option<TableIndex>,
    index: Option<TableIndex>,
}

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

impl From<TableDataRefV1> for TableDataRef {
    fn from(v1: TableDataRefV1) -> Self {
        Self {
            table: v1.table,
            column: v1.column.map(|i| i.into()),
            index: v1.index.map(|i| i.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_dataframe_row_filter() {
        let dir = tempdir().unwrap();

        let table_def = format!(
            r#"
            {{
                "name": "my-arrays",
                "type": "array",
                "format": "csv",
                "lookup": {{"row": 1}},
                "url": "{}/my-data.csv"
            }}"#,
            dir.as_ref().display()
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
