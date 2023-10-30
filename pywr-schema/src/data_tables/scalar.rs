use crate::data_tables::{make_path, TableError};
use crate::parameters::TableIndexEntry;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::str::FromStr;

fn table_index_to_position(key: &TableIndexEntry, keys: &[String]) -> Result<usize, TableError> {
    match key {
        TableIndexEntry::Index(idx) => Ok(*idx),
        TableIndexEntry::Name(name) => keys.iter().position(|k| k == name).ok_or(TableError::EntryNotFound),
    }
}

fn table_index_to_str(key: &TableIndexEntry) -> Result<&str, TableError> {
    match key {
        TableIndexEntry::Index(_) => Err(TableError::IndexingByPositionNotSupported),
        TableIndexEntry::Name(name) => Ok(name.as_str()),
    }
}

/// A simple table with a string based key for scalar values.
struct ScalarTableOne<T> {
    keys: Vec<String>,
    values: Vec<T>,
}

impl<T> ScalarTableOne<T>
where
    T: Copy,
{
    fn get_scalar(&self, index: &TableIndexEntry) -> Result<T, TableError> {
        let k = table_index_to_position(index, &self.keys)?;
        self.values.get(k).ok_or(TableError::IndexOutOfBounds(k)).copied()
    }
}

/// A simple table with two strings for a key to scalar values.
struct ScalarTableTwo<T> {
    index: (Vec<String>, Vec<String>),
    // Could this be flattened for a small performance gain?
    values: Vec<Vec<T>>,
}

impl<T> ScalarTableTwo<T>
where
    T: Copy,
{
    fn get_scalar(&self, index: &[&TableIndexEntry]) -> Result<T, TableError> {
        if index.len() == 2 {
            // I think this copies the strings and is not very efficient.
            let k0 = table_index_to_position(index[0], &self.index.0)?;
            let k1 = table_index_to_position(index[1], &self.index.1)?;

            self.values
                .get(k0)
                .ok_or(TableError::IndexOutOfBounds(k0))?
                .get(k1)
                .ok_or(TableError::IndexOutOfBounds(k1))
                .copied()
        } else {
            Err(TableError::WrongKeySize(2, index.len()))
        }
    }
}

/// A simple table with three strings for a key to scalar values.
///
/// This table can not be indexed by position.
struct ScalarTableThree<T> {
    values: HashMap<(String, String, String), T>,
}

impl<T> ScalarTableThree<T>
where
    T: Copy,
{
    fn get_scalar(&self, index: &[&TableIndexEntry]) -> Result<T, TableError> {
        if index.len() == 3 {
            // I think this copies the strings and is not very efficient.
            let k0 = table_index_to_str(index[0])?;
            let k1 = table_index_to_str(index[1])?;
            let k2 = table_index_to_str(index[2])?;

            let k = (k0.to_string(), k1.to_string(), k2.to_string());

            self.values.get(&k).ok_or(TableError::EntryNotFound).copied()
        } else {
            Err(TableError::WrongKeySize(3, index.len()))
        }
    }
}

pub enum LoadedScalarTable<T> {
    One(ScalarTableOne<T>),
    Two(ScalarTableTwo<T>),
    Three(ScalarTableThree<T>),
}

impl<T> LoadedScalarTable<T>
where
    T: Copy,
{
    fn get_scalar(&self, key: &[&TableIndexEntry]) -> Result<T, TableError> {
        match self {
            LoadedScalarTable::One(tbl) => {
                if key.len() == 1 {
                    tbl.get_scalar(key[0])
                } else {
                    Err(TableError::WrongKeySize(1, key.len()))
                }
            }
            LoadedScalarTable::Two(tbl) => tbl.get_scalar(key),
            LoadedScalarTable::Three(tbl) => tbl.get_scalar(key),
        }
    }
}

/// Load a CSV file with looks for each rows & columns
pub fn load_csv_row_col_scalar_table_one<T>(
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

    let col_headers: Vec<String> = rdr
        .headers()
        .map_err(|e| TableError::Csv(e.to_string()))?
        .iter()
        .skip(1)
        .map(|s| s.to_string())
        .collect();

    let mut row_headers: Vec<String> = Vec::new();
    let values: Vec<Vec<T>> = rdr
        .records()
        .map(|result| {
            // The iterator yields Result<StringRecord, Error>, so we check the
            // error here.
            let record = result.map_err(|e| TableError::Csv(e.to_string()))?;

            let key = record.get(0).ok_or(TableError::KeyParse)?.to_string();

            let values: Vec<T> = record.iter().skip(1).map(|v| v.parse()).collect::<Result<_, _>>()?;

            row_headers.push(key.clone());

            Ok(values)
        })
        .collect::<Result<Vec<_>, TableError>>()?;

    Ok(LoadedScalarTable::Two(ScalarTableTwo {
        index: (row_headers, col_headers),
        values,
    }))
}

pub fn load_csv_row_scalar_table_one<T>(
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

    let (keys, values): (Vec<String>, Vec<T>) = rdr
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
        .collect::<Result<Vec<_>, TableError>>()?
        .into_iter()
        .unzip();

    Ok(LoadedScalarTable::One(ScalarTableOne { keys, values }))
}

pub fn load_csv_row2_scalar_table_one<T>(
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
                return Err(TableError::TooManyValues(path.clone()));
            }

            Ok((key, values[0]))
        })
        .collect::<Result<_, TableError>>()?;

    Ok(LoadedScalarTable::Three(ScalarTableThree { values }))
}
