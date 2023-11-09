use crate::data_tables::{make_path, TableError};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::str::FromStr;

pub enum LoadedVecTable<T> {
    One(HashMap<String, Vec<T>>),
    Two(HashMap<(String, String), Vec<T>>),
    Three(HashMap<(String, String, String), Vec<T>>),
}

impl<T> LoadedVecTable<T>
where
    T: Copy,
{
    pub fn get_vec(&self, key: &[&str]) -> Result<&Vec<T>, TableError> {
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

pub fn load_csv_row_vec_table_one<T>(
    table_path: &Path,
    data_path: Option<&Path>,
) -> Result<LoadedVecTable<T>, TableError>
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

pub fn load_csv_row2_vec_table_one<T>(
    table_path: &Path,
    data_path: Option<&Path>,
) -> Result<LoadedVecTable<T>, TableError>
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
