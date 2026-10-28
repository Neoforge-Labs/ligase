//! The store as a directory of files: the local tier, and the default.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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

    /// Every key this store holds.
    ///
    /// For `ligase store status` and `ligase store push`. Two levels deep,
    /// which is the whole of the layout, and a name that cannot be read
    /// back as a key is skipped rather than guessed at — a partial file
    /// from an interrupted write is debris, not an object.
    ///
    /// # Errors
    ///
    /// If the store directory cannot be read.
    pub fn keys(&self) -> Result<Vec<ObjectKey>, StoreError> {
        let mut keys = Vec::new();
        for shard in read_dir(&self.root)? {
            let Some(prefix) = shard
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
            else {
                continue;
            };
            if prefix.len() != 2 || !shard.is_dir() {
                continue;
            }
            for object in read_dir(&shard)? {
                let Some(name) = object
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                else {
                    continue;
                };
                let (rest, suffix) = match name.find('.') {
                    Some(at) => (&name[..at], &name[at..]),
                    None => (name.as_str(), ""),
                };
                if let Some(hash) = ContentHash::parse(&format!("{prefix}{rest}")) {
                    keys.push(ObjectKey::new(hash, suffix));
                }
            }
        }
        keys.sort_unstable();
        Ok(keys)
    }

    /// Move a staged file to its place, read-only, leaving an object already
    /// there alone.
    /// Move a staged file in, if and only if nothing is there already.
    ///
    /// # Why this is a hard link and not a rename
    ///
    /// `rename` *replaces*, so "check, then rename" is two steps with a
    /// window between them: two writers both find the slot empty and the
    /// second one's rename overwrites the first's object. M14 made the
    /// staged *names* unique, which stopped the writers tripping over each
    /// other; it did not make the install atomic, and nothing noticed
    /// because for an object the race is invisible — the name is the hash
    /// of the contents, so the loser's bytes and the winner's bytes are
    /// the same bytes.
    ///
    /// M18 is where it stops being invisible. A **published row**'s name is
    /// the hash of the *question* it answers, not of its contents
    /// ([`super::published`]), so two writers of one row key can legitimately
    /// hold different bytes — two workers each claiming a lease is exactly
    /// that — and "whoever wrote last wins" means both of them read their
    /// own claim back and both believe they hold it. Measured, not
    /// reasoned about: two workers divided four steps into six executions
    /// instead of four.
    ///
    /// `hard_link` fails with `AlreadyExists` instead of replacing, and
    /// does it in one syscall, so the winner is decided by the filesystem
    /// rather than by a gap in this function.
    fn install(&self, staged: &Path, key: &ObjectKey) -> Result<(), StoreError> {
        let path = self.path_of(key);
        if let Some(parent) = path.parent() {
            create_dir(parent)?;
        }

        match fs::hard_link(staged, &path) {
            Ok(()) => {
                remove(staged)?;
                make_read_only(&path)
            }
            // Somebody else got there first. Their object stands: write-once
            // means the first writer's bytes are the object's bytes.
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => remove(staged),
            // A filesystem with no hard links, which is rarer than it used
            // to be but not gone -- some FUSE mounts, and SMB without
            // POSIX extensions. Fall back to the non-atomic install rather
            // than refusing to store anything: a store that works and can
            // lose a lease race is worth more than one that cannot hold an
            // object at all, and D52 is what makes the loss a cost rather
            // than an error.
            Err(_) => {
                if path.is_file() {
                    return remove(staged);
                }
                move_file(staged, &path)?;
                make_read_only(&path)
            }
        }
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
        //
        // The staged name carries the writer as well as the key. Two
        // machines -- or two fan-out nodes -- writing the same object at
        // once is the normal case for a content-addressed store, and the
        // race is supposed to be benign: the bytes under a key are fixed
        // by the key, so whoever wins, the answer is the same. It is only
        // benign if the writers do not share a staging path. With one path
        // per key, the second writer found its own staged file already
        // renamed away by the first and failed with `NotFound` -- a
        // spurious error on a put that had in fact succeeded.
        let staged = self.root.join(format!(".partial-{key}.{}", writer()));
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

/// Something no two concurrent writers share.
///
/// The process, so two machines on one network mount do not collide, and a
/// counter, so two threads in one process do not either.
fn writer() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// The entries of a directory, or none at all if it is not there.
fn read_dir(path: &Path) -> Result<Vec<PathBuf>, StoreError> {
    match fs::read_dir(path) {
        Ok(entries) => Ok(entries.filter_map(Result::ok).map(|e| e.path()).collect()),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(StoreError::Io {
            path: path.to_path_buf(),
            source,
        }),
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
