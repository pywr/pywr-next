use crate::data_tables::TableError;
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

/// A simple table with N strings for a key to scalar values.
pub struct ScalarTable<const N: usize> {
    values: HashMap<[String; N], TableScalarValue>,
}

impl<const N: usize> ScalarTable<N> {
    fn get_scalar(&self, key: &[&str]) -> Result<TableScalarValue, TableError> {
        if key.len() == N {
            // SAFETY: Length checked above.
            let k: [String; N] = key
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            self.values.get(&k).ok_or(TableError::EntryNotFound).copied()
        } else {
            Err(TableError::WrongKeySize(N, key.len()))
        }
    }
}

pub enum LoadedScalarTable {
    One(ScalarTable<1>),
    Two(ScalarTable<2>),
    Three(ScalarTable<3>),
    Four(ScalarTable<4>),
}

impl LoadedScalarTable {
    pub fn get_scalar(&self, key: &[&str]) -> Result<TableScalarValue, TableError> {
        match self {
            LoadedScalarTable::One(tbl) => tbl.get_scalar(key),
            LoadedScalarTable::Two(tbl) => tbl.get_scalar(key),
            LoadedScalarTable::Three(tbl) => tbl.get_scalar(key),
            LoadedScalarTable::Four(tbl) => tbl.get_scalar(key),
        }
    }

    /// Load a CSV file with a row-based index of size `rows`.
    pub fn from_csv_row(path: &Path, rows: usize) -> Result<LoadedScalarTable, TableError> {
        match rows {
            1 => Ok(LoadedScalarTable::One(load_csv_rows_scalar_table(path)?)),
            2 => Ok(LoadedScalarTable::Two(load_csv_rows_scalar_table(path)?)),
            3 => Ok(LoadedScalarTable::Two(load_csv_rows_scalar_table(path)?)),
            4 => Ok(LoadedScalarTable::Two(load_csv_rows_scalar_table(path)?)),
            _ => Err(TableError::FormatNotSupported(
                "CSV row scalar table with more than four index columns is not supported.".to_string(),
            )),
        }
    }

    /// Load a CSV file with a col-based index of size `cols`.
    pub fn from_csv_col(path: &Path, cols: usize) -> Result<LoadedScalarTable, TableError> {
        match cols {
            1 => Ok(LoadedScalarTable::One(load_csv_cols_scalar_table(path)?)),
            2 => Ok(LoadedScalarTable::Two(load_csv_cols_scalar_table(path)?)),
            3 => Ok(LoadedScalarTable::Two(load_csv_cols_scalar_table(path)?)),
            4 => Ok(LoadedScalarTable::Two(load_csv_cols_scalar_table(path)?)),
            _ => Err(TableError::FormatNotSupported(
                "CSV row scalar table with more than four index columns is not supported.".to_string(),
            )),
        }
    }

    pub fn from_csv_row_col(path: &Path, rows: usize, cols: usize) -> Result<LoadedScalarTable, TableError> {
        match (rows, cols) {
            (1, 1) => Ok(LoadedScalarTable::Two(load_csv_row_col_scalar_table::<1, 1, _>(path)?)),
            (1, 2) => Ok(LoadedScalarTable::Three(load_csv_row_col_scalar_table::<1, 2, _>(
                path,
            )?)),
            (2, 1) => Ok(LoadedScalarTable::Three(load_csv_row_col_scalar_table::<2, 1, _>(
                path,
            )?)),
            (2, 2) => Ok(LoadedScalarTable::Four(load_csv_row_col_scalar_table::<2, 2, _>(path)?)),
            _ => Err(TableError::FormatNotSupported(
                "CSV row/column scalar table with more than two row or columns is not supported.".to_string(),
            )),
        }
    }
}

/// Load a CSV file with a look-up for rows & columns.
///
/// The CSV file should have a header row(s) with the column names, and first column(s)
/// with the row names. The rest of the cells should be scalar values.
fn load_csv_row_col_scalar_table<const R: usize, const C: usize, const N: usize>(
    path: &Path,
) -> Result<ScalarTable<N>, TableError> {
    // Ensure R + C == N
    // Const generic expressions are not yet stable, so we use an assert at runtime.
    assert_eq!(R + C, N, "R + C must equal N");

    let file = File::open(path).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    // Read the column headers
    // Skip the first C columns which are for the row keys
    // Each header is a vector of strings, one for each row header
    let mut col_headers: Vec<Vec<String>> = rdr
        .headers()
        .map_err(|e| TableError::Csv(e.to_string()))?
        .iter()
        .skip(C)
        .map(|s| {
            let mut h = Vec::with_capacity(R);
            h.push(s.to_string());
            h
        })
        .collect();

    let mut records = rdr.records();

    // Read the next R-1 header rows
    for _ in 1..R {
        let next_headers = records.next();
        if let Some(Ok(record)) = next_headers {
            for (i, header) in record.iter().skip(C).enumerate() {
                col_headers[i].push(header.to_string());
            }
        } else {
            return Err(TableError::Csv("Not enough header rows".to_string()));
        }
    }

    struct Row {
        key: Vec<String>,
        values: Vec<Option<TableScalarValue>>,
    }

    let rows: Vec<Row> = records
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let key: Vec<_> = (0..C)
                .map(|i| Ok(record.get(i).ok_or(TableError::KeyParse)?.to_string()))
                .collect::<Result<Vec<_>, TableError>>()?;

            let values: Vec<Option<TableScalarValue>> = record
                .iter()
                .skip(C)
                .map(|v| TableScalarValue::from_str(v).ok())
                .collect();

            Ok(Row { key, values })
        })
        .collect::<Result<Vec<_>, TableError>>()?;

    // Turn this into a look-up table with key (row, column)
    let mut values: HashMap<[String; N], TableScalarValue> = HashMap::new();

    for row in &rows {
        for (col, value) in col_headers.iter().zip(&row.values) {
            if let Some(v) = value {
                // SAFETY: We have already checked that R + C == N
                let key: [String; N] = row
                    .key
                    .iter()
                    .cloned()
                    .chain(col.iter().cloned())
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap();

                values.insert(key, *v);
            }
        }
    }

    Ok(ScalarTable { values })
}

/// Load a CSV file with a row-based index of size `N`.
fn load_csv_rows_scalar_table<const N: usize>(path: &Path) -> Result<ScalarTable<N>, TableError> {
    let file = File::open(path).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    let values: HashMap<[String; N], Option<TableScalarValue>> = rdr
        .records()
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let key: [String; N] = (0..N)
                .map(|i| Ok(record.get(i).ok_or(TableError::KeyParse)?.to_string()))
                .collect::<Result<Vec<_>, TableError>>()?
                .try_into()
                .unwrap();

            let values: Vec<_> = record
                .iter()
                .skip(N)
                .map(|v| TableScalarValue::from_str(v).ok())
                .collect();

            if values.len() > 1 {
                return Err(TableError::TooManyColumns {
                    found: values.len() + N,
                    expected: 1 + N,
                });
            }

            Ok((key, values[0]))
        })
        .collect::<Result<_, TableError>>()?;

    // Remove None values
    let values: HashMap<[String; N], TableScalarValue> =
        values.into_iter().filter_map(|(k, v)| v.map(|v| (k, v))).collect();

    Ok(ScalarTable { values })
}

/// Load a CSV file with a look-up for columns.
///
/// The CSV file should have a header row(s) with the column names.
/// The rest of the cells should be scalar values.
fn load_csv_cols_scalar_table<const N: usize>(path: &Path) -> Result<ScalarTable<N>, TableError> {
    let file = File::open(path).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    // Read the column headers
    // Skip the first R columns which are for the row keys
    // Each header is a vector of strings, one for each row header
    let mut col_headers: Vec<Vec<String>> = rdr
        .headers()
        .map_err(|e| TableError::Csv(e.to_string()))?
        .iter()
        .map(|s| {
            let mut h = Vec::with_capacity(N);
            h.push(s.to_string());
            h
        })
        .collect();

    let mut records = rdr.records();

    // Read the next N-1 header rows
    for _ in 1..N {
        let next_headers = records.next();
        if let Some(Ok(record)) = next_headers {
            for (i, header) in record.iter().enumerate() {
                col_headers[i].push(header.to_string());
            }
        } else {
            return Err(TableError::Csv("Not enough header rows".to_string()));
        }
    }

    let rows: Vec<_> = records
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let values: Vec<Option<TableScalarValue>> =
                record.iter().map(|v| TableScalarValue::from_str(v).ok()).collect();

            Ok(values)
        })
        .collect::<Result<Vec<_>, TableError>>()?;

    if rows.len() > 1 {
        return Err(TableError::TooManyRows {
            found: rows.len() + N,
            expected: 1 + N,
        });
    }

    // Turn this into a look-up table with key (row, column)
    let mut values: HashMap<[String; N], TableScalarValue> = HashMap::new();

    for (col, value) in col_headers.iter().zip(&rows[0]) {
        if let Some(v) = value {
            // SAFETY: We have already checked ensured that the number of header rows is N
            let key: [String; N] = col.to_vec().try_into().unwrap();

            values.insert(key, *v);
        }
    }

    Ok(ScalarTable { values })
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
        let table: ScalarTable<1> = load_csv_rows_scalar_table(path).unwrap();
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
        let table: ScalarTable<2> = load_csv_rows_scalar_table(path).unwrap();
        assert_eq!(table.get_scalar(&["A", "X"]).unwrap().as_f64(), 10.0);
        assert_eq!(table.get_scalar(&["B", "Y"]).unwrap().as_f64(), 20.0);
        assert!(table.get_scalar(&["C", "Z"]).is_err());
    }

    #[test]
    fn test_load_csv_col_scalar_table_one() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "A,B").unwrap();
        writeln!(file, "1.0,2.0").unwrap();
        let path = file.path();
        let table: ScalarTable<1> = load_csv_cols_scalar_table(path).unwrap();
        assert_eq!(table.get_scalar(&["A"]).unwrap().as_f64(), 1.0);
        assert_eq!(table.get_scalar(&["B"]).unwrap().as_f64(), 2.0);
        assert!(table.get_scalar(&["C"]).is_err());
    }

    #[test]
    fn test_load_csv_col2_scalar_table_one() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "A,B").unwrap();
        writeln!(file, "X,Y").unwrap();
        writeln!(file, "10.0,20.0").unwrap();
        let path = file.path();
        let table: ScalarTable<2> = load_csv_cols_scalar_table(path).unwrap();
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
        let table = load_csv_row_col_scalar_table::<1, 1, 2>(path).unwrap();
        assert_eq!(table.get_scalar(&["A", "col1"]).unwrap().as_f64(), 1.0);
        assert_eq!(table.get_scalar(&["A", "col2"]).unwrap().as_f64(), 2.0);
        assert_eq!(table.get_scalar(&["B", "col1"]).unwrap().as_f64(), 3.0);
        assert_eq!(table.get_scalar(&["B", "col2"]).unwrap().as_f64(), 4.0);
        assert!(table.get_scalar(&["C", "col1"]).is_err());
    }

    #[test]
    fn test_load_csv_row2_col2_scalar_table_one() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "key1,key2,col1,col2").unwrap();
        writeln!(file, "key1,key2,A,A").unwrap();
        writeln!(file, "X,A,1.0,2.0").unwrap();
        writeln!(file, "X,B,3.0,4.0").unwrap();
        let path = file.path();
        let table = load_csv_row_col_scalar_table::<2, 2, 4>(path).unwrap();
        assert_eq!(table.get_scalar(&["X", "A", "col1", "A"]).unwrap().as_f64(), 1.0);
        assert_eq!(table.get_scalar(&["X", "A", "col2", "A"]).unwrap().as_f64(), 2.0);
        assert_eq!(table.get_scalar(&["X", "B", "col1", "A"]).unwrap().as_f64(), 3.0);
        assert_eq!(table.get_scalar(&["X", "B", "col2", "A"]).unwrap().as_f64(), 4.0);
        assert!(table.get_scalar(&["Y", "C", "col1", "B"]).is_err());
    }

    #[test]
    fn test_load_csv_row2_col_scalar_table_one() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "key1,col1,col2").unwrap();
        writeln!(file, "key1,A,A").unwrap();
        writeln!(file, "X,1.0,2.0").unwrap();
        writeln!(file, "Y,3.0,4.0").unwrap();
        let path = file.path();
        let table = load_csv_row_col_scalar_table::<2, 1, 3>(path).unwrap();
        assert_eq!(table.get_scalar(&["X", "col1", "A"]).unwrap().as_f64(), 1.0);
        assert_eq!(table.get_scalar(&["X", "col2", "A"]).unwrap().as_f64(), 2.0);
        assert_eq!(table.get_scalar(&["Y", "col1", "A"]).unwrap().as_f64(), 3.0);
        assert_eq!(table.get_scalar(&["Y", "col2", "A"]).unwrap().as_f64(), 4.0);
        assert!(table.get_scalar(&["Y", "col1", "B"]).is_err());
    }

    #[test]
    fn test_wrong_key_size() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "key,value").unwrap();
        writeln!(file, "A,1.0").unwrap();
        let path = file.path();
        let table: ScalarTable<1> = load_csv_rows_scalar_table(path).unwrap();
        // Should error if key size is wrong
        assert!(matches!(
            table.get_scalar(&["A", "extra"]),
            Err(TableError::WrongKeySize(1, 2))
        ));
    }
}
