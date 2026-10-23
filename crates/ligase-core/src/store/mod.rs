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
pub mod published;
mod remote;
mod tiered;

pub use fs::FsStore;
pub use tiered::{StoreStats, TieredStore};

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
    /// A published row that answers a different question.
    ///
    /// A row's name is the hash of the question rather than of its
    /// contents ([`published`]), so the body restating the question is
    /// the only check it can carry. One that restates the wrong question
    /// was written by hand or by accident, and a store holding one is a
    /// store to look at rather than to read.
    Impostor {
        /// The object it was found under.
        at: String,
        /// What it says it is instead.
        claims: String,
    },
    /// A store URL this build cannot open.
    ///
    /// Either the URL names nothing Ligase knows, or it names a backend
    /// this build was not compiled with. The two are deliberately one
    /// variant with two reasons, because what a user needs is the reason
    /// and not the category.
    Unsupported {
        /// The URL as it was given.
        url: String,
        /// Why it cannot be opened, and what to do instead.
        reason: String,
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
            Self::Impostor { at, claims } => write!(
                f,
                "the row at {at} is not the row it was asked for: it is {claims}. \
                 It has been refused rather than believed"
            ),
            Self::Unsupported { url, reason } => write!(f, "{url}: {reason}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Mismatch { .. } | Self::Impostor { .. } | Self::Unsupported { .. } => None,
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

    /// Hold a file that is already in a store, under a key already known.
    ///
    /// For a tier writing through to the one below it: the object has been
    /// installed and hashed, and what is left is to copy it without
    /// reading it into memory. The default does read it into memory, which
    /// is right for a filesystem and wrong for a forty-gigabyte upload, so
    /// a backend that can stream overrides this.
    ///
    /// Unlike [`put_file`](BlobStore::put_file), the source is **left
    /// where it is**: it belongs to the tier above.
    ///
    /// # Errors
    ///
    /// If the file cannot be read or the object cannot be written.
    fn put_from_path(&self, key: &ObjectKey, from: &Path) -> Result<(), StoreError> {
        let bytes = std::fs::read(from).map_err(|source| StoreError::Io {
            path: from.to_path_buf(),
            source,
        })?;
        self.put(key, &bytes)
    }

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

    /// The bytes of a published row, which cannot be verified by hash.
    ///
    /// A row's name is the hash of the question it answers, not of its
    /// contents ([`published`]), so [`checked_get`](BlobStore::checked_get)
    /// would refuse every one of them. The check a row *can* carry is that
    /// its body restates the table and key it was found under, and
    /// [`published::fetch`] applies it on every read — including a read
    /// served from a local tier, so a bad row cannot be laundered by
    /// being cached.
    ///
    /// A key that is not a row's falls through to the ordinary, verified
    /// read. That is what keeps this from being a way around verification:
    /// it cannot fetch an *object* unverified however it is called.
    ///
    /// # Errors
    ///
    /// If the store cannot be read.
    fn get_row(&self, key: &ObjectKey) -> Result<Option<Vec<u8>>, StoreError> {
        if key.suffix != ROW_SUFFIX {
            return self.checked_get(key);
        }
        self.get(key)
    }

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

/// The suffix of a published row.
///
/// Rows are the one object class whose name is the hash of the *question*
/// rather than of the contents ([`published`]), so they cannot be read
/// through the verified path and are told apart from objects by this.
pub const ROW_SUFFIX: &str = ".row";

/// The environment variable that names the store.
///
/// The one Ligase-owned name in the whole of this feature. Everything else
/// a backend needs -- endpoint, region, credentials -- comes from the
/// provider's own environment, so a user who already has `aws s3 ls`
/// working has Ligase working (D43).
pub const STORE_VARIABLE: &str = "LIGASE_STORE";

/// The URL schemes a store can be named by.
///
/// Closed and listed, on the same ground as a remote locator's four
/// schemes (D39): a set extensible by configuration would be a promise the
/// engine could not keep.
pub const SCHEMES: [&str; 4] = ["file", "s3", "gs", "az"];

/// What a store URL turned out to name.
///
/// Parsed before anything connects. A URL that cannot work is then an
/// error with the alternatives in it, rather than a run that starts, gets
/// an hour in, and misses on every lookup because the store it was told
/// about does not exist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Backend {
    /// A directory, which may be a network mount. Served by [`FsStore`],
    /// so it needs no feature and no dependency: a lab with an NFS share
    /// already has somewhere to put a shared store.
    Local(PathBuf),
    /// An object store, served by the `remote-store` feature.
    Remote {
        /// The URL, passed to the backend as it was given.
        url: String,
    },
}

impl Backend {
    /// What this URL names, or why it cannot name anything.
    ///
    /// Note that a bucket URL parses in *every* build, including one
    /// compiled without the feature. Opening it is what fails there, and
    /// the difference matters: "this build has no remote-store support"
    /// sends a user to their installation, and "unknown scheme" would send
    /// them to fix a URL that is correct.
    ///
    /// # Errors
    ///
    /// [`StoreError::Unsupported`] for a URL with no scheme, an unknown
    /// scheme, or nothing after the scheme.
    pub fn parse(url: &str) -> Result<Self, StoreError> {
        let unsupported = |reason: &str| StoreError::Unsupported {
            url: url.to_owned(),
            reason: reason.to_owned(),
        };

        let Some((scheme, rest)) = url.split_once("://") else {
            return Err(unsupported(&format!(
                "a store URL needs a scheme, one of {}",
                SCHEMES.join(", ")
            )));
        };
        if rest.is_empty() {
            return Err(unsupported("a store URL needs something after its scheme"));
        }
        if !SCHEMES.contains(&scheme) {
            return Err(unsupported(&format!(
                "`{scheme}` is not a store Ligase knows; it reads {}",
                SCHEMES.join(", ")
            )));
        }

        if scheme == "file" {
            return Ok(Self::Local(PathBuf::from(rest)));
        }
        Ok(Self::Remote {
            url: url.to_owned(),
        })
    }
}

/// The store the environment names, if it names one.
///
/// Read here rather than threaded through every caller so that one
/// variable configures every entry point at once, and read *only* here so
/// that a test can hand [`tiered::TieredStore::open`] a URL directly
/// without touching a process-wide variable other tests are reading.
#[must_use]
pub fn configured() -> Option<String> {
    std::env::var(STORE_VARIABLE)
        .ok()
        .filter(|url| !url.trim().is_empty())
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
