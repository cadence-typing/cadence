//! Safe lifetime locks and private creation, without process-global umask changes.

use std::fs::{DirBuilder, File, OpenOptions, TryLockError};
use std::path::Path;

use crate::{StoreError, paths::LOCK_FILE};

pub(crate) fn private_directory(path: &Path) -> Result<(), StoreError> {
    let mut builder = DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|error| StoreError::Open(error.to_string()))
}

fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

pub(crate) fn reserve(path: &Path) -> Result<File, StoreError> {
    options().create_new(true).open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            StoreError::DestinationExists(path.to_path_buf())
        } else {
            StoreError::Open(error.to_string())
        }
    })
}

pub(crate) fn create_database(path: &Path) -> Result<(), StoreError> {
    match reserve(path) {
        Ok(file) => file
            .sync_all()
            .map_err(|error| StoreError::Open(error.to_string())),
        Err(StoreError::DestinationExists(_)) => Ok(()),
        Err(error) => Err(error),
    }
}

pub(crate) struct WriterLock {
    _file: File,
}

impl WriterLock {
    pub(crate) fn acquire(directory: &Path) -> Result<Self, StoreError> {
        let path = directory.join(LOCK_FILE);
        let file = options()
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|error| StoreError::Open(error.to_string()))?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(StoreError::WriterLockHeld { path }),
            Err(TryLockError::Error(error)) => Err(StoreError::Open(error.to_string())),
        }
    }
}

pub(crate) fn sync_parent(path: &Path) -> Result<(), StoreError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|error| StoreError::Open(error.to_string()))
}

pub(crate) fn remove_failed(path: &Path, original: StoreError) -> StoreError {
    match std::fs::remove_file(path) {
        Ok(()) => original,
        Err(error) => StoreError::Open(format!(
            "{original}; removing partial {} failed: {error}",
            path.display()
        )),
    }
}
