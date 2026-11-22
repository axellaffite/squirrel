//! Lightbox previews rendered on demand, kept on disk under a size budget:
//!
//! ```text
//! <data_dir>/cache/previews/ab/<sha256>-<bucket>-r<version>.{jpg,webp}
//! ```
//!
//! Least recently used entries go first once the budget is exceeded (down to
//! 90 % of it). The in-memory index is rebuilt from the directory at start;
//! last use is written back to a file's mtime at most hourly, so the order
//! survives restarts. Entries come from a renderer's scratch directory under
//! `<data_dir>/tmp/` (same filesystem, checked at open) and are renamed into
//! place read-only. Not counted in quotas. Blocking: call from `spawn_blocking`.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use squirrel_core::ContentHash;

use crate::error::{Result, StorageError};
use crate::fsutil::{create_dir_all_durable, device_of, make_read_only, remove_if_exists};

/// A cached preview larger than this is refused (a 3200 px JPEG is ≈ 1 MB).
pub const MAX_ENTRY_BYTES: u64 = 5 * 1024 * 1024;
/// Last use is written back to disk at most this often per entry.
const TOUCH_EVERY: Duration = Duration::from_secs(60 * 60);
/// Eviction stops at this share of the budget, so it runs in batches.
const TRIM_TO_PERCENT: u64 = 90;
/// The file extensions an entry may have.
pub const EXTENSIONS: [&str; 2] = ["jpg", "webp"];

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub hash: ContentHash,
    /// The preview's long side bucket (1440, 2160, 3200).
    pub bucket: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct PreviewCacheConfig {
    /// The renderer's version: entries of any other version are purged at open.
    pub version: i16,
    /// Budget for all entries together.
    pub max_bytes: u64,
    /// Nothing is written when it would leave less than this free on the disk.
    pub min_free_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    pub entries: usize,
    pub bytes: u64,
    pub max_bytes: u64,
}

#[derive(Debug, Clone)]
struct Entry {
    extension: &'static str,
    size: u64,
    used: SystemTime,
    /// When `used` was last written to the file's mtime.
    touched: SystemTime,
}

#[derive(Debug, Default)]
struct Index {
    entries: HashMap<CacheKey, Entry>,
    bytes: u64,
}

impl Index {
    fn insert(&mut self, key: CacheKey, entry: Entry) -> Option<Entry> {
        self.bytes += entry.size;
        let previous = self.entries.insert(key, entry);
        if let Some(old) = &previous {
            self.bytes -= old.size;
        }
        previous
    }

    fn remove(&mut self, key: &CacheKey) -> Option<Entry> {
        let removed = self.entries.remove(key);
        if let Some(entry) = &removed {
            self.bytes -= entry.size;
        }
        removed
    }
}

#[derive(Debug)]
pub struct PreviewCache {
    root: PathBuf,
    config: PreviewCacheConfig,
    index: Mutex<Index>,
}

impl PreviewCache {
    /// Creates `cache/previews/` (durably), checks it shares a filesystem with
    /// `tmp/`, then rebuilds the index: files of another version, of an
    /// unknown name or that are not regular files of a sane size are removed.
    pub fn open(data_dir: &Path, config: PreviewCacheConfig) -> Result<Self> {
        let root = data_dir.join("cache").join("previews");
        create_dir_all_durable(&root)?;
        let tmp = data_dir.join("tmp");
        create_dir_all_durable(&tmp)?;
        if device_of(&root)? != device_of(&tmp)? {
            return Err(StorageError::CrossDevice(root.display().to_string()));
        }
        let cache = Self {
            root,
            config,
            index: Mutex::default(),
        };
        let index = cache.scan()?;
        *cache.lock() = index;
        cache.trim(&mut cache.lock())?;
        Ok(cache)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Index> {
        self.index.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn config(&self) -> PreviewCacheConfig {
        self.config
    }

    fn shard(&self, hash: &ContentHash) -> PathBuf {
        self.root.join(&hash.as_str()[..2])
    }

    fn file_name(&self, key: &CacheKey, extension: &str) -> String {
        format!(
            "{}-{}-r{}.{extension}",
            key.hash.as_str(),
            key.bucket,
            self.config.version
        )
    }

    fn path(&self, key: &CacheKey, extension: &str) -> PathBuf {
        self.shard(&key.hash).join(self.file_name(key, extension))
    }

    /// `(key, extension)` of a file name of the current version.
    fn parse(&self, name: &str) -> Option<(CacheKey, &'static str)> {
        let (stem, extension) = name.rsplit_once('.')?;
        let extension = EXTENSIONS.into_iter().find(|e| *e == extension)?;
        let mut parts = stem.splitn(3, '-');
        let hash = ContentHash::parse(parts.next()?).ok()?;
        let bucket = parts.next()?.parse().ok()?;
        let version: i16 = parts.next()?.strip_prefix('r')?.parse().ok()?;
        (version == self.config.version).then_some((CacheKey { hash, bucket }, extension))
    }

    fn scan(&self) -> Result<Index> {
        let mut index = Index::default();
        for shard in fs::read_dir(&self.root)? {
            let shard = shard?;
            if !shard.file_type()?.is_dir() {
                remove_if_exists(&shard.path())?;
                continue;
            }
            for file in fs::read_dir(shard.path())? {
                let file = file?;
                let path = file.path();
                let meta = fs::symlink_metadata(&path)?;
                let parsed = file.file_name().to_str().and_then(|name| self.parse(name));
                let valid = meta.file_type().is_file()
                    && meta.len() <= MAX_ENTRY_BYTES
                    && parsed
                        .as_ref()
                        .is_some_and(|(key, _)| self.shard(&key.hash) == shard.path());
                match parsed {
                    Some((key, extension)) if valid => {
                        let used = meta.modified()?;
                        let entry = Entry {
                            extension,
                            size: meta.len(),
                            used,
                            touched: used,
                        };
                        if let Some(old) = index.insert(key.clone(), entry) {
                            // The same key in both formats: keep the newer.
                            self.keep_newer(&mut index, key, old)?;
                        }
                    }
                    _ if meta.is_dir() => fs::remove_dir_all(&path)?,
                    _ => remove_if_exists(&path)?,
                }
            }
        }
        Ok(index)
    }

    fn keep_newer(&self, index: &mut Index, key: CacheKey, old: Entry) -> Result<()> {
        let current = index.entries[&key].clone();
        let (keep, drop) = if old.used > current.used {
            (old, current)
        } else {
            (current, old)
        };
        remove_if_exists(&self.path(&key, drop.extension))?;
        index.remove(&key);
        index.insert(key, keep);
        Ok(())
    }

    /// The bytes and extension of a cached preview, marking it used. A file
    /// gone from disk (an admin `clear`) is forgotten.
    pub fn read(&self, key: &CacheKey) -> Result<Option<(Vec<u8>, &'static str)>> {
        let Some(entry) = self.lock().entries.get(key).cloned() else {
            return Ok(None);
        };
        let path = self.path(key, entry.extension);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                self.lock().remove(key);
                return Ok(None);
            }
            Err(err) => return Err(err.into()),
        };
        self.mark_used(key, &path)?;
        Ok(Some((bytes, entry.extension)))
    }

    fn mark_used(&self, key: &CacheKey, path: &Path) -> Result<()> {
        let now = SystemTime::now();
        let touch = {
            let mut index = self.lock();
            let Some(entry) = index.entries.get_mut(key) else {
                return Ok(());
            };
            entry.used = now;
            let due = now
                .duration_since(entry.touched)
                .is_ok_and(|age| age >= TOUCH_EVERY);
            if due {
                entry.touched = now;
            }
            due
        };
        if touch {
            // The owner may set times through a read-only descriptor.
            match File::open(path).and_then(|file| file.set_modified(now)) {
                Ok(()) => {}
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(err.into()),
            }
        }
        Ok(())
    }

    /// Moves `source` (a renderer's output in `<data_dir>/tmp/`) into the
    /// cache as `key`, read-only, then evicts down to budget. `Ok(false)`:
    /// not cached because the disk is nearly full (`free_bytes` now); the
    /// caller still owns `source`. A symlink, a non-file or an oversized file
    /// is refused (`InvalidData`): the renderer is untrusted.
    pub fn insert(
        &self,
        key: &CacheKey,
        source: &Path,
        extension: &str,
        free_bytes: u64,
    ) -> Result<bool> {
        let extension = EXTENSIONS
            .into_iter()
            .find(|e| *e == extension)
            .ok_or_else(|| invalid(format!("unknown preview extension {extension}")))?;
        let meta = checked(source)?;
        if free_bytes < self.config.min_free_bytes.saturating_add(meta.len()) {
            return Ok(false);
        }
        make_read_only(source)?;
        let shard = self.shard(&key.hash);
        create_dir_all_durable(&shard)?;
        let dest = self.path(key, extension);
        let now = SystemTime::now();
        let mut index = self.lock();
        fs::rename(source, &dest)?;
        let entry = Entry {
            extension,
            size: meta.len(),
            used: now,
            touched: now,
        };
        if let Some(old) = index.insert(key.clone(), entry)
            && old.extension != extension
        {
            remove_if_exists(&self.path(key, old.extension))?;
        }
        self.trim(&mut index)?;
        Ok(true)
    }

    /// Evicts least recently used entries down to 90 % of the budget, once over it.
    fn trim(&self, index: &mut Index) -> Result<()> {
        if index.bytes <= self.config.max_bytes {
            return Ok(());
        }
        let target = self.config.max_bytes / 100 * TRIM_TO_PERCENT;
        let mut by_age: Vec<(SystemTime, CacheKey)> = index
            .entries
            .iter()
            .map(|(key, entry)| (entry.used, key.clone()))
            .collect();
        by_age.sort_unstable_by_key(|(used, _)| *used);
        for (_, key) in by_age {
            if index.bytes <= target {
                break;
            }
            if let Some(entry) = index.remove(&key) {
                remove_if_exists(&self.path(&key, entry.extension))?;
            }
        }
        Ok(())
    }

    /// Removes every cached preview of `hash` (its blob was collected),
    /// including files another process added. Returns how many.
    pub fn remove_hash(&self, hash: &ContentHash) -> Result<usize> {
        let mut index = self.lock();
        index.entries.retain(|key, _| key.hash != *hash);
        index.bytes = index.entries.values().map(|entry| entry.size).sum();
        let prefix = format!("{}-", hash.as_str());
        let entries = match fs::read_dir(self.shard(hash)) {
            Ok(entries) => entries,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(0),
            Err(err) => return Err(err.into()),
        };
        let mut removed = 0;
        for file in entries {
            let file = file?;
            if file
                .file_name()
                .to_str()
                .is_some_and(|n| n.starts_with(&prefix))
            {
                remove_if_exists(&file.path())?;
                removed += 1;
            }
        }
        Ok(removed)
    }

    /// Removes every entry. Returns how many files were removed.
    pub fn clear(&self) -> Result<usize> {
        let mut index = self.lock();
        let mut removed = 0;
        for shard in fs::read_dir(&self.root)? {
            let path = shard?.path();
            if path.is_dir() {
                removed += fs::read_dir(&path)?.count();
                fs::remove_dir_all(&path)?;
            } else {
                remove_if_exists(&path)?;
            }
        }
        *index = Index::default();
        Ok(removed)
    }

    pub fn stats(&self) -> CacheStats {
        let index = self.lock();
        CacheStats {
            entries: index.entries.len(),
            bytes: index.bytes,
            max_bytes: self.config.max_bytes,
        }
    }
}

/// The metadata of a renderer's output, refused unless it is a regular file
/// (never a symlink) of a sane size.
fn checked(source: &Path) -> Result<fs::Metadata> {
    let meta = fs::symlink_metadata(source)?;
    if !meta.file_type().is_file() {
        return Err(invalid(format!(
            "{} is not a regular file",
            source.display()
        )));
    }
    if meta.len() > MAX_ENTRY_BYTES {
        return Err(invalid(format!(
            "{} is {} bytes",
            source.display(),
            meta.len()
        )));
    }
    Ok(meta)
}

/// Checks a renderer's output (an exited sandbox child's) like
/// [`PreviewCache::insert`] does, without reading it.
pub fn check_rendered(source: &Path) -> Result<()> {
    checked(source).map(|_| ())
}

/// The bytes of a renderer's output (an exited sandbox child's), checked like
/// [`PreviewCache::insert`] does.
pub fn read_rendered(source: &Path) -> Result<Vec<u8>> {
    checked(source)?;
    Ok(fs::read(source)?)
}

fn invalid(message: String) -> StorageError {
    StorageError::Io(io::Error::new(io::ErrorKind::InvalidData, message))
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
    const B: &str = "486ea46224d1bb4fb680f34f7c9ad96a8f24ec88be73ea8e5a6c65260e9cb8a7";

    fn config(max_bytes: u64) -> PreviewCacheConfig {
        PreviewCacheConfig {
            version: 1,
            max_bytes,
            min_free_bytes: 0,
        }
    }

    fn key(hash: &str, bucket: u32) -> CacheKey {
        CacheKey {
            hash: ContentHash::parse(hash).unwrap(),
            bucket,
        }
    }

    /// Writes `size` bytes into `<dir>/tmp/` and caches them as `key`.
    fn put(dir: &Path, cache: &PreviewCache, key: &CacheKey, size: usize) -> bool {
        let source = dir.join("tmp").join(format!("render-{}", key.bucket));
        fs::write(&source, vec![7u8; size]).unwrap();
        cache.insert(key, &source, "jpg", u64::MAX).unwrap()
    }

    #[test]
    fn entries_are_read_only_under_a_versioned_name() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::open(dir.path(), config(1000)).unwrap();
        let k = key(A, 2160);
        assert!(put(dir.path(), &cache, &k, 10));
        let path = dir
            .path()
            .join(format!("cache/previews/2c/{A}-2160-r1.jpg"));
        assert!(fs::metadata(&path).unwrap().permissions().readonly());
        let (bytes, extension) = cache.read(&k).unwrap().unwrap();
        assert_eq!((bytes.len(), extension), (10, "jpg"));
        assert!(cache.read(&key(A, 1440)).unwrap().is_none());
        assert_eq!(cache.stats().bytes, 10);
    }

    #[test]
    fn the_least_recently_used_go_first_down_to_ninety_percent() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::open(dir.path(), config(100)).unwrap();
        let (old, used, new) = (key(A, 1440), key(A, 2160), key(B, 1440));
        put(dir.path(), &cache, &old, 40);
        put(dir.path(), &cache, &used, 40);
        std::thread::sleep(Duration::from_millis(5));
        cache.read(&old).unwrap().unwrap();
        // 120 > 100: evicts `used` (oldest use), leaving 80 ≤ 90.
        put(dir.path(), &cache, &new, 40);
        assert!(cache.read(&used).unwrap().is_none());
        assert!(cache.read(&old).unwrap().is_some());
        assert!(cache.read(&new).unwrap().is_some());
        assert_eq!(cache.stats().bytes, 80);
    }

    #[test]
    fn the_index_is_rebuilt_at_start_and_other_versions_purged() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::open(dir.path(), config(1000)).unwrap();
        put(dir.path(), &cache, &key(A, 1440), 10);
        put(dir.path(), &cache, &key(B, 3200), 20);
        let shard = dir.path().join("cache/previews/2c");
        fs::write(shard.join(format!("{A}-1440-r0.jpg")), b"old").unwrap();
        fs::write(shard.join("stray.txt"), b"?").unwrap();
        drop(cache);
        let cache = PreviewCache::open(dir.path(), config(1000)).unwrap();
        assert_eq!(
            cache.stats(),
            CacheStats {
                entries: 2,
                bytes: 30,
                max_bytes: 1000
            }
        );
        assert!(!shard.join(format!("{A}-1440-r0.jpg")).exists());
        assert!(!shard.join("stray.txt").exists());
        assert!(cache.read(&key(B, 3200)).unwrap().is_some());
    }

    #[test]
    fn symlinks_and_oversized_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::open(dir.path(), config(u64::MAX)).unwrap();
        let secret = dir.path().join("secret");
        fs::write(&secret, b"not a preview").unwrap();
        let link = dir.path().join("tmp/link.jpg");
        std::os::unix::fs::symlink(&secret, &link).unwrap();
        assert!(cache.insert(&key(A, 1440), &link, "jpg", u64::MAX).is_err());
        let big = dir.path().join("tmp/big.jpg");
        File::create(&big)
            .unwrap()
            .set_len(MAX_ENTRY_BYTES + 1)
            .unwrap();
        assert!(cache.insert(&key(A, 1440), &big, "jpg", u64::MAX).is_err());
        assert_eq!(cache.stats().entries, 0);

        // A symlink planted in the cache directory is dropped at start.
        let shard = dir.path().join("cache/previews/2c");
        fs::create_dir_all(&shard).unwrap();
        std::os::unix::fs::symlink(&secret, shard.join(format!("{A}-1440-r1.jpg"))).unwrap();
        let cache = PreviewCache::open(dir.path(), config(u64::MAX)).unwrap();
        assert!(cache.read(&key(A, 1440)).unwrap().is_none());
        assert!(secret.exists());
    }

    #[test]
    fn nothing_is_cached_when_the_disk_is_nearly_full() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::open(
            dir.path(),
            PreviewCacheConfig {
                min_free_bytes: 1000,
                ..config(u64::MAX)
            },
        )
        .unwrap();
        let source = dir.path().join("tmp/render.jpg");
        fs::write(&source, [1u8; 10]).unwrap();
        assert!(!cache.insert(&key(A, 1440), &source, "jpg", 1005).unwrap());
        assert!(source.exists(), "left to the caller");
        assert!(cache.insert(&key(A, 1440), &source, "jpg", 1010).unwrap());
    }

    #[test]
    fn removing_a_hash_drops_all_its_buckets() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::open(dir.path(), config(u64::MAX)).unwrap();
        put(dir.path(), &cache, &key(A, 1440), 10);
        put(dir.path(), &cache, &key(A, 3200), 10);
        put(dir.path(), &cache, &key(B, 1440), 10);
        let hash = ContentHash::parse(A).unwrap();
        assert_eq!(cache.remove_hash(&hash).unwrap(), 2);
        assert!(cache.read(&key(A, 3200)).unwrap().is_none());
        assert_eq!(cache.stats().entries, 1);
        assert_eq!(cache.clear().unwrap(), 1);
        assert_eq!(cache.stats().entries, 0);
        assert!(cache.read(&key(B, 1440)).unwrap().is_none());
    }

    #[test]
    fn a_file_removed_behind_its_back_is_a_miss() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::open(dir.path(), config(u64::MAX)).unwrap();
        let k = key(A, 1440);
        put(dir.path(), &cache, &k, 10);
        fs::remove_file(
            dir.path()
                .join(format!("cache/previews/2c/{A}-1440-r1.jpg")),
        )
        .unwrap();
        assert!(cache.read(&k).unwrap().is_none());
        assert_eq!(cache.stats().entries, 0);
    }
}
