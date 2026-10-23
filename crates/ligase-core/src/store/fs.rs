//! The store as a directory of files: the local tier, and the default.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::hash::ContentHash;

use super::{BlobStore, ObjectKey, StoreError};

/// The store as a directory of files.
///
/// The default and, until M14, the only backend. Objects are fanned out over
/// the first byte of the hash: a flat directory of one object per step per
/// sample is a directory no filesystem enjoys.
#[derive(Clone, Debug)]
pub struct FsStore {
    root: PathBuf,
}

impl FsStore {
    /// Open the store at `root`, creating it if it is not there.
    ///
    /// # Errors
    ///
    /// If the directory cannot be created.
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        create_dir(root)?;
        Ok(Self {
            root: root.to_path_buf(),
        })
    }

    /// The directory the objects live in.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where an object with this key lives, whether or not it is there yet.
    #[must_use]
    pub fn path_of(&self, key: &ObjectKey) -> PathBuf {
        let hex = key.hash.to_hex();
        self.root
            .join(&hex[..2])
            .join(format!("{}{}", &hex[2..], key.suffix))
    }

    /// Move a staged file to its place, read-only, leaving an object already
    /// there alone.
    fn install(&self, staged: &Path, key: &ObjectKey) -> Result<(), StoreError> {
        let path = self.path_of(key);
        if path.is_file() {
            return remove(staged);
        }
        if let Some(parent) = path.parent() {
            create_dir(parent)?;
        }
        move_file(staged, &path)?;
        make_read_only(&path)
    }
}

impl BlobStore for FsStore {
    fn has(&self, key: &ObjectKey) -> Result<bool, StoreError> {
        Ok(self.path_of(key).is_file())
    }

    fn get(&self, key: &ObjectKey) -> Result<Option<Vec<u8>>, StoreError> {
        let path = self.path_of(key);
        match fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(StoreError::Io { path, source }),
        }
    }

    fn put(&self, key: &ObjectKey, bytes: &[u8]) -> Result<(), StoreError> {
        if self.has(key)? {
            return Ok(());
        }
        // Written beside the store and moved in, so a crash mid-write
        // cannot leave a partial object wearing a whole object's name.
        let staged = self.root.join(format!(".partial-{key}"));
        if let Some(parent) = staged.parent() {
            create_dir(parent)?;
        }
        fs::write(&staged, bytes).map_err(|source| StoreError::Io {
            path: staged.clone(),
            source,
        })?;
        self.install(&staged, key)
    }

    fn put_file(&self, staged: &Path, suffix: &str) -> Result<ObjectKey, StoreError> {
        let hash = ContentHash::of_file(staged).map_err(|source| StoreError::Io {
            path: staged.to_path_buf(),
            source,
        })?;
        let key = ObjectKey::new(hash, suffix);
        self.install(staged, &key)?;
        Ok(key)
    }

    fn put_file_as(&self, staged: &Path, key: &ObjectKey) -> Result<(), StoreError> {
        let found = ContentHash::of_file(staged).map_err(|source| StoreError::Io {
            path: staged.to_path_buf(),
            source,
        })?;
        if found != key.hash {
            return Err(StoreError::Mismatch {
                expected: key.hash,
                found,
                at: staged.to_path_buf(),
            });
        }
        self.install(staged, key)
    }

    fn local_path(&self, key: &ObjectKey) -> Result<Option<PathBuf>, StoreError> {
        let path = self.path_of(key);
        Ok(path.is_file().then_some(path))
    }
}

fn create_dir(path: &Path) -> Result<(), StoreError> {
    fs::create_dir_all(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn remove(path: &Path) -> Result<(), StoreError> {
    fs::remove_file(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn move_file(from: &Path, to: &Path) -> Result<(), StoreError> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        // A rename across filesystems fails; copying is the fallback, and
        // the only cost is the copy.
        Err(_) => {
            fs::copy(from, to).map_err(|source| StoreError::Io {
                path: to.to_path_buf(),
                source,
            })?;
            remove(from)
        }
    }
}

fn make_read_only(path: &Path) -> Result<(), StoreError> {
    let metadata = fs::metadata(path).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut permissions = metadata.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).map_err(|source| StoreError::Io {
        path: path.to_path_buf(),
        source,
    })
}
