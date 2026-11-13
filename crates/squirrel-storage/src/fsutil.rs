use std::fs::{self, File};
use std::io;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;

/// Makes a rename or create in `dir` durable.
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

pub fn device_of(path: &Path) -> io::Result<u64> {
    Ok(fs::metadata(path)?.dev())
}

/// Blobs are immutable once stored.
pub fn make_read_only(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o444))
}

/// Removes a file, treating "already gone" as success.
pub fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}
