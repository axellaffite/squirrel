use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use squirrel_core::ContentHash;
use uuid::Uuid;

use crate::error::{Result, StorageError};
use crate::fsutil::{device_of, make_read_only, remove_if_exists, sync_dir};

const COPY_BUFFER: usize = 1024 * 1024;

/// Content-addressed blob store rooted at the data directory:
///
/// ```text
/// <data_dir>/blobs/ab/cd/<sha256>   immutable, read-only blobs
/// <data_dir>/tmp/                   in-progress writes (same filesystem, so renames are atomic)
/// <data_dir>/tmp/uploads/<id>       chunked upload sessions
/// ```
///
/// All methods block: call them from `spawn_blocking`.
#[derive(Debug, Clone)]
pub struct BlobStore {
    blobs: PathBuf,
    tmp: PathBuf,
    uploads: PathBuf,
}

/// Whether [`BlobStore::adopt`] stored new content or found it already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adopted {
    Stored,
    AlreadyPresent,
}

impl BlobStore {
    /// Creates the directory layout if needed and checks that `tmp/` and `blobs/`
    /// share a filesystem (otherwise `rename` would not be atomic).
    pub fn open(data_dir: &Path) -> Result<Self> {
        let store = Self {
            blobs: data_dir.join("blobs"),
            tmp: data_dir.join("tmp"),
            uploads: data_dir.join("tmp").join("uploads"),
        };
        for dir in [&store.blobs, &store.tmp, &store.uploads] {
            fs::create_dir_all(dir)?;
        }
        if device_of(&store.tmp)? != device_of(&store.blobs)? {
            return Err(StorageError::CrossDevice(store.tmp.display().to_string()));
        }
        Ok(store)
    }

    pub fn blob_path(&self, hash: &ContentHash) -> PathBuf {
        let (a, b) = hash.shard();
        self.blobs.join(a).join(b).join(hash.as_str())
    }

    pub fn contains(&self, hash: &ContentHash) -> bool {
        self.blob_path(hash).is_file()
    }

    /// Moves a fully written, verified temporary file into place under `hash`.
    /// Idempotent: if the blob already exists the temporary file is removed.
    pub fn adopt(&self, tmp: &Path, hash: &ContentHash) -> Result<Adopted> {
        let dest = self.blob_path(hash);
        if dest.is_file() {
            remove_if_exists(tmp)?;
            return Ok(Adopted::AlreadyPresent);
        }
        File::open(tmp)?.sync_all()?;
        make_read_only(tmp)?;
        let dir = dest.parent().expect("blob paths have a parent");
        fs::create_dir_all(dir)?;
        fs::rename(tmp, &dest)?;
        sync_dir(dir)?;
        Ok(Adopted::Stored)
    }

    /// Streams `reader` into a temporary file while hashing it, then adopts it.
    pub fn put_reader(&self, mut reader: impl Read) -> Result<(ContentHash, u64)> {
        let tmp = self.tmp.join(format!("put-{}", Uuid::now_v7()));
        let result = (|| {
            let mut file = File::create_new(&tmp)?;
            let mut hasher = Sha256::new();
            let mut buffer = vec![0; COPY_BUFFER];
            let mut size = 0u64;
            loop {
                let read = match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => read,
                    Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                    Err(err) => return Err(err.into()),
                };
                hasher.update(&buffer[..read]);
                file.write_all(&buffer[..read])?;
                size += read as u64;
            }
            let hash = ContentHash::from_digest(hasher.finalize().into());
            self.adopt(&tmp, &hash)?;
            Ok((hash, size))
        })();
        if result.is_err() {
            let _ = remove_if_exists(&tmp);
        }
        result
    }

    /// Opens a blob for reading, with its size.
    pub fn open_blob(&self, hash: &ContentHash) -> Result<(File, u64)> {
        let file = File::open(self.blob_path(hash)).map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => StorageError::NotFound,
            _ => err.into(),
        })?;
        let size = file.metadata()?.len();
        Ok((file, size))
    }

    /// Deletes a blob; a missing blob is not an error.
    pub fn remove_blob(&self, hash: &ContentHash) -> Result<()> {
        Ok(remove_if_exists(&self.blob_path(hash))?)
    }

    /// Free bytes available to this process on the data filesystem.
    pub fn available_space(&self) -> Result<u64> {
        let stat = rustix::fs::statvfs(&self.blobs).map_err(io::Error::from)?;
        Ok(stat.f_bavail.saturating_mul(stat.f_frsize))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    fn store() -> (tempfile::TempDir, BlobStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn open_creates_the_layout() {
        let (dir, _store) = store();
        for sub in ["blobs", "tmp", "tmp/uploads"] {
            assert!(dir.path().join(sub).is_dir(), "{sub}");
        }
    }

    #[test]
    fn put_reader_stores_content_under_its_hash() {
        let (_dir, store) = store();
        let (hash, size) = store.put_reader(&b"hello"[..]).unwrap();
        assert_eq!(hash.as_str(), HELLO);
        assert_eq!(size, 5);
        let path = store.blob_path(&hash);
        assert!(path.ends_with(format!("2c/f2/{HELLO}")));
        assert_eq!(fs::read(&path).unwrap(), b"hello");
        assert!(fs::metadata(&path).unwrap().permissions().readonly());
    }

    #[test]
    fn storing_the_same_content_twice_is_idempotent() {
        let (dir, store) = store();
        let first = store.put_reader(&b"hello"[..]).unwrap();
        let second = store.put_reader(&b"hello"[..]).unwrap();
        assert_eq!(first, second);
        let leftovers: Vec<_> = fs::read_dir(dir.path().join("tmp"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() != "uploads")
            .collect();
        assert!(leftovers.is_empty(), "temporary files left behind");
    }

    #[test]
    fn open_blob_reports_missing_blobs() {
        let (_dir, store) = store();
        let hash = ContentHash::parse(HELLO).unwrap();
        assert!(matches!(
            store.open_blob(&hash),
            Err(StorageError::NotFound)
        ));
        store.put_reader(&b"hello"[..]).unwrap();
        let (mut file, size) = store.open_blob(&hash).unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert_eq!((content.as_str(), size), ("hello", 5));
    }

    #[test]
    fn remove_blob_is_idempotent() {
        let (_dir, store) = store();
        let (hash, _) = store.put_reader(&b"hello"[..]).unwrap();
        store.remove_blob(&hash).unwrap();
        store.remove_blob(&hash).unwrap();
        assert!(!store.contains(&hash));
    }

    #[test]
    fn reports_available_space() {
        let (_dir, store) = store();
        assert!(store.available_space().unwrap() > 0);
    }

    #[test]
    fn no_space_errors_are_recognised() {
        let err: StorageError = io::Error::from(io::ErrorKind::StorageFull).into();
        assert!(matches!(err, StorageError::NoSpace));
    }
}
