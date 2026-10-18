//! Cooperative document locking across the desktop application and CLI processes.
use crate::model::Result;
use std::{
    fs::{File, OpenOptions},
    path::Path,
};

pub struct FileLock {
    _file: File,
}

impl FileLock {
    /// Lock a stable adjacent file, so atomic replacement does not release the lock.
    /// The file stays on disk; the operating system releases its lock on drop/crash.
    pub fn try_acquire(path: &Path) -> Result<Self> {
        let name = path
            .file_name()
            .ok_or("Choose a file name")?
            .to_string_lossy();
        let lock_path = path.with_file_name(format!(".{name}.lock"));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|e| e.to_string())?;
        file.try_lock().map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => "Document is busy in another process".to_owned(),
            std::fs::TryLockError::Error(error) => error.to_string(),
        })?;
        Ok(Self { _file: file })
    }
}
