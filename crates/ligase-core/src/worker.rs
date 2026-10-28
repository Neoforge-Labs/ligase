//! Who a worker is, and the one cache directory it is allowed to have.
//!
//! A worker is one `ligase run` among several, dividing a pipeline through
//! a shared store (D51). It needs two things nothing else in the engine
//! needed: a name a user can read in a lease and in a report, and an
//! exclusive hold on its cache directory.
//!
//! **The name is supplied rather than discovered.** There is no standard
//! library call for a hostname, and taking one would have cost a
//! dependency for a string the user is better placed to choose anyway:
//! `--worker=$SLURM_ARRAY_TASK_ID` says something a hostname does not. An
//! unnamed worker gets a short identifier instead, which is enough to tell
//! two of them apart and honest about saying nothing else.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Where a worker's own cache directory sits under the cache it was given.
const WORKERS: &str = "workers";

/// The file that holds a cache directory against a second worker.
const LOCK: &str = "worker.lock";

/// Serial numbers, so two workers made in one process still differ.
static MADE: AtomicU64 = AtomicU64::new(0);

/// Who a worker is.
///
/// Two fields rather than one, because they answer different questions. The
/// **name** is for a human reading a lease or a report, and the user chooses
/// it. The **instance** is for the engine: it is unique to this process, and
/// it is what the read-back after publishing a lease compares, so that two
/// processes a user has confusingly given one name to still cannot both
/// believe they hold the same key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Worker {
    name: String,
    instance: String,
}

impl Worker {
    /// A worker with the name the user gave it.
    ///
    /// The name is reduced to characters a path and a single TSV field can
    /// both carry — letters, digits, `.`, `-` and `_`, with anything else
    /// becoming `-`. That is sanitising rather than refusing because this
    /// is reached from three surfaces and a name is not worth three
    /// error paths; the CLI refuses the names worth telling a user about
    /// (a path separator, above all) before it gets here.
    #[must_use]
    pub fn named(name: &str) -> Self {
        let cleaned: String = name
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                    character
                } else {
                    '-'
                }
            })
            .collect();
        let name = if cleaned.trim_matches('-').is_empty() {
            token()
        } else {
            cleaned
        };
        Self {
            name,
            instance: token(),
        }
    }

    /// A worker that was given no name, and so names itself.
    #[must_use]
    pub fn unnamed() -> Self {
        let instance = token();
        Self {
            name: instance.clone(),
            instance,
        }
    }

    /// What to call this worker in a lease, a report or an error.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// What tells this worker from every other, including one with the
    /// same name.
    #[must_use]
    pub fn instance(&self) -> &str {
        &self.instance
    }
}

/// A short identifier, from the clock, the process and a serial number.
///
/// None of the three alone is enough: two processes can start in the same
/// nanosecond on different machines, a pid is reused, and a serial number
/// restarts. Together they are enough for the only thing this is used for
/// — telling two workers apart.
fn token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let serial = MADE.fetch_add(1, Ordering::Relaxed);
    let mut hasher = blake3::Hasher::new();
    hasher.update(&nanos.to_le_bytes());
    hasher.update(&std::process::id().to_le_bytes());
    hasher.update(&serial.to_le_bytes());
    hasher.finalize().to_hex()[..8].to_string()
}

/// Why a cache directory could not be held.
#[derive(Debug)]
pub enum LockError {
    /// Another worker has it.
    Held {
        /// The directory.
        root: PathBuf,
        /// What the lock file says about who holds it.
        holder: String,
    },
    /// The lock file could not be written or read.
    Io {
        /// The lock file.
        path: PathBuf,
        /// What the filesystem said.
        source: io::Error,
    },
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Held { root, holder } => write!(
                f,
                "the cache at {} is held by {holder}.\n\
                 Two workers must not share a cache directory: the index is \
                 SQLite, and SQLite over a network filesystem is the failure \
                 D43 names. Give each worker its own --cache, or drop --cache \
                 and let --worker choose one.\n\
                 If that worker is gone, delete {}.",
                root.display(),
                root.join(LOCK).display()
            ),
            Self::Io { path, source } => {
                write!(f, "{}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for LockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Held { .. } => None,
        }
    }
}

/// An exclusive hold on one cache directory, for as long as it is alive.
///
/// Created with `O_EXCL` and removed on drop, so a clean exit gives the
/// directory back. A crash leaves the file behind and the next worker is
/// refused and told to delete it — a real wart, accepted because the
/// alternatives are `flock` (unreliable on exactly the network
/// filesystems this is guarding) or a liveness probe that is not portable,
/// and because a stale lock costs a deletion rather than a wrong answer.
#[derive(Debug)]
pub struct Lock {
    path: PathBuf,
}

impl Lock {
    /// The cache directory this worker should use under `root`.
    ///
    /// Its own, named after it. The right thing is the default rather than
    /// merely the documented thing: a Slurm array whose tasks all default
    /// to one `$HOME/.ligase` would otherwise share a `cache.sqlite` over
    /// NFS, and every one of them would be doing the thing D43 forbids
    /// without having chosen to.
    #[must_use]
    pub fn directory(root: &Path, worker: &Worker) -> PathBuf {
        root.join(WORKERS).join(worker.name())
    }

    /// Hold `root` for this worker.
    ///
    /// # Errors
    ///
    /// [`LockError::Held`] if another worker has it, naming that worker;
    /// [`LockError::Io`] if the lock file cannot be written.
    pub fn take(root: &Path, worker: &Worker) -> Result<Self, LockError> {
        let path = root.join(LOCK);
        let io = |source: io::Error| LockError::Io {
            path: path.clone(),
            source,
        };

        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                use std::io::Write;
                // Written for a human to read when they are deciding
                // whether the holder is still alive.
                let note = format!(
                    "{} (instance {}, pid {})\n",
                    worker.name(),
                    worker.instance(),
                    std::process::id()
                );
                file.write_all(note.as_bytes()).map_err(io)?;
                Ok(Self { path })
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let holder = fs::read_to_string(&path)
                    .map(|text| text.trim().to_owned())
                    .unwrap_or_default();
                Err(LockError::Held {
                    root: root.to_path_buf(),
                    holder: if holder.is_empty() {
                        "another worker".to_owned()
                    } else {
                        holder
                    },
                })
            }
            Err(error) => Err(io(error)),
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        // A lock we cannot remove is a stale lock, which the next worker is
        // told how to clear. There is nothing useful to do about it here.
        let _ = fs::remove_file(&self.path);
    }
}
