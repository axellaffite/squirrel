use std::fs::{self, File};
#[cfg(any(test, feature = "test-util"))]
use std::io::Read;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use squirrel_core::ContentHash;
use uuid::Uuid;

use crate::derived::DerivedStore;
use crate::error::{Result, StorageError};
#[cfg(any(test, feature = "test-util"))]
use crate::fsutil::hash_stream;
use crate::fsutil::{
    create_dir_all_durable, device_of, make_read_only, remove_if_exists, sync_dir,
};

/// Content-addressed blob store rooted at the data directory:
///
/// ```text
/// <data_dir>/blobs/ab/cd/<sha256>   immutable, read-only blobs
/// <data_dir>/tmp/                   in-progress writes (same filesystem, so renames are atomic)
/// <data_dir>/tmp/uploads/<id>       chunked upload sessions
/// <data_dir>/derived/ab/cd/<sha256>/ thumbnails and other derived files (see `DerivedStore`)
/// ```
///
/// Directories are created with mode 0700 and blobs are 0400.
///
/// All methods block: call them from `spawn_blocking`.
#[derive(Debug, Clone)]
pub struct BlobStore {
    root: PathBuf,
    blobs: PathBuf,
    tmp: PathBuf,
    uploads: PathBuf,
    derived: DerivedStore,
}

/// Whether [`BlobStore::adopt`] stored new content or found it already there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Adopted {
    Stored,
    AlreadyPresent,
}

impl BlobStore {
    /// Creates the directory layout if needed (durably) and checks that `tmp/`
    /// and `blobs/` share a filesystem (otherwise `rename` would not be atomic).
    pub fn open(data_dir: &Path) -> Result<Self> {
        let (blobs, tmp, uploads) = (
            data_dir.join("blobs"),
            data_dir.join("tmp"),
            data_dir.join("tmp").join("uploads"),
        );
        for dir in [&blobs, &tmp, &uploads] {
            create_dir_all_durable(dir)?;
        }
        let store = Self {
            root: data_dir.to_path_buf(),
            blobs,
            tmp,
            uploads,
            derived: DerivedStore::open(data_dir)?,
        };
        if device_of(&store.tmp)? != device_of(&store.blobs)? {
            return Err(StorageError::CrossDevice(store.tmp.display().to_string()));
        }
        Ok(store)
    }

    /// Files computed from blobs (thumbnails), under `<data_dir>/derived/`.
    pub fn derived(&self) -> &DerivedStore {
        &self.derived
    }

    pub fn blob_path(&self, hash: &ContentHash) -> PathBuf {
        let (a, b) = hash.shard();
        self.blobs.join(a).join(b).join(hash.as_str())
    }

    /// Test helper: whether a blob file exists (the catalogue is the source of truth).
    #[cfg(any(test, feature = "test-util"))]
    pub fn contains(&self, hash: &ContentHash) -> bool {
        self.blob_path(hash).is_file()
    }

    /// Moves a fully written, verified temporary file into place under `hash`.
    /// Idempotent: if the blob already exists the temporary file is removed.
    ///
    /// The caller must hold the per-blob lock (`squirrel_db::blobs::lock`) so a
    /// concurrent [`BlobStore::remove_blob`] cannot delete the blob between the
    /// existence check and the database row being recorded.
    pub fn adopt(&self, tmp: &Path, hash: &ContentHash) -> Result<Adopted> {
        self.adopt_with(tmp, hash, || File::open(tmp)?.sync_all(), sync_dir)
    }

    /// [`BlobStore::adopt`], with `sync` making the temporary file's content
    /// durable before it is renamed into place and `sync_parent` making the
    /// rename durable. A failure after the rename is [`StorageError::Moved`]
    /// and removes the blob again: no catalogue row may point at a file whose
    /// directory entry might not survive a crash.
    pub(crate) fn adopt_with(
        &self,
        tmp: &Path,
        hash: &ContentHash,
        sync: impl FnOnce() -> io::Result<()>,
        sync_parent: impl FnOnce(&Path) -> io::Result<()>,
    ) -> Result<Adopted> {
        let dest = self.blob_path(hash);
        if dest.is_file() {
            remove_if_exists(tmp)?;
            return Ok(Adopted::AlreadyPresent);
        }
        sync()?;
        make_read_only(tmp)?;
        let dir = dest.parent().expect("blob paths have a parent");
        create_dir_all_durable(dir)?;
        fs::rename(tmp, &dest)?;
        if let Err(err) = sync_parent(dir) {
            // Best effort: a leftover has no row and is swept as an orphan.
            let _ = remove_if_exists(&dest);
            return Err(StorageError::Moved(err));
        }
        Ok(Adopted::Stored)
    }

    /// Stores `bytes` as blob `hash` (the caller computed it), durably.
    /// Idempotent like [`BlobStore::adopt`], and the caller must hold the
    /// per-blob lock for the same reason. For small in-memory content (text
    /// saved by the editor); uploads stream through [`BlobStore::adopt_upload`].
    pub fn put_bytes(&self, bytes: &[u8], hash: &ContentHash) -> Result<Adopted> {
        let tmp = self.tmp.join(format!("put-{}", Uuid::now_v7()));
        let result = (|| {
            let mut file = File::create_new(&tmp)?;
            file.write_all(bytes)?;
            self.adopt_with(&tmp, hash, || file.sync_all(), sync_dir)
        })();
        if result.is_err() {
            let _ = remove_if_exists(&tmp);
        }
        result
    }

    /// Test helper: streams `reader` into a temporary file while hashing it,
    /// syncs it, then adopts it, without the per-blob lock and without a
    /// catalogue row (so the result looks like an orphan to maintenance).
    /// Production uploads go through [`BlobStore::adopt_upload`].
    #[cfg(any(test, feature = "test-util"))]
    pub fn put_reader(&self, reader: impl Read) -> Result<(ContentHash, u64)> {
        let tmp = self.tmp.join(format!("put-{}", Uuid::now_v7()));
        let result = (|| {
            let mut file = File::create_new(&tmp)?;
            let (hash, size) = hash_stream(reader, |block| file.write_all(block))?;
            self.adopt_with(&tmp, &hash, || file.sync_all(), sync_dir)?;
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

    /// Deletes a blob; a missing blob is not an error. The caller must hold the
    /// per-blob lock and have checked that nothing references the blob.
    pub fn remove_blob(&self, hash: &ContentHash) -> Result<()> {
        Ok(remove_if_exists(&self.blob_path(hash))?)
    }

    /// Free bytes available to this process on the data filesystem.
    pub fn available_space(&self) -> Result<u64> {
        let stat = rustix::fs::statvfs(&self.blobs).map_err(io::Error::from)?;
        Ok(stat.f_bavail.saturating_mul(stat.f_frsize))
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// The data directory this store is rooted at.
    pub fn data_dir(&self) -> &Path {
        &self.root
    }

    pub(crate) fn blobs_dir(&self) -> &Path {
        &self.blobs
    }

    pub(crate) fn uploads_dir(&self) -> &Path {
        &self.uploads
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
    fn put_bytes_stores_once_and_cleans_up() {
        let (dir, store) = store();
        let hash = ContentHash::parse(HELLO).unwrap();
        assert_eq!(store.put_bytes(b"hello", &hash).unwrap(), Adopted::Stored);
        assert_eq!(
            store.put_bytes(b"hello", &hash).unwrap(),
            Adopted::AlreadyPresent
        );
        assert_eq!(fs::read(store.blob_path(&hash)).unwrap(), b"hello");
        let leftovers = fs::read_dir(dir.path().join("tmp"))
            .unwrap()
            .filter(|entry| entry.as_ref().unwrap().path().is_file())
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn open_creates_the_layout() {
        let (dir, _store) = store();
        for sub in ["blobs", "tmp", "tmp/uploads", "derived"] {
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
    fn a_failure_after_the_move_is_reported_as_such_and_leaves_no_blob() {
        let (dir, store) = store();
        let hash = ContentHash::parse(HELLO).unwrap();
        let tmp = dir.path().join("tmp/hello");
        fs::write(&tmp, b"hello").unwrap();
        let result = store.adopt_with(
            &tmp,
            &hash,
            || Ok(()),
            |_| Err(io::Error::other("fsync failed")),
        );
        assert!(matches!(result, Err(StorageError::Moved(_))), "{result:?}");
        assert!(!store.contains(&hash), "an unsynced blob must not stay");
        assert!(!tmp.exists());
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

    #[test]
    fn quota_exceeded_errors_are_recognised() {
        let err: StorageError = io::Error::from(io::ErrorKind::QuotaExceeded).into();
        assert!(matches!(err, StorageError::NoSpace));
        let edquot = io::Error::from_raw_os_error(rustix::io::Errno::DQUOT.raw_os_error());
        assert!(matches!(StorageError::from(edquot), StorageError::NoSpace));
    }

    fn temp_leftovers(dir: &Path) -> Vec<fs::DirEntry> {
        fs::read_dir(dir.join("tmp"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() != "uploads")
            .collect()
    }

    #[test]
    fn concurrent_puts_of_the_same_content_both_succeed() {
        let (dir, store) = store();
        let barrier = std::sync::Barrier::new(2);
        let results: Vec<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..2)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        store.put_reader(&b"hello"[..])
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for result in results {
            let (hash, size) = result.unwrap();
            assert_eq!((hash.as_str(), size), (HELLO, 5));
        }
        let hash = ContentHash::parse(HELLO).unwrap();
        assert_eq!(fs::read(store.blob_path(&hash)).unwrap(), b"hello");
        assert!(temp_leftovers(dir.path()).is_empty());
    }

    /// Yields some bytes, then fails.
    struct FailingReader(bool);

    impl Read for FailingReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if std::mem::replace(&mut self.0, true) {
                return Err(io::Error::other("connection reset"));
            }
            buf[..3].copy_from_slice(b"abc");
            Ok(3)
        }
    }

    #[test]
    fn a_reader_failing_midway_leaves_no_temporary_file() {
        let (dir, store) = store();
        assert!(matches!(
            store.put_reader(FailingReader(false)),
            Err(StorageError::Io(_))
        ));
        assert!(temp_leftovers(dir.path()).is_empty());
    }

    /// Yields "hello" with an `Interrupted` error in between.
    struct InterruptedReader(u8);

    impl Read for InterruptedReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0 += 1;
            match self.0 {
                1 => Err(io::ErrorKind::Interrupted.into()),
                2 => {
                    buf[..5].copy_from_slice(b"hello");
                    Ok(5)
                }
                _ => Ok(0),
            }
        }
    }

    #[test]
    fn interrupted_reads_are_retried() {
        let (_dir, store) = store();
        let (hash, _) = store.put_reader(InterruptedReader(0)).unwrap();
        assert_eq!(hash.as_str(), HELLO);
    }

    #[test]
    fn directories_are_private_and_blobs_owner_read_only() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, store) = store();
        let (hash, _) = store.put_reader(&b"hello"[..]).unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        for sub in ["blobs", "tmp", "tmp/uploads", "blobs/2c", "blobs/2c/f2"] {
            assert_eq!(mode(&dir.path().join(sub)), 0o700, "{sub}");
        }
        assert_eq!(mode(&store.blob_path(&hash)), 0o400);
    }
}
