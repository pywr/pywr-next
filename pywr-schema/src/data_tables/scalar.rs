use crate::data_tables::{TableError, make_path};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::str::FromStr;

/// A value in a table.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum TableScalarValue {
    Float(f64),
    Int(i64),
}

impl TableScalarValue {
    /// Convert the value to f64.
    pub fn as_f64(&self) -> f64 {
        match self {
            TableScalarValue::Float(v) => *v,
            TableScalarValue::Int(v) => *v as f64,
        }
    }

    /// Try to convert the value to u64, returning None if it is not possible.
    pub fn try_as_u64(&self) -> Option<u64> {
        match self {
            TableScalarValue::Float(v) => {
                if *v >= 0.0 && v.fract() == 0.0 {
                    Some(*v as u64)
                } else {
                    None
                }
            }
            TableScalarValue::Int(v) => {
                if *v >= 0 {
                    Some(*v as u64)
                } else {
                    None
                }
            }
        }
    }
}

impl FromStr for TableScalarValue {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(v) = s.parse::<f64>() {
            Ok(TableScalarValue::Float(v))
        } else if let Ok(v) = s.parse::<i64>() {
            Ok(TableScalarValue::Int(v))
        } else {
            Err(())
        }
    }
}

/// A simple table with a string based key for scalar values.
pub struct ScalarTableOne {
    values: HashMap<String, TableScalarValue>,
}

impl ScalarTableOne {
    fn get_scalar(&self, index: &[&str]) -> Result<TableScalarValue, TableError> {
        if index.len() == 1 {
            self.values.get(index[0]).ok_or(TableError::EntryNotFound).copied()
        } else {
            Err(TableError::WrongKeySize(1, index.len()))
        }
    }
}

/// A simple table with two strings for a key to scalar values.
pub struct ScalarTableTwo {
    values: HashMap<(String, String), TableScalarValue>,
}

impl ScalarTableTwo {
    fn get_scalar(&self, index: &[&str]) -> Result<TableScalarValue, TableError> {
        if index.len() == 2 {
            // I think this copies the strings and is not very efficient.
            let k = (index[0].to_string(), index[1].to_string());
            self.values.get(&k).ok_or(TableError::EntryNotFound).copied()
        } else {
            Err(TableError::WrongKeySize(2, index.len()))
        }
    }
}

/// A simple table with three strings for a key to scalar values.
///
/// This table can not be indexed by position.
pub struct ScalarTableThree {
    values: HashMap<(String, String, String), TableScalarValue>,
}

impl ScalarTableThree {
    fn get_scalar(&self, index: &[&str]) -> Result<TableScalarValue, TableError> {
        if index.len() == 3 {
            // I think this copies the strings and is not very efficient.
            let k = (index[0].to_string(), index[1].to_string(), index[2].to_string());
            self.values.get(&k).ok_or(TableError::EntryNotFound).copied()
        } else {
            Err(TableError::WrongKeySize(3, index.len()))
        }
    }
}

pub enum LoadedScalarTable {
    One(ScalarTableOne),
    Two(ScalarTableTwo),
    Three(ScalarTableThree),
}

impl LoadedScalarTable {
    pub fn get_scalar(&self, key: &[&str]) -> Result<TableScalarValue, TableError> {
        match self {
            LoadedScalarTable::One(tbl) => tbl.get_scalar(key),
            LoadedScalarTable::Two(tbl) => tbl.get_scalar(key),
            LoadedScalarTable::Three(tbl) => tbl.get_scalar(key),
        }
    }
}

/// Load a CSV file with looks for each rows & columns
pub fn load_csv_row_col_scalar_table_one(
    table_path: &Path,
    data_path: Option<&Path>,
) -> Result<LoadedScalarTable, TableError> {
    let path = make_path(table_path, data_path);

    let file = File::open(path).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    let col_headers: Vec<String> = rdr
        .headers()
        .map_err(|e| TableError::Csv(e.to_string()))?
        .iter()
        .skip(1)
        .map(|s| s.to_string())
        .collect();

    struct Row {
        key: String,
        values: Vec<Option<TableScalarValue>>,
    }

    let rows: Vec<Row> = rdr
        .records()
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let key = record.get(0).ok_or(TableError::KeyParse)?.to_string();

            let values: Vec<Option<TableScalarValue>> = record
                .iter()
                .skip(1)
                .map(|v| TableScalarValue::from_str(v).ok())
                .collect();

            Ok(Row { key, values })
        })
        .collect::<Result<Vec<_>, TableError>>()?;

    // Turn this into a look-up table with key (row, column)
    let mut values: HashMap<(String, String), TableScalarValue> = HashMap::new();

    for row in &rows {
        for (col, value) in col_headers.iter().zip(&row.values) {
            if let Some(v) = value {
                values.insert((row.key.clone(), col.clone()), *v);
            }
        }
    }

    Ok(LoadedScalarTable::Two(ScalarTableTwo { values }))
}

pub fn load_csv_row_scalar_table_one(
    table_path: &Path,
    data_path: Option<&Path>,
) -> Result<LoadedScalarTable, TableError> {
    let path = make_path(table_path, data_path);

    let file = File::open(path.clone()).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    struct Row {
        key: String,
        value: Option<TableScalarValue>,
    }

    let values: Vec<Row> = rdr
        .records()
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let key = record.get(0).ok_or(TableError::KeyParse)?.to_string();

            let values: Vec<Option<_>> = record
                .iter()
                .skip(1)
                .map(|v| TableScalarValue::from_str(v).ok())
                .collect();

            if values.len() > 1 {
                return Err(TableError::TooManyValues(path.clone()));
            }

            Ok(Row { key, value: values[0] })
        })
        .collect::<Result<_, TableError>>()?;

    // Turn this into a look-up table with key (row)
    let values: HashMap<String, TableScalarValue> =
        values.into_iter().filter_map(|r| r.value.map(|v| (r.key, v))).collect();

    Ok(LoadedScalarTable::One(ScalarTableOne { values }))
}

pub fn load_csv_row2_scalar_table_one(
    table_path: &Path,
    data_path: Option<&Path>,
) -> Result<LoadedScalarTable, TableError> {
    let path = make_path(table_path, data_path);

    let file = File::open(path.clone()).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    let values: HashMap<(String, String), Option<TableScalarValue>> = rdr
        .records()
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let key = (
                record.get(0).ok_or(TableError::KeyParse)?.to_string(),
                record.get(1).ok_or(TableError::KeyParse)?.to_string(),
            );

            let values: Vec<_> = record
                .iter()
                .skip(2)
                .map(|v| TableScalarValue::from_str(v).ok())
                .collect();

            if values.len() > 1 {
                return Err(TableError::TooManyValues(path.clone()));
            }

            Ok((key, values[0]))
        })
        .collect::<Result<_, TableError>>()?;

    // Remove None values
    let values: HashMap<(String, String), TableScalarValue> =
        values.into_iter().filter_map(|(k, v)| v.map(|v| (k, v))).collect();

    Ok(LoadedScalarTable::Two(ScalarTableTwo { values }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    #[deny(clippy::approx_constant)]
    fn test_table_scalar_value_parsing() {
        assert_eq!(TableScalarValue::from_str("42"), Ok(TableScalarValue::Float(42.0)));
        assert_eq!(TableScalarValue::from_str("-7"), Ok(TableScalarValue::Float(-7.0)));
        assert_eq!(TableScalarValue::from_str("2.6"), Ok(TableScalarValue::Float(2.6)));
        assert!(TableScalarValue::from_str("not_a_number").is_err());
    }

    #[test]
    fn test_load_csv_row_scalar_table_one() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "key,value").unwrap();
        writeln!(file, "A,1.0").unwrap();
        writeln!(file, "B,2.0").unwrap();
        let path = file.path();
        let table = load_csv_row_scalar_table_one(path, None).unwrap();
        assert_eq!(table.get_scalar(&["A"]).unwrap().as_f64(), 1.0);
        assert_eq!(table.get_scalar(&["B"]).unwrap().as_f64(), 2.0);
        assert!(table.get_scalar(&["C"]).is_err());
    }

    #[test]
    fn test_load_csv_row2_scalar_table_one() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "key1,key2,value").unwrap();
        writeln!(file, "A,X,10.0").unwrap();
        writeln!(file, "B,Y,20.0").unwrap();
        let path = file.path();
        let table = load_csv_row2_scalar_table_one(path, None).unwrap();
        assert_eq!(table.get_scalar(&["A", "X"]).unwrap().as_f64(), 10.0);
        assert_eq!(table.get_scalar(&["B", "Y"]).unwrap().as_f64(), 20.0);
        assert!(table.get_scalar(&["C", "Z"]).is_err());
    }

    #[test]
    fn test_load_csv_row_col_scalar_table_one() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "key,col1,col2").unwrap();
        writeln!(file, "A,1.0,2.0").unwrap();
        writeln!(file, "B,3.0,4.0").unwrap();
        let path = file.path();
        let table = load_csv_row_col_scalar_table_one(path, None).unwrap();
        assert_eq!(table.get_scalar(&["A", "col1"]).unwrap().as_f64(), 1.0);
        assert_eq!(table.get_scalar(&["A", "col2"]).unwrap().as_f64(), 2.0);
        assert_eq!(table.get_scalar(&["B", "col1"]).unwrap().as_f64(), 3.0);
        assert_eq!(table.get_scalar(&["B", "col2"]).unwrap().as_f64(), 4.0);
        assert!(table.get_scalar(&["C", "col1"]).is_err());
    }

    #[test]
    fn test_wrong_key_size() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "key,value").unwrap();
        writeln!(file, "A,1.0").unwrap();
        let path = file.path();
        let table = load_csv_row_scalar_table_one(path, None).unwrap();
        // Should error if key size is wrong
        assert!(matches!(
            table.get_scalar(&["A", "extra"]),
            Err(TableError::WrongKeySize(1, 2))
        ));
    }
}
