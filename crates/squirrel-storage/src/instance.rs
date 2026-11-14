//! Binds a data directory to one installation (database), so a server pointed
//! at the wrong database refuses to start instead of treating every stored
//! blob as an orphan.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::PathBuf;

use uuid::Uuid;

use crate::error::StorageError;
use crate::fsutil::{remove_if_exists, sync_dir};
use crate::store::BlobStore;

/// File at the root of the data directory holding the installation id.
pub const INSTANCE_MARKER: &str = ".squirrel-instance";

/// How [`BlobStore::bind`] found the data directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Binding {
    /// The marker names this installation.
    Bound,
    /// The directory had no marker and no blob: the marker was written now.
    Claimed,
}

#[derive(Debug, thiserror::Error)]
pub enum BindingError {
    /// The marker names another installation.
    #[error(
        "the data directory belongs to installation {marker}, the database is installation {database}"
    )]
    Mismatch { marker: Uuid, database: Uuid },
    /// Blobs are stored but nothing says for which installation.
    #[error("the data directory holds blobs but has no installation marker")]
    Unmarked,
    #[error("{} does not hold an installation id", .0.display())]
    InvalidMarker(PathBuf),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl From<io::Error> for BindingError {
    fn from(err: io::Error) -> Self {
        Self::Storage(err.into())
    }
}

impl BlobStore {
    pub fn marker_path(&self) -> PathBuf {
        self.root().join(INSTANCE_MARKER)
    }

    /// The installation id the data directory is bound to, if any.
    pub fn read_marker(&self) -> Result<Option<Uuid>, BindingError> {
        let path = self.marker_path();
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        Uuid::parse_str(content.trim())
            .map(Some)
            .map_err(|_| BindingError::InvalidMarker(path))
    }

    /// Writes (or replaces) the marker durably: temporary file, fsync, rename.
    pub fn write_marker(&self, installation: Uuid) -> Result<(), StorageError> {
        let tmp = self.root().join(format!("{INSTANCE_MARKER}.tmp"));
        let result = (|| {
            let mut file = File::create(&tmp)?;
            writeln!(file, "{installation}")?;
            file.sync_all()?;
            fs::rename(&tmp, self.marker_path())?;
            sync_dir(self.root())
        })();
        if result.is_err() {
            let _ = remove_if_exists(&tmp);
        }
        Ok(result?)
    }

    /// Checks that the data directory belongs to `installation`. A directory
    /// without a marker is claimed only while it holds no blob; one with blobs
    /// must be adopted deliberately (with [`BlobStore::write_marker`]).
    pub fn bind(&self, installation: Uuid) -> Result<Binding, BindingError> {
        match self.read_marker()? {
            Some(marker) if marker == installation => Ok(Binding::Bound),
            Some(marker) => Err(BindingError::Mismatch {
                marker,
                database: installation,
            }),
            None if self.has_blobs()? => Err(BindingError::Unmarked),
            None => {
                self.write_marker(installation)?;
                Ok(Binding::Claimed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, BlobStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn an_empty_data_directory_is_claimed_then_bound() {
        let (dir, store) = store();
        let id = Uuid::now_v7();
        assert_eq!(store.bind(id).unwrap(), Binding::Claimed);
        assert_eq!(
            fs::read_to_string(dir.path().join(INSTANCE_MARKER)).unwrap(),
            format!("{id}\n")
        );
        assert_eq!(store.bind(id).unwrap(), Binding::Bound);
    }

    #[test]
    fn another_installation_is_refused() {
        let (_dir, store) = store();
        let (mine, other) = (Uuid::now_v7(), Uuid::now_v7());
        store.bind(mine).unwrap();
        match store.bind(other) {
            Err(BindingError::Mismatch { marker, database }) => {
                assert_eq!((marker, database), (mine, other));
            }
            result => panic!("expected a mismatch, got {result:?}"),
        }
    }

    #[test]
    fn blobs_without_a_marker_are_refused_until_adopted() {
        let (_dir, store) = store();
        store.put_reader(&b"existing"[..]).unwrap();
        let id = Uuid::now_v7();
        assert!(matches!(store.bind(id), Err(BindingError::Unmarked)));
        assert_eq!(store.read_marker().unwrap(), None);
        store.write_marker(id).unwrap();
        assert_eq!(store.bind(id).unwrap(), Binding::Bound);
    }

    #[test]
    fn a_garbled_marker_is_reported() {
        let (dir, store) = store();
        fs::write(dir.path().join(INSTANCE_MARKER), "not an id").unwrap();
        assert!(matches!(
            store.bind(Uuid::now_v7()),
            Err(BindingError::InvalidMarker(_))
        ));
    }

    #[test]
    fn upload_leftovers_do_not_count_as_blobs() {
        let (_dir, store) = store();
        store.create_upload(Uuid::now_v7(), 10).unwrap();
        assert_eq!(store.bind(Uuid::now_v7()).unwrap(), Binding::Claimed);
    }
}
