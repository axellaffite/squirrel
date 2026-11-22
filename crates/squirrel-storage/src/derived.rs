//! Files computed from a blob (thumbnails now; posters and HLS in Phase 4):
//!
//! ```text
//! <data_dir>/derived/ab/cd/<sha256>/thumb-v1.webp
//! <data_dir>/derived/ab/cd/<sha256>/preview-v1.webp
//! ```
//!
//! Written into a scratch directory under `<data_dir>/tmp/` and renamed into
//! place (same filesystem, checked at open). Keyed by the blob's hash, so
//! deduplicated content shares them. Blocking: call from `spawn_blocking`.

use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use squirrel_core::ContentHash;
use uuid::Uuid;

use crate::error::{Result, StorageError};
use crate::fsutil::{create_dir_all_durable, device_of, make_read_only, sync_dir};

/// Mode of scratch directories: private to the server.
const SCRATCH_MODE: u32 = 0o700;
const EXTENSION: &str = "webp";
/// A variant larger than this is refused (a 1440 px WebP is a few hundred KB).
pub const MAX_VARIANT_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Thumb,
    Preview,
}

impl Variant {
    pub const ALL: [Variant; 2] = [Variant::Thumb, Variant::Preview];

    pub fn stem(self) -> &'static str {
        match self {
            Self::Thumb => "thumb",
            Self::Preview => "preview",
        }
    }

    /// Name of the variant in a scratch directory (what the renderer writes).
    pub fn scratch_name(self) -> String {
        format!("{}.{EXTENSION}", self.stem())
    }

    fn file_name(self, version: i16) -> String {
        format!("{}-v{version}.{EXTENSION}", self.stem())
    }
}

#[derive(Debug, Clone)]
pub struct DerivedStore {
    root: PathBuf,
    tmp: PathBuf,
}

/// A private temporary directory, removed (with its content) when dropped.
#[derive(Debug)]
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

impl DerivedStore {
    /// Creates `derived/` (durably) and checks it shares a filesystem with `tmp/`.
    pub fn open(data_dir: &Path) -> Result<Self> {
        let store = Self {
            root: data_dir.join("derived"),
            tmp: data_dir.join("tmp"),
        };
        create_dir_all_durable(&store.root)?;
        create_dir_all_durable(&store.tmp)?;
        if device_of(&store.root)? != device_of(&store.tmp)? {
            return Err(StorageError::CrossDevice(store.root.display().to_string()));
        }
        Ok(store)
    }

    /// `<data_dir>/derived/ab/cd/<sha256>/`.
    pub fn dir(&self, hash: &ContentHash) -> PathBuf {
        let (a, b) = hash.shard();
        self.root.join(a).join(b).join(hash.as_str())
    }

    pub fn variant_path(&self, hash: &ContentHash, variant: Variant, version: i16) -> PathBuf {
        self.dir(hash).join(variant.file_name(version))
    }

    /// A fresh private directory under `tmp/` for a renderer to write into.
    pub fn scratch(&self) -> Result<Scratch> {
        let path = self.tmp.join(format!("derived-{}", Uuid::now_v7()));
        DirBuilder::new().mode(SCRATCH_MODE).create(&path)?;
        Ok(Scratch(path))
    }

    /// Moves every variant present in `scratch` (`thumb.webp`, `preview.webp`)
    /// into place as `version`, read-only, then removes other versions of the
    /// variants installed.
    ///
    /// Races, both harmless:
    /// - Garbage collection may remove the blob (and its derived directory)
    ///   while a job renders it; this install then recreates the directory for
    ///   a blob that no longer exists. The caller removes it again when it finds
    ///   no media row to record the result in, and failing that the orphan
    ///   sweep does, after the grace period.
    /// - [`DerivedStore::remove_all`] of another hash in the same shard may
    ///   prune the shard directories between their creation here and the
    ///   rename (`ENOENT`): the directories are then created again and the
    ///   rename retried once.
    pub fn install(&self, hash: &ContentHash, version: i16, scratch: &Scratch) -> Result<()> {
        let dir = self.dir(hash);
        create_dir_all_durable(&dir)?;
        for variant in Variant::ALL {
            let source = scratch.path().join(variant.scratch_name());
            if !Self::is_installable(&source)? {
                continue;
            }
            make_read_only(&source)?;
            place(&source, &self.variant_path(hash, variant, version), &dir)?;
            self.remove_other_versions(&dir, variant, version)?;
        }
        sync_dir(&dir)?;
        Ok(())
    }

    /// The renderer (an untrusted child process) wrote the scratch directory:
    /// only a regular file of a sane size is installed, never a symlink (it
    /// would be served as a thumbnail, whatever it points at) or a device.
    fn is_installable(source: &Path) -> Result<bool> {
        let meta = match fs::symlink_metadata(source) {
            Ok(meta) => meta,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(err) => return Err(err.into()),
        };
        if !meta.file_type().is_file() {
            return Err(StorageError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} is not a regular file", source.display()),
            )));
        }
        if meta.len() > MAX_VARIANT_BYTES {
            return Err(StorageError::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} is {} bytes", source.display(), meta.len()),
            )));
        }
        Ok(true)
    }

    fn remove_other_versions(&self, dir: &Path, variant: Variant, keep: i16) -> io::Result<()> {
        let prefix = format!("{}-v", variant.stem());
        let current = variant.file_name(keep);
        for entry in fs::read_dir(dir)? {
            let name = entry?.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with(&prefix) && name != current {
                crate::fsutil::remove_if_exists(&dir.join(name))?;
            }
        }
        Ok(())
    }

    /// Removes every version of `variant` stored for `hash`; how many files
    /// went. Nothing there is not an error.
    pub fn remove_variant(&self, hash: &ContentHash, variant: Variant) -> Result<usize> {
        let dir = self.dir(hash);
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(err) => return Err(err.into()),
        };
        let prefix = format!("{}-v", variant.stem());
        let mut removed = 0;
        for entry in entries {
            let name = entry?.file_name();
            if name.to_str().is_some_and(|name| name.starts_with(&prefix)) {
                crate::fsutil::remove_if_exists(&dir.join(name))?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// The bytes of one installed variant.
    pub fn read(&self, hash: &ContentHash, variant: Variant, version: i16) -> Result<Vec<u8>> {
        fs::read(self.variant_path(hash, variant, version)).map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => StorageError::NotFound,
            _ => err.into(),
        })
    }

    /// Removes everything derived from `hash`; nothing there is not an error.
    /// Empty shard directories are pruned too.
    pub fn remove_all(&self, hash: &ContentHash) -> Result<()> {
        let dir = self.dir(hash);
        match fs::remove_dir_all(&dir) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err.into()),
            _ => {}
        }
        let mut parent = dir.parent();
        while let Some(shard) = parent.filter(|p| *p != self.root) {
            match fs::remove_dir(shard) {
                Ok(()) => {}
                Err(err)
                    if matches!(
                        err.kind(),
                        io::ErrorKind::DirectoryNotEmpty | io::ErrorKind::NotFound
                    ) =>
                {
                    break;
                }
                Err(err) => return Err(err.into()),
            }
            parent = shard.parent();
        }
        Ok(())
    }

    /// Hashes whose derived directory was last modified before `cutoff`.
    /// Entries that are not a hash directory at its sharded path are skipped.
    pub fn list_older_than(&self, cutoff: SystemTime) -> Result<Vec<ContentHash>> {
        let mut found = Vec::new();
        for first in subdirectories(&self.root)? {
            for second in subdirectories(&first)? {
                for dir in subdirectories(&second)? {
                    let Some(hash) = dir
                        .file_name()
                        .and_then(|name| name.to_str())
                        .and_then(|name| ContentHash::parse(name).ok())
                        .filter(|hash| self.dir(hash) == dir)
                    else {
                        continue;
                    };
                    match fs::metadata(&dir) {
                        Ok(meta) if meta.modified()? < cutoff => found.push(hash),
                        Ok(_) => {}
                        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                        Err(err) => return Err(err.into()),
                    }
                }
            }
        }
        Ok(found)
    }
}

/// Renames `source` to `dest` in `dir`, recreating `dir` once if a concurrent
/// shard prune removed it.
fn place(source: &Path, dest: &Path, dir: &Path) -> io::Result<()> {
    match fs::rename(source, dest) {
        Err(err) if err.kind() == io::ErrorKind::NotFound && source.exists() => {
            create_dir_all_durable(dir)?;
            fs::rename(source, dest)
        }
        other => other,
    }
}

fn subdirectories(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let read = match fs::read_dir(dir) {
        Ok(read) => read,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };
    let mut dirs = Vec::new();
    for entry in read {
        let entry = entry?;
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            dirs.push(entry.path());
        }
    }
    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const HASH: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";

    fn store() -> (tempfile::TempDir, DerivedStore) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("tmp")).unwrap();
        let store = DerivedStore::open(dir.path()).unwrap();
        (dir, store)
    }

    fn scratch_with(store: &DerivedStore, files: &[&str]) -> Scratch {
        let scratch = store.scratch().unwrap();
        for name in files {
            fs::write(scratch.path().join(name), name.as_bytes()).unwrap();
        }
        scratch
    }

    #[test]
    fn installs_versioned_variants_read_only_under_the_hash() {
        let (_dir, store) = store();
        let hash = ContentHash::parse(HASH).unwrap();
        store
            .install(
                &hash,
                1,
                &scratch_with(&store, &["thumb.webp", "preview.webp"]),
            )
            .unwrap();
        let path = store.variant_path(&hash, Variant::Thumb, 1);
        assert!(path.ends_with(format!("2c/f2/{HASH}/thumb-v1.webp")));
        assert_eq!(
            store.read(&hash, Variant::Preview, 1).unwrap(),
            b"preview.webp"
        );
        assert!(fs::metadata(&path).unwrap().permissions().readonly());
    }

    #[test]
    fn a_new_version_replaces_the_old_one() {
        let (_dir, store) = store();
        let hash = ContentHash::parse(HASH).unwrap();
        store
            .install(&hash, 1, &scratch_with(&store, &["thumb.webp"]))
            .unwrap();
        store
            .install(&hash, 2, &scratch_with(&store, &["thumb.webp"]))
            .unwrap();
        assert!(matches!(
            store.read(&hash, Variant::Thumb, 1),
            Err(StorageError::NotFound)
        ));
        assert!(store.read(&hash, Variant::Thumb, 2).is_ok());
    }

    #[test]
    fn reinstalling_the_same_version_overwrites_it() {
        let (_dir, store) = store();
        let hash = ContentHash::parse(HASH).unwrap();
        for _ in 0..2 {
            store
                .install(&hash, 1, &scratch_with(&store, &["thumb.webp"]))
                .unwrap();
        }
        assert!(store.read(&hash, Variant::Thumb, 1).is_ok());
    }

    #[test]
    fn one_variant_can_be_removed_in_every_version() {
        let (_dir, store) = store();
        let hash = ContentHash::parse(HASH).unwrap();
        store
            .install(
                &hash,
                1,
                &scratch_with(&store, &["thumb.webp", "preview.webp"]),
            )
            .unwrap();
        assert_eq!(store.remove_variant(&hash, Variant::Preview).unwrap(), 1);
        assert_eq!(store.remove_variant(&hash, Variant::Preview).unwrap(), 0);
        assert!(store.read(&hash, Variant::Thumb, 1).is_ok());
        let gone = ContentHash::parse(&"0".repeat(64)).unwrap();
        assert_eq!(store.remove_variant(&gone, Variant::Preview).unwrap(), 0);
    }

    #[test]
    fn scratch_directories_vanish_when_dropped() {
        let (dir, store) = store();
        let path = store.scratch().unwrap().path().to_owned();
        assert!(!path.exists());
        assert!(path.starts_with(dir.path().join("tmp")));
    }

    #[test]
    fn remove_all_is_idempotent_and_lists_follow() {
        let (dir, store) = store();
        let hash = ContentHash::parse(HASH).unwrap();
        store
            .install(&hash, 1, &scratch_with(&store, &["thumb.webp"]))
            .unwrap();
        let later = SystemTime::now() + Duration::from_secs(60);
        assert_eq!(store.list_older_than(later).unwrap(), vec![hash.clone()]);
        store.remove_all(&hash).unwrap();
        store.remove_all(&hash).unwrap();
        assert!(store.list_older_than(later).unwrap().is_empty());
        assert!(
            !dir.path().join("derived/2c").exists(),
            "empty shards pruned"
        );
        assert!(dir.path().join("derived").is_dir());
    }

    #[test]
    fn only_regular_files_of_a_sane_size_are_installed() {
        let (dir, store) = store();
        let hash = ContentHash::parse(HASH).unwrap();
        let secret = dir.path().join("secret");
        fs::write(&secret, b"not a thumbnail").unwrap();
        let scratch = store.scratch().unwrap();
        std::os::unix::fs::symlink(&secret, scratch.path().join("thumb.webp")).unwrap();
        assert!(store.install(&hash, 1, &scratch).is_err());
        assert!(matches!(
            store.read(&hash, Variant::Thumb, 1),
            Err(StorageError::NotFound)
        ));

        let scratch = store.scratch().unwrap();
        let big = fs::File::create(scratch.path().join("thumb.webp")).unwrap();
        big.set_len(MAX_VARIANT_BYTES + 1).unwrap();
        assert!(store.install(&hash, 1, &scratch).is_err());

        let scratch = store.scratch().unwrap();
        fs::create_dir(scratch.path().join("preview.webp")).unwrap();
        assert!(store.install(&hash, 1, &scratch).is_err());
    }

    #[test]
    fn a_pruned_destination_is_recreated() {
        let (dir, _store) = store();
        let source = dir.path().join("tmp/thumb.webp");
        fs::write(&source, b"x").unwrap();
        let target = dir.path().join("derived/2c/f2").join(HASH);
        place(&source, &target.join("thumb-v1.webp"), &target).unwrap();
        assert!(target.join("thumb-v1.webp").is_file());
    }

    #[test]
    fn stray_entries_are_not_listed() {
        let (dir, store) = store();
        let shard = dir.path().join("derived/2c/f2");
        fs::create_dir_all(shard.join("not-a-hash")).unwrap();
        fs::write(shard.join(HASH), b"a file, not a directory").unwrap();
        let later = SystemTime::now() + Duration::from_secs(60);
        assert!(store.list_older_than(later).unwrap().is_empty());
    }
}
