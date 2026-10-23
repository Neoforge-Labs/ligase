//! The blob tier: objects named by the hash of their contents.
//!
//! This is the half of the cache that holds *things* rather than facts about
//! things — step outputs, canonical forms, and from M10 the bodies of
//! recorded responses. It is a trait rather than a directory because records
//! are the first thing written here that a lab will want to share, and
//! sharing it means a second backend: an object store, behind a cargo
//! feature, consulted on a local miss and written through on a local put.
//! The filesystem is the only backend until then.
//!
//! The metadata index is deliberately *not* here. SQLite over object storage
//! is a known failure, so the index stays local; it can, because everything
//! in it derives from content hashes and is therefore rebuildable from the
//! objects (D39's store trait, which M14 builds the remote tier on).
//!
//! # A key is a hash and a name
//!
//! [`ObjectKey`] carries a suffix beside the hash, and the suffix is
//! functional rather than cosmetic: a step's output is handed to `bcftools`
//! or `samtools` as a path, and those tools dispatch on the extension. A
//! store keyed on the hash alone could not name the file it was asked for.
//! An object store handles a suffixed key as happily as a filesystem does,
//! so nothing is given up by carrying it.
//!
//! # Nothing read from a store is trusted
//!
//! An object's name is a claim about its contents, which means it is a claim
//! that can be checked — and [`BlobStore::checked_get`] checks it, on the
//! trait rather than in any backend, so that a backend author cannot forget
//! to. A store a lab shares is a store other people can write to, and a
//! wrong answer served quickly from a cache is worse than no cache. Objects
//! are written once and never overwritten, for the same reason: every cache
//! key derived from an object rests on it still being what it was.

use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::hash::ContentHash;

mod fs;

pub use fs::FsStore;

/// An object's name: the hash of its contents, and the extension it is read
/// under.
///
/// Two keys sharing a hash and differing in suffix are two objects. That
/// looks like duplication and is the point: the bytes are the same, and what
/// differs is the name a tool has to be handed in order to read them.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ObjectKey {
    /// The hash of the object's contents, which is most of its name.
    pub hash: ContentHash,
    /// Everything after the first dot of the file name, so `.vcf.gz`
    /// survives. Empty is legal: a recorded response body has no extension
    /// to speak of and must not need an invented one.
    pub suffix: String,
}

impl ObjectKey {
    /// A key from a hash and a suffix.
    #[must_use]
    pub fn new(hash: ContentHash, suffix: &str) -> Self {
        Self {
            hash,
            suffix: suffix.to_owned(),
        }
    }

    /// The key an object with these bytes would have.
    ///
    /// For data already in memory — a recorded response body, a small
    /// canonical form. A file on disk is hashed by [`ContentHash::of_file`]
    /// instead, which memory-maps it.
    #[must_use]
    pub fn of_bytes(bytes: &[u8], suffix: &str) -> Self {
        Self::new(ContentHash::of_bytes(bytes), suffix)
    }
}

impl fmt::Display for ObjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.hash.to_hex(), self.suffix)
    }
}

/// Why the store could not do something.
#[derive(Debug)]
pub enum StoreError {
    /// An object could not be read or written.
    Io {
        /// What was being read or written.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// An object is not what its name says it is.
    ///
    /// The one failure a content-addressed store exists to be able to
    /// report. It means corruption, or a store someone else can write to,
    /// and in either case the object is refused rather than served.
    Mismatch {
        /// The hash the object's name claimed.
        expected: ContentHash,
        /// The hash its bytes actually have.
        found: ContentHash,
        /// Where the offending object was found.
        at: PathBuf,
    },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Mismatch {
                expected,
                found,
                at,
            } => write!(
                f,
                "the object at {} is not what its name says: it is named for \
                 {} but its contents hash to {}. It has been changed since it \
                 was stored, so it is refused rather than served",
                at.display(),
                expected.to_hex(),
                found.to_hex()
            ),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Mismatch { .. } => None,
        }
    }
}

/// Objects named by the hash of their contents.
///
/// The three operations D39's store trait names — `has`, `get`, `put` — plus
/// the two a filesystem affords and an object store has to emulate:
/// [`put_file`](BlobStore::put_file), so a finished step's output is moved
/// rather than copied through memory, and
/// [`local_path`](BlobStore::local_path), so an external tool can be handed a
/// path.
///
/// Implementors provide the unchecked primitives. Verification is
/// [`checked_get`](BlobStore::checked_get), which is provided here, because
/// "nothing read from a store is trusted" must not be a rule each backend
/// remembers separately.
pub trait BlobStore {
    /// Whether this object is held.
    ///
    /// # Errors
    ///
    /// If the store cannot be consulted.
    fn has(&self, key: &ObjectKey) -> Result<bool, StoreError>;

    /// The object's bytes, or `None` if it is not held.
    ///
    /// A miss is an ordinary answer rather than a failure: it is what sends
    /// a step off to compute. **This does not verify the object** — prefer
    /// [`checked_get`](BlobStore::checked_get) for anything whose answer is
    /// going to be believed.
    ///
    /// # Errors
    ///
    /// If the object is held but cannot be read.
    fn get(&self, key: &ObjectKey) -> Result<Option<Vec<u8>>, StoreError>;

    /// Hold these bytes under this key.
    ///
    /// An object already held is kept as it is: identical bytes are
    /// identical bytes, and rewriting one would move ground that every cache
    /// key derived from it stands on.
    ///
    /// # Errors
    ///
    /// If the object cannot be written.
    fn put(&self, key: &ObjectKey, bytes: &[u8]) -> Result<(), StoreError>;

    /// Hold a staged file, under the hash of the contents it turns out to
    /// have.
    ///
    /// The staged file is consumed. For a step's output, where what was
    /// produced is whatever was produced and the hash is discovered rather
    /// than promised.
    ///
    /// # Errors
    ///
    /// If the file cannot be read, hashed or moved.
    fn put_file(&self, staged: &Path, suffix: &str) -> Result<ObjectKey, StoreError>;

    /// Hold a staged file under a key it is *expected* to have, refusing it
    /// if it does not.
    ///
    /// For a fetch, where the hash was promised in advance: an interrupted
    /// download must never acquire a name that claims to be a whole file.
    /// The staged file is consumed on success and left where it is on a
    /// mismatch, so the caller can report on it.
    ///
    /// # Errors
    ///
    /// [`StoreError::Mismatch`] if the file is not what the key claims, or
    /// an I/O failure.
    fn put_file_as(&self, staged: &Path, key: &ObjectKey) -> Result<(), StoreError>;

    /// Where this object sits on this filesystem, if it sits on one.
    ///
    /// `None` for a backend with no local copy. M14's two-tier store answers
    /// by fetching into its local tier first, which is why this returns a
    /// `Result` as well as an `Option`: finding out can fail.
    ///
    /// # Errors
    ///
    /// If the store cannot be consulted, or a lower tier cannot be read.
    fn local_path(&self, key: &ObjectKey) -> Result<Option<PathBuf>, StoreError>;

    /// The object's bytes, verified against the hash its key claims.
    ///
    /// `None` for an object that is not held: nothing to verify is not a
    /// failed verification, and collapsing the two would turn a cold cache
    /// into a hard error.
    ///
    /// # Errors
    ///
    /// [`StoreError::Mismatch`] if the object is not what its name says, or
    /// an I/O failure.
    fn checked_get(&self, key: &ObjectKey) -> Result<Option<Vec<u8>>, StoreError> {
        let Some(bytes) = self.get(key)? else {
            return Ok(None);
        };
        let found = ContentHash::of_bytes(&bytes);
        if found == key.hash {
            return Ok(Some(bytes));
        }
        Err(StoreError::Mismatch {
            expected: key.hash,
            found,
            at: self
                .local_path(key)?
                .unwrap_or_else(|| PathBuf::from(key.to_string())),
        })
    }
}

/// Everything after the first dot of a file's name, so `.vcf.gz` survives.
///
/// A suffix is part of an object's key, so it has to be derived the same way
/// everywhere; this is the one place that derives it.
#[must_use]
pub fn suffix_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy())
        .and_then(|name| name.find('.').map(|at| name[at..].to_owned()))
        .unwrap_or_default()
}
