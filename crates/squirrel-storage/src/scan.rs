//! Listing stored blobs, for reconciling the data directory with the catalogue.

use std::fs;
use std::io;
use std::path::Path;
use std::time::SystemTime;

use squirrel_core::ContentHash;

use crate::error::Result;
use crate::store::BlobStore;

impl BlobStore {
    /// Every blob file last modified before `cutoff`. Entries that are not a
    /// blob at its sharded path (stray names, directories) are skipped; a
    /// shard directory vanishing mid-scan is not an error.
    pub fn list_blobs_older_than(&self, cutoff: SystemTime) -> Result<Vec<ContentHash>> {
        let mut found = Vec::new();
        for first in subdirectories(self.blobs_dir())? {
            for second in subdirectories(&first)? {
                for entry in entries(&second)? {
                    found.extend(self.old_blob(&entry, cutoff)?);
                }
            }
        }
        Ok(found)
    }

    /// Whether any file is stored under `blobs/` (stray names included: the
    /// directory is not empty, so it is not a fresh one).
    pub fn has_blobs(&self) -> Result<bool> {
        for first in subdirectories(self.blobs_dir())? {
            for second in subdirectories(&first)? {
                if entries(&second)?
                    .iter()
                    .any(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// The blob stored at `entry`, if it is one and older than `cutoff`.
    fn old_blob(&self, entry: &fs::DirEntry, cutoff: SystemTime) -> Result<Option<ContentHash>> {
        let Some(hash) = entry
            .file_name()
            .to_str()
            .and_then(|name| ContentHash::parse(name).ok())
            .filter(|hash| self.blob_path(hash) == entry.path())
        else {
            return Ok(None);
        };
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        Ok((metadata.is_file() && metadata.modified()? < cutoff).then_some(hash))
    }
}

fn entries(dir: &Path) -> io::Result<Vec<fs::DirEntry>> {
    match fs::read_dir(dir) {
        Ok(read) => read.collect(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}

fn subdirectories(dir: &Path) -> io::Result<Vec<std::path::PathBuf>> {
    Ok(entries(dir)?
        .into_iter()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn lists_blobs_older_than_the_cutoff_only() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        let (old, _) = store.put_reader(&b"old"[..]).unwrap();
        let later = SystemTime::now() + Duration::from_secs(60);
        assert_eq!(store.list_blobs_older_than(later).unwrap(), [old]);
        let earlier = SystemTime::now() - Duration::from_secs(60);
        assert!(store.list_blobs_older_than(earlier).unwrap().is_empty());
    }

    #[test]
    fn stray_entries_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path()).unwrap();
        let (hash, _) = store.put_reader(&b"hello"[..]).unwrap();
        let shard = store.blob_path(&hash).parent().unwrap().to_owned();
        fs::write(shard.join("notes.txt"), b"x").unwrap();
        // A valid hash in the wrong shard is not a blob of this store.
        let (other, _) = store.put_reader(&b"other"[..]).unwrap();
        fs::copy(store.blob_path(&other), shard.join(other.as_str())).unwrap();
        fs::write(dir.path().join("blobs/README"), b"x").unwrap();
        let later = SystemTime::now() + Duration::from_secs(60);
        let mut listed = store.list_blobs_older_than(later).unwrap();
        listed.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        let mut expected = vec![hash, other];
        expected.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        assert_eq!(listed, expected);
    }
}
