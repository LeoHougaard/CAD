//! Cooperative document locking across the desktop application and CLI processes.
use crate::model::Result;
use std::{
    fs::{File, OpenOptions},
    path::Path,
};

pub struct FileLock {
    file: File,
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
        Ok(Self { file })
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        // A concurrently spawned child can retain the open file description
        // until exec. Release our lock even while that inherited copy exists.
        let _ = self.file.unlock();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn dropping_guard_releases_lock_even_with_a_duplicate_descriptor() {
        let name = format!(
            "cad-lock-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(&name);
        let guard = FileLock::try_acquire(&path).unwrap();
        // A forked child temporarily owns this same open file description.
        let duplicate = guard.file.try_clone().unwrap();
        assert!(FileLock::try_acquire(&path).is_err());
        drop(guard);
        let replacement = FileLock::try_acquire(&path).unwrap();
        drop(duplicate);
        assert!(FileLock::try_acquire(&path).is_err());
        drop(replacement);
        drop(FileLock::try_acquire(&path).unwrap());
        std::fs::remove_file(path.with_file_name(format!(".{name}.lock"))).unwrap();
    }
}
