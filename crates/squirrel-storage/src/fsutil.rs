use std::fs::{self, DirBuilder, File};
use std::io::{self, Read};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::Path;

use sha2::{Digest, Sha256};
use squirrel_core::ContentHash;

/// Mode of every directory the store creates: only the server can list it.
const DIR_MODE: u32 = 0o700;
/// Mode of stored blobs: immutable, readable by the server only.
const BLOB_MODE: u32 = 0o400;
const HASH_BUFFER: usize = 1024 * 1024;

/// Makes a rename or create in `dir` durable.
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

/// Like `create_dir_all` (directories get mode 0700), but also fsyncs the
/// parent of every directory it creates, so the new entries survive a crash.
pub fn create_dir_all_durable(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    if let Some(parent) = dir.parent().filter(|p| !p.as_os_str().is_empty()) {
        create_dir_all_durable(parent)?;
    }
    match DirBuilder::new().mode(DIR_MODE).create(dir) {
        Ok(()) => {}
        // Created concurrently: its creator may not have synced the parent yet.
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists && dir.is_dir() => {}
        Err(err) => return Err(err),
    }
    match dir.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(parent) => sync_dir(parent),
        None => Ok(()),
    }
}

pub fn device_of(path: &Path) -> io::Result<u64> {
    Ok(fs::metadata(path)?.dev())
}

/// Blobs are immutable once stored.
pub fn make_read_only(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(BLOB_MODE))
}

/// Removes a file, treating "already gone" as success.
pub fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Streams `reader` through SHA-256, handing every block to `sink`, and
/// returns the hash and byte count. `Interrupted` reads are retried.
pub fn hash_stream(
    mut reader: impl Read,
    mut sink: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<(ContentHash, u64)> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; HASH_BUFFER];
    let mut size = 0u64;
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        hasher.update(&buffer[..read]);
        sink(&buffer[..read])?;
        size += read as u64;
    }
    Ok((ContentHash::from_digest(hasher.finalize().into()), size))
}
