use crate::data_tables::TableError;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::str::FromStr;

/// A table that maps N string keys to a vector of T values.
pub struct VecTable<const N: usize, T> {
    values: HashMap<[String; N], Vec<T>>,
}

impl<const N: usize, T> VecTable<N, T> {
    fn get_vec(&self, key: &[&str]) -> Result<&[T], TableError> {
        if key.len() == N {
            // SAFETY: Length checked above.
            let k: [String; N] = key
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<String>>()
                .try_into()
                .unwrap();

            self.values
                .get(&k)
                .map(|a| a.as_slice())
                .ok_or(TableError::EntryNotFound)
        } else {
            Err(TableError::WrongKeySize(N, key.len()))
        }
    }
}

pub enum LoadedVecTable<T> {
    One(VecTable<1, T>),
    Two(VecTable<2, T>),
    Three(VecTable<3, T>),
    Four(VecTable<4, T>),
}

impl<T> LoadedVecTable<T>
where
    T: Copy,
{
    pub fn get_vec(&self, key: &[&str]) -> Result<&[T], TableError> {
        match self {
            LoadedVecTable::One(tbl) => tbl.get_vec(key),
            LoadedVecTable::Two(tbl) => tbl.get_vec(key),
            LoadedVecTable::Three(tbl) => tbl.get_vec(key),
            LoadedVecTable::Four(tbl) => tbl.get_vec(key),
        }
    }

    pub fn from_csv_row(path: &Path, rows: usize) -> Result<Self, TableError>
    where
        T: FromStr,
        TableError: From<T::Err>,
    {
        match rows {
            1 => Ok(LoadedVecTable::One(load_csv_row_vec_table(path)?)),
            2 => Ok(LoadedVecTable::Two(load_csv_row_vec_table(path)?)),
            3 => Ok(LoadedVecTable::Three(load_csv_row_vec_table(path)?)),
            4 => Ok(LoadedVecTable::Four(load_csv_row_vec_table(path)?)),
            _ => Err(TableError::FormatNotSupported(
                "CSV row array table with more than four index columns is not supported.".to_string(),
            )),
        }
    }

    pub fn from_csv_col(path: &Path, cols: usize) -> Result<Self, TableError>
    where
        T: FromStr,
        TableError: From<T::Err>,
    {
        match cols {
            1 => Ok(LoadedVecTable::One(load_csv_col_vec_table(path)?)),
            2 => Ok(LoadedVecTable::Two(load_csv_col_vec_table(path)?)),
            3 => Ok(LoadedVecTable::Three(load_csv_col_vec_table(path)?)),
            4 => Ok(LoadedVecTable::Four(load_csv_col_vec_table(path)?)),
            _ => Err(TableError::FormatNotSupported(
                "CSV row array table with more than four index columns is not supported.".to_string(),
            )),
        }
    }
}

fn load_csv_row_vec_table<const N: usize, T>(path: &Path) -> Result<VecTable<N, T>, TableError>
where
    T: FromStr,
    TableError: From<T::Err>,
{
    let file = File::open(path).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    let values: HashMap<[String; N], Vec<T>> = rdr
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

            let values: Vec<T> = record.iter().skip(N).map(|v| v.parse()).collect::<Result<_, _>>()?;

            Ok((key, values))
        })
        .collect::<Result<_, TableError>>()?;

    Ok(VecTable { values })
}

fn load_csv_col_vec_table<const N: usize, T>(path: &Path) -> Result<VecTable<N, T>, TableError>
where
    T: FromStr + Copy,
    TableError: From<T::Err>,
{
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

            let values: Vec<T> = record
                .iter()
                .map(|v| v.parse::<T>())
                .collect::<Result<_, <T as FromStr>::Err>>()?;

            if values.len() != col_headers.len() {
                return Err(TableError::Csv("Row length does not match header length".to_string()));
            }

            Ok(values)
        })
        .collect::<Result<Vec<_>, TableError>>()?;

    let values = col_headers
        .into_iter()
        .enumerate()
        .map(|(i, header)| {
            // SAFETY: We have read N header rows.
            let key: [String; N] = header.try_into().unwrap();
            let col_values: Vec<T> = rows.iter().map(|r| r[i]).collect();
            (key, col_values)
        })
        .collect();

    Ok(VecTable { values })
}
