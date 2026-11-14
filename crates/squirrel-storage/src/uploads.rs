use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::PathBuf;

use squirrel_core::ContentHash;
use uuid::Uuid;

use crate::error::{Result, StorageError};
use crate::fsutil::{hash_stream, remove_if_exists, sync_dir};
use crate::mime::SNIFF_LEN;
use crate::store::{Adopted, BlobStore};

const UPLOAD_FILE_MODE: u32 = 0o600;

/// What [`BlobStore::hash_upload`] learned about a fully received upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashedUpload {
    pub hash: ContentHash,
    pub size: u64,
    /// The first bytes, for MIME sniffing.
    pub head: Vec<u8>,
}

impl BlobStore {
    fn upload_path(&self, id: Uuid) -> PathBuf {
        self.uploads_dir().join(id.to_string())
    }

    /// Opens an existing upload file; a missing one (aborted or discarded
    /// session) is [`StorageError::NotFound`].
    fn open_upload(&self, id: Uuid, write: bool) -> Result<File> {
        OpenOptions::new()
            .read(!write)
            .write(write)
            .open(self.upload_path(id))
            .map_err(|err| match err.kind() {
                io::ErrorKind::NotFound => StorageError::NotFound,
                _ => err.into(),
            })
    }

    /// Creates the (sparse) file that chunks of upload `id` are written into.
    /// Calling it again for the same id keeps what was already received.
    /// `true` when the file did not exist (so no chunk is on disk).
    pub fn create_upload(&self, id: Uuid, size: u64) -> Result<bool> {
        let created = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(UPLOAD_FILE_MODE)
            .open(self.upload_path(id));
        let file = match created {
            Ok(file) => {
                file.set_len(size)?;
                file.sync_all()?;
                sync_dir(self.uploads_dir())?;
                return Ok(true);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => self.open_upload(id, true)?,
            Err(err) => return Err(err.into()),
        };
        file.set_len(size)?;
        Ok(false)
    }

    /// Writes one chunk at `offset` and makes it durable before returning, so a
    /// chunk recorded as received in the database is really on disk. A chunk
    /// reaching past the size given to [`BlobStore::create_upload`] is refused.
    ///
    /// The file is opened by path for this one write and closed before
    /// returning: the API holds the upload's in-process read lock around this
    /// call, and completion its write lock, so no write can land between
    /// verification and adoption.
    pub fn write_chunk(&self, id: Uuid, offset: u64, data: &[u8]) -> Result<()> {
        let file = self.open_upload(id, true)?;
        let planned = file.metadata()?.len();
        let end = u64::try_from(data.len())
            .ok()
            .and_then(|len| offset.checked_add(len));
        if end.is_none_or(|end| end > planned) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("chunk at offset {offset} overruns the {planned}-byte upload"),
            )
            .into());
        }
        file.write_all_at(data, offset)?;
        file.sync_data()?;
        Ok(())
    }

    /// Streams the whole upload file through SHA-256. A missing upload file is
    /// [`StorageError::NotFound`]: the session is gone and cannot be retried.
    pub fn hash_upload(&self, id: Uuid) -> Result<HashedUpload> {
        let file = self.open_upload(id, false)?;
        let mut head = Vec::with_capacity(SNIFF_LEN);
        let (hash, size) = hash_stream(file, |block| {
            let take = (SNIFF_LEN - head.len()).min(block.len());
            head.extend_from_slice(&block[..take]);
            Ok(())
        })?;
        Ok(HashedUpload { hash, size, head })
    }

    /// Moves a verified upload into the blob store (idempotent, see
    /// [`BlobStore::adopt`]) and checks that the blob has the verified `size`.
    /// The caller must hold the per-blob lock. No writable descriptor is kept:
    /// adoption only syncs through a read-only handle, then renames.
    ///
    /// A blob of another size is refused; if this call stored it, it is removed.
    /// Every failure once the upload file has left the uploads area is
    /// [`StorageError::Moved`]: the upload cannot be completed again.
    pub fn adopt_upload(&self, id: Uuid, hash: &ContentHash, size: u64) -> Result<Adopted> {
        let adopted = self.adopt(&self.upload_path(id), hash)?;
        self.check_adopted_size(hash, size, adopted)
            .map_err(StorageError::Moved)?;
        Ok(adopted)
    }

    fn check_adopted_size(
        &self,
        hash: &ContentHash,
        size: u64,
        adopted: Adopted,
    ) -> io::Result<()> {
        let stored = fs::metadata(self.blob_path(hash))?.len();
        if stored == size {
            return Ok(());
        }
        if adopted == Adopted::Stored {
            remove_if_exists(&self.blob_path(hash))?;
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("blob {hash} has {stored} bytes, expected {size}"),
        ))
    }

    pub fn discard_upload(&self, id: Uuid) -> Result<()> {
        Ok(remove_if_exists(&self.upload_path(id))?)
    }

    /// Ids of every upload file on disk, to find files whose session is gone.
    pub fn list_uploads(&self) -> Result<Vec<Uuid>> {
        let mut ids = Vec::new();
        for entry in fs::read_dir(self.uploads_dir())? {
            let name = entry?.file_name();
            if let Some(id) = name.to_str().and_then(|n| Uuid::parse_str(n).ok()) {
                ids.push(id);
            }
        }
        Ok(ids)
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
    fn chunks_written_in_any_order_reassemble_in_place() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 5).unwrap();
        store.write_chunk(id, 4, b"o").unwrap();
        store.write_chunk(id, 0, b"hell").unwrap();
        let hashed = store.hash_upload(id).unwrap();
        assert_eq!(hashed.hash.as_str(), HELLO);
        assert_eq!(hashed.size, 5);
        assert_eq!(hashed.head, b"hello");
    }

    #[test]
    fn rewriting_a_chunk_is_idempotent() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 5).unwrap();
        store.write_chunk(id, 0, b"hell").unwrap();
        store.write_chunk(id, 0, b"hell").unwrap();
        store.write_chunk(id, 4, b"o").unwrap();
        assert_eq!(store.hash_upload(id).unwrap().hash.as_str(), HELLO);
    }

    #[test]
    fn recreating_an_upload_keeps_received_bytes() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 5).unwrap();
        store.write_chunk(id, 0, b"hell").unwrap();
        store.create_upload(id, 5).unwrap();
        store.write_chunk(id, 4, b"o").unwrap();
        assert_eq!(store.hash_upload(id).unwrap().hash.as_str(), HELLO);
    }

    #[test]
    fn adopting_moves_the_upload_into_the_store() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 5).unwrap();
        store.write_chunk(id, 0, b"hello").unwrap();
        let hash = store.hash_upload(id).unwrap().hash;
        assert_eq!(store.adopt_upload(id, &hash, 5).unwrap(), Adopted::Stored);
        assert!(store.contains(&hash));
        assert!(store.list_uploads().unwrap().is_empty());
    }

    #[test]
    fn adopting_existing_content_discards_the_upload() {
        let (_dir, store) = store();
        store.put_reader(&b"hello"[..]).unwrap();
        let id = Uuid::now_v7();
        store.create_upload(id, 5).unwrap();
        store.write_chunk(id, 0, b"hello").unwrap();
        let hash = store.hash_upload(id).unwrap().hash;
        assert_eq!(
            store.adopt_upload(id, &hash, 5).unwrap(),
            Adopted::AlreadyPresent
        );
        assert!(store.list_uploads().unwrap().is_empty());
    }

    #[test]
    fn creating_an_upload_reports_whether_the_file_is_new() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        assert!(store.create_upload(id, 5).unwrap());
        assert!(!store.create_upload(id, 5).unwrap());
        store.discard_upload(id).unwrap();
        assert!(store.create_upload(id, 5).unwrap());
    }

    #[test]
    fn adopting_refuses_a_file_of_the_wrong_size() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 5).unwrap();
        store.write_chunk(id, 0, b"hello").unwrap();
        let hash = store.hash_upload(id).unwrap().hash;
        assert!(matches!(
            store.adopt_upload(id, &hash, 6),
            Err(StorageError::Moved(_))
        ));
        assert!(!store.contains(&hash), "a wrong-sized blob must not stay");
    }

    #[test]
    fn lists_and_discards_uploads() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 1).unwrap();
        assert_eq!(store.list_uploads().unwrap(), vec![id]);
        store.discard_upload(id).unwrap();
        store.discard_upload(id).unwrap();
        assert!(store.list_uploads().unwrap().is_empty());
    }

    #[test]
    fn hashing_a_missing_upload_is_not_found() {
        let (_dir, store) = store();
        assert!(matches!(
            store.hash_upload(Uuid::now_v7()),
            Err(StorageError::NotFound)
        ));
    }

    #[test]
    fn chunks_past_the_planned_size_are_refused() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 5).unwrap();
        assert!(store.write_chunk(id, 4, b"oo").is_err());
        assert!(store.write_chunk(id, u64::MAX, b"x").is_err());
        assert!(store.write_chunk(id, 5, b"x").is_err());
        store.write_chunk(id, 5, b"").unwrap();
        assert_eq!(store.hash_upload(id).unwrap().size, 5);
    }

    #[test]
    fn empty_uploads_hash_and_adopt() {
        const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 0).unwrap();
        let hashed = store.hash_upload(id).unwrap();
        assert_eq!(
            (hashed.hash.as_str(), hashed.size, hashed.head.len()),
            (EMPTY, 0, 0)
        );
        assert_eq!(
            store.adopt_upload(id, &hashed.hash, 0).unwrap(),
            Adopted::Stored
        );
        assert!(store.contains(&hashed.hash));
        assert!(store.list_uploads().unwrap().is_empty());
    }

    #[test]
    fn writing_to_a_discarded_upload_is_not_found() {
        let (_dir, store) = store();
        let id = Uuid::now_v7();
        store.create_upload(id, 1).unwrap();
        store.discard_upload(id).unwrap();
        assert!(matches!(
            store.write_chunk(id, 0, b"x"),
            Err(StorageError::NotFound)
        ));
    }
}
