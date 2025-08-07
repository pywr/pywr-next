use crate::data_tables::TableError;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::str::FromStr;

fn table_key_to_position(key: &str, keys: &[String]) -> Result<usize, TableError> {
    keys.iter().position(|k| k == key).ok_or(TableError::EntryNotFound)
}

/// A simple table with a string based key for scalar values.
pub struct ScalarTableOne<T> {
    keys: Vec<String>,
    values: Vec<T>,
}

impl<T> ScalarTableOne<T>
where
    T: Copy,
{
    fn get_scalar(&self, key: &str) -> Result<T, TableError> {
        let index = table_key_to_position(key, &self.keys)?;
        self.values
            .get(index)
            .ok_or(TableError::IndexOutOfBounds(index))
            .copied()
    }
}

/// A simple table with two strings for a key to scalar values.
pub struct ScalarTableR1C1<T> {
    index: (Vec<String>, Vec<String>),
    // Could this be flattened for a small performance gain?
    values: Vec<Vec<Option<T>>>,
}

impl<T> ScalarTableR1C1<T>
where
    T: Copy,
{
    fn get_scalar(&self, index: &[&str]) -> Result<T, TableError> {
        if index.len() == 2 {
            let idx0 = table_key_to_position(index[0], &self.index.0)?;
            let idx1 = table_key_to_position(index[1], &self.index.1)?;

            let value = self
                .values
                .get(idx0)
                .ok_or(TableError::IndexOutOfBounds(idx0))?
                .get(idx1)
                .ok_or(TableError::IndexOutOfBounds(idx1))?
                .ok_or_else(|| TableError::EntryNotFound)?;

            Ok(value)
        } else {
            Err(TableError::WrongKeySize(2, index.len()))
        }
    }
}

/// A simple table with two strings for a key to scalar values.
pub struct ScalarTableR2<T> {
    values: HashMap<(String, String), T>,
}

impl<T> ScalarTableR2<T>
where
    T: Copy,
{
    fn get_scalar(&self, index: &[&str]) -> Result<T, TableError> {
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
pub struct ScalarTableThree<T> {
    values: HashMap<(String, String, String), T>,
}

impl<T> ScalarTableThree<T>
where
    T: Copy,
{
    fn get_scalar(&self, index: &[&str]) -> Result<T, TableError> {
        if index.len() == 3 {
            // I think this copies the strings and is not very efficient.
            let k = (index[0].to_string(), index[1].to_string(), index[2].to_string());
            self.values.get(&k).ok_or(TableError::EntryNotFound).copied()
        } else {
            Err(TableError::WrongKeySize(3, index.len()))
        }
    }
}

pub enum LoadedScalarTable<T> {
    One(ScalarTableOne<T>),
    Row1Col1(ScalarTableR1C1<T>),
    Row2(ScalarTableR2<T>),
    Three(ScalarTableThree<T>),
}

impl<T> LoadedScalarTable<T>
where
    T: Copy,
{
    pub fn get_scalar(&self, key: &[&str]) -> Result<T, TableError> {
        match self {
            LoadedScalarTable::One(tbl) => {
                if key.len() == 1 {
                    tbl.get_scalar(key[0])
                } else {
                    Err(TableError::WrongKeySize(1, key.len()))
                }
            }
            LoadedScalarTable::Row1Col1(tbl) => tbl.get_scalar(key),
            LoadedScalarTable::Row2(tbl) => tbl.get_scalar(key),
            LoadedScalarTable::Three(tbl) => tbl.get_scalar(key),
        }
    }
}

/// Load a CSV file with looks for each rows & columns
pub fn load_csv_row_col_scalar_table_one<T>(path: &Path) -> Result<LoadedScalarTable<T>, TableError>
where
    T: FromStr + Copy,
    TableError: From<T::Err>,
{
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

    let mut row_headers: Vec<String> = Vec::new();
    let values: Vec<Vec<Option<T>>> = rdr
        .records()
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let key = record.get(0).ok_or(TableError::KeyParse)?.to_string();

            let values: Vec<Option<T>> = record.iter().skip(1).map(|v| v.parse::<T>().ok()).collect();

            row_headers.push(key.clone());

            Ok(values)
        })
        .collect::<Result<Vec<_>, TableError>>()?;

    Ok(LoadedScalarTable::Row1Col1(ScalarTableR1C1 {
        index: (row_headers, col_headers),
        values,
    }))
}

pub fn load_csv_row_scalar_table_one<T>(path: &Path) -> Result<LoadedScalarTable<T>, TableError>
where
    T: FromStr + Copy,
    TableError: From<T::Err>,
{
    let file = File::open(path).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    let (keys, values): (Vec<String>, Vec<T>) = rdr
        .records()
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let key = record.get(0).ok_or(TableError::KeyParse)?.to_string();

            let values: Vec<T> = record.iter().skip(1).map(|v| v.parse()).collect::<Result<_, _>>()?;

            if values.len() > 1 {
                return Err(TableError::TooManyValues(path.to_path_buf()));
            }

            Ok((key, values[0]))
        })
        .collect::<Result<Vec<_>, TableError>>()?
        .into_iter()
        .unzip();

    Ok(LoadedScalarTable::One(ScalarTableOne { keys, values }))
}

pub fn load_csv_row2_scalar_table_one<T>(path: &Path) -> Result<LoadedScalarTable<T>, TableError>
where
    T: FromStr + Copy,
    TableError: From<T::Err>,
{
    let file = File::open(path).map_err(|e| TableError::IO(e.to_string()))?;
    let buf_reader = BufReader::new(file);
    let mut rdr = csv::Reader::from_reader(buf_reader);

    let values: HashMap<(String, String), T> = rdr
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
                return Err(TableError::TooManyValues(path.to_path_buf()));
            }

            Ok((key, values[0]))
        })
        .collect::<Result<_, TableError>>()?;

    Ok(LoadedScalarTable::Row2(ScalarTableR2 { values }))
}
