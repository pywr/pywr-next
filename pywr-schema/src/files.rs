use bytes::Bytes;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Cursor, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

/// Opens the files a model reads as it is built: its CSV tables, its Arrow and Parquet time
/// series and their checksums, and the network files of a multi-network model.
///
/// [`FileSystem`] opens them on disk, and [`MemoryFiles`] serves preloaded bytes for a host with
/// no file system, such as a browser.
pub trait FileProvider {
    /// Open the file at `path`: a path from the model, resolved against the data path.
    fn open(&self, path: &Path) -> io::Result<InputFile>;
}

/// An open input file.
///
/// Parquet reads only from a file or from bytes, so this is one of the two.
pub enum InputFile {
    Disk(File),
    Memory(Cursor<Bytes>),
}

impl Read for InputFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Disk(file) => file.read(buf),
            Self::Memory(cursor) => cursor.read(buf),
        }
    }
}

impl Seek for InputFile {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        match self {
            Self::Disk(file) => file.seek(pos),
            Self::Memory(cursor) => cursor.seek(pos),
        }
    }
}

/// Opens files on disk.
#[derive(Debug, Clone, Copy)]
pub struct FileSystem;

impl FileProvider for FileSystem {
    fn open(&self, path: &Path) -> io::Result<InputFile> {
        File::open(path).map(InputFile::Disk)
    }
}

/// Files held in memory, keyed by path.
///
/// Paths are normalised on insert and on lookup, so `models/../data/inflow.csv` finds a file
/// inserted as `data/inflow.csv`.
#[derive(Clone, Default)]
pub struct MemoryFiles {
    files: HashMap<PathBuf, Bytes>,
}

impl MemoryFiles {
    /// Add a file, replacing any at the same path.
    pub fn insert(&mut self, path: impl AsRef<Path>, contents: impl Into<Bytes>) {
        self.files.insert(normalise(path.as_ref()), contents.into());
    }
}

impl FileProvider for MemoryFiles {
    fn open(&self, path: &Path) -> io::Result<InputFile> {
        let path = normalise(path);
        let contents = self.files.get(&path).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("`{}` is not in memory", path.display()),
            )
        })?;
        Ok(InputFile::Memory(Cursor::new(contents.clone())))
    }
}

/// Remove `.` from `path` and resolve each `..` against the component before it, without a file
/// system. A `..` with no component before it to remove is kept.
fn normalise(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if matches!(components.last(), Some(Component::Normal(_))) => {
                components.pop();
            }
            component => components.push(component),
        }
    }
    components.iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_files_normalise_paths() {
        let mut files = MemoryFiles::default();
        files.insert("data/inflow.csv", "date,inflow".as_bytes());

        for path in [
            "data/inflow.csv",
            "./data/inflow.csv",
            "models/m2/../../data/inflow.csv",
        ] {
            let mut contents = String::new();
            files
                .open(Path::new(path))
                .unwrap()
                .read_to_string(&mut contents)
                .unwrap();
            assert_eq!(contents, "date,inflow", "{path}");
        }

        // The second `..` has nothing left to remove, so it stays.
        let error = files.open(Path::new("models/../../data/inflow.csv")).err().unwrap();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        let key = Path::new("..").join("data").join("inflow.csv");
        assert!(error.to_string().contains(&key.display().to_string()), "{error}");
    }
}
