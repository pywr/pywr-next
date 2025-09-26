use crate::{ConversionData, ConversionError, TryFromV1};
use digest::Digest;
use md5::Md5;
use pywr_schema_macros::PywrVisitAll;
use schemars::JsonSchema;
use sha2::Sha256;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Write, copy};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ChecksumError {
    #[error("Checksum mismatch (actual: `{actual}`, expected: `{expected}`) for file: {path}")]
    ChecksumMismatch {
        actual: String,
        expected: String,
        path: PathBuf,
    },
    #[error("IO error when trying to read `{path}`: {source}")]
    IoError {
        path: PathBuf,
        #[source]
        source: Box<std::io::Error>,
    },
}

/// A checksum for a file, either MD5 or SHA256.
///
/// This is used to verify the integrity of files, such as those downloaded. These are
/// commonly used to ensure the correct version of a file has been downloaded, or to verify that a
/// file has not been corrupted.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, JsonSchema, PywrVisitAll)]
#[serde(tag = "type")]
pub enum Checksum {
    MD5 { hash: String },
    SHA256 { hash: String },
}

impl Checksum {
    pub fn check(&self, path: &Path) -> Result<(), ChecksumError> {
        match self {
            Checksum::MD5 { hash } => validate_hex_digest::<Md5>(path, hash),
            Checksum::SHA256 { hash } => validate_hex_digest::<Sha256>(path, hash),
        }
    }
}

/// Create a new checksum from a HashMap of hashes.
///
/// This is the format used in the Pywr v1.x schema. We can only keep a single checksum.
impl TryFromV1<HashMap<String, String>> for Checksum {
    type Error = ConversionError;

    fn try_from_v1(
        value: HashMap<String, String>,
        _parent_node: Option<&str>,
        _conversion_data: &mut ConversionData,
    ) -> Result<Self, Self::Error> {
        if let Some(hash) = value.get("md5") {
            Ok(Checksum::MD5 { hash: hash.clone() })
        } else if let Some(hash) = value.get("sha256") {
            Ok(Checksum::SHA256 { hash: hash.clone() })
        } else {
            let algos: String = value.keys().map(|k| k.to_string()).collect::<Vec<String>>().join(", ");

            Err(ConversionError::UnsupportedFeature {
                feature: format!(
                    "None of the hash algorithm(s) `{algos}` are supported. Only `md5` and `sha256` are currently supported."
                ),
            })
        }
    }
}

/// Validate a file's checksum against the expected hash.
fn validate_hex_digest<D: Digest + Write>(path: &Path, expected: &str) -> Result<(), ChecksumError> {
    let input = File::open(path).map_err(|e| ChecksumError::IoError {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    let mut reader = BufReader::new(input);

    let mut hasher = D::new();
    let _n = copy(&mut reader, &mut hasher).map_err(|e| ChecksumError::IoError {
        path: path.to_path_buf(),
        source: Box::new(e),
    })?;
    let hash = hasher.finalize();

    let actual_hash = hex::encode(hash);
    if actual_hash == expected {
        Ok(())
    } else {
        Err(ChecksumError::ChecksumMismatch {
            actual: actual_hash,
            expected: expected.to_string(),
            path: path.to_path_buf(),
        })
    }
}
