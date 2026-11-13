use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::os::unix::fs::FileExt;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use squirrel_core::ContentHash;
use uuid::Uuid;

use crate::error::{Result, StorageError};
use crate::fsutil::remove_if_exists;
use crate::mime::SNIFF_LEN;
use crate::store::{Adopted, BlobStore};

const HASH_BUFFER: usize = 1024 * 1024;

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

    /// Creates the (sparse) file that chunks of upload `id` are written into.
    /// Calling it again for the same id keeps what was already received.
    pub fn create_upload(&self, id: Uuid, size: u64) -> Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.upload_path(id))?;
        file.set_len(size)?;
        Ok(())
    }

    /// Writes one chunk at `offset` and makes it durable before returning, so a
    /// chunk recorded as received in the database is really on disk.
    pub fn write_chunk(&self, id: Uuid, offset: u64, data: &[u8]) -> Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .open(self.upload_path(id))
            .map_err(|err| match err.kind() {
                std::io::ErrorKind::NotFound => StorageError::NotFound,
                _ => err.into(),
            })?;
        file.write_all_at(data, offset)?;
        file.sync_data()?;
        Ok(())
    }

    /// Streams the whole upload file through SHA-256.
    pub fn hash_upload(&self, id: Uuid) -> Result<HashedUpload> {
        let mut file = File::open(self.upload_path(id))?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0; HASH_BUFFER];
        let mut head = Vec::with_capacity(SNIFF_LEN);
        let mut size = 0u64;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            if head.len() < SNIFF_LEN {
                let take = (SNIFF_LEN - head.len()).min(read);
                head.extend_from_slice(&buffer[..take]);
            }
            hasher.update(&buffer[..read]);
            size += read as u64;
        }
        let hash = ContentHash::from_digest(hasher.finalize().into());
        Ok(HashedUpload { hash, size, head })
    }

    /// Moves a verified upload into the blob store (idempotent, see [`BlobStore::adopt`]).
    pub fn adopt_upload(&self, id: Uuid, hash: &ContentHash) -> Result<Adopted> {
        self.adopt(&self.upload_path(id), hash)
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
        assert_eq!(store.adopt_upload(id, &hash).unwrap(), Adopted::Stored);
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
            store.adopt_upload(id, &hash).unwrap(),
            Adopted::AlreadyPresent
        );
        assert!(store.list_uploads().unwrap().is_empty());
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
