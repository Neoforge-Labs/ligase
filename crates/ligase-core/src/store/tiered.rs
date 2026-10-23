//! Two tiers: a local store, and a remote one consulted on a miss.
//!
//! This is the Nix binary-cache model (D43). The local store is always
//! there and is always the first tier; the remote is a *substituter* — a
//! place an answer may already exist — and it is written through on a put
//! so that the next machine finds what this one computed.
//!
//! # What a second tier changes is failure, not storage
//!
//! A local miss used to be an answer. Now it can be a network call, so it
//! can be slow, and it can fail. Three rules settle what happens, and they
//! are the whole design:
//!
//! **An unreachable remote is a miss.** Not a failure. A remote holds
//! nothing the engine cannot recompute, so falling back to computing is
//! always *correct*, and failing the run instead would make the engine
//! worse than having no remote at all. The cost is time, and the one way
//! that cost becomes a bug is a user who is not told — so every fallback is
//! counted in [`StoreStats`] and the run report says so.
//!
//! **A hash mismatch is not a miss.** It is a finding. An object that is
//! not what its name says means corruption or a bucket someone else can
//! write to, and both are things a user has to hear about. It is returned
//! as an error; the engine above turns it into a rerun.
//!
//! **Promotion verifies before it installs.** The local tier is trusted —
//! nothing re-hashes an object read from it on the hot path — so anything
//! promoted into it has to earn that. A corrupt object copied down
//! unverified would be caught once and then never again, which is the
//! worst of the available behaviours.
//!
//! # Why the remote is never read unverified
//!
//! [`BlobStore::get`] is documented as not verifying, and that is right for
//! a local file the engine itself wrote. For a remote it is not, so this
//! module calls [`BlobStore::checked_get`] on the remote and nothing else,
//! and the backend is held behind a `Box<dyn BlobStore>` that no caller
//! outside this module is given. The rule is structural rather than
//! remembered.
//!
//! # What this does not do
//!
//! It does not evict (§8 q5: the local tier grows and does not shrink in
//! v0.2, because an LRU whose source of truth is a remote turns a cache
//! miss into a network failure mode, and there is no measurement yet to
//! size one with). It does not collect garbage, and it does not manage
//! buckets. `ligase store gc` is a v0.3 item.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::{Backend, BlobStore, FsStore, ObjectKey, StoreError};

/// What the store did, in numbers.
///
/// Reported by `ligase run`, because every one of these is something a user
/// would otherwise have to guess at from how long the run took.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StoreStats {
    /// Objects promoted from the remote into the local tier.
    pub fetched: u64,
    /// Objects written through to the remote.
    pub pushed: u64,
    /// Calls to the remote that failed, each of which became a miss.
    ///
    /// Not an error, and not nothing: a run that recomputed for an hour
    /// because a bucket was briefly away is not wrong, and a user who is
    /// not told is misled.
    pub unreachable: u64,
    /// Objects the remote served that were not what their names said.
    pub rejected: u64,
}

impl StoreStats {
    /// Whether anything happened worth printing.
    #[must_use]
    pub fn quiet(&self) -> bool {
        *self == Self::default()
    }
}

/// The counters, as the store keeps them.
#[derive(Debug, Default)]
struct Counters {
    fetched: AtomicU64,
    pushed: AtomicU64,
    unreachable: AtomicU64,
    rejected: AtomicU64,
}

/// A local store, and a remote one consulted on a miss.
pub struct TieredStore {
    local: FsStore,
    /// `None` is the default install: one tier, exactly as before.
    remote: Option<Box<dyn BlobStore + Send + Sync>>,
    url: Option<String>,
    counters: Counters,
}

impl fmt::Debug for TieredStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TieredStore")
            .field("local", &self.local)
            .field("remote", &self.url)
            .finish()
    }
}

impl TieredStore {
    /// Open the local tier at `local`, with the store `url` names behind it.
    ///
    /// `None` is the default install and costs nothing: no second tier, no
    /// network, no configuration.
    ///
    /// # Errors
    ///
    /// If the local directory cannot be created, if `url` cannot be parsed,
    /// or if it names a backend this build was not compiled with.
    pub fn open(local: &Path, url: Option<&str>) -> Result<Self, StoreError> {
        let Some(url) = url else {
            return Ok(Self::alone(FsStore::open(local)?));
        };

        let remote: Box<dyn BlobStore + Send + Sync> = match Backend::parse(url)? {
            Backend::Local(path) => Box::new(FsStore::open(&path)?),
            Backend::Remote { url } => super::remote::open(&url)?,
        };
        Self::with_remote_store(FsStore::open(local)?, remote, url)
    }

    /// The same, with a backend supplied directly.
    ///
    /// For a test, and for anyone embedding the engine who has a store
    /// Ligase does not know about. Whatever is passed is still read through
    /// [`BlobStore::checked_get`] and never through
    /// [`BlobStore::get`], so supplying a backend cannot be a way around
    /// verification.
    ///
    /// # Errors
    ///
    /// If the local directory cannot be created.
    pub fn with_remote(
        local: &Path,
        remote: Box<dyn BlobStore + Send + Sync>,
        url: &str,
    ) -> Result<Self, StoreError> {
        Self::with_remote_store(FsStore::open(local)?, remote, url)
    }

    fn with_remote_store(
        local: FsStore,
        remote: Box<dyn BlobStore + Send + Sync>,
        url: &str,
    ) -> Result<Self, StoreError> {
        Ok(Self {
            local,
            remote: Some(remote),
            url: Some(url.to_owned()),
            counters: Counters::default(),
        })
    }

    fn alone(local: FsStore) -> Self {
        Self {
            local,
            remote: None,
            url: None,
            counters: Counters::default(),
        }
    }

    /// The local tier, which is where objects actually sit.
    #[must_use]
    pub fn local(&self) -> &FsStore {
        &self.local
    }

    /// The URL of the remote tier, if there is one.
    #[must_use]
    pub fn remote_url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// Whether a remote tier is configured at all.
    #[must_use]
    pub fn is_tiered(&self) -> bool {
        self.remote.is_some()
    }

    /// Where an object sits in the local tier, whether or not it is there.
    #[must_use]
    pub fn path_of(&self, key: &ObjectKey) -> PathBuf {
        self.local.path_of(key)
    }

    /// What the store has done since it was opened.
    #[must_use]
    pub fn stats(&self) -> StoreStats {
        StoreStats {
            fetched: self.counters.fetched.load(Ordering::Relaxed),
            pushed: self.counters.pushed.load(Ordering::Relaxed),
            unreachable: self.counters.unreachable.load(Ordering::Relaxed),
            rejected: self.counters.rejected.load(Ordering::Relaxed),
        }
    }

    /// Whether the remote answers at all, for `ligase store status`.
    ///
    /// Asks for an object that cannot exist, so the answer is about the
    /// store rather than about its contents. `None` where no remote is
    /// configured.
    ///
    /// # Errors
    ///
    /// Never: an unreachable store is a reported fact here, not a failure.
    pub fn reachable(&self) -> Option<Result<(), String>> {
        let remote = self.remote.as_ref()?;
        let probe = ObjectKey::new(crate::hash::ContentHash::of_bytes(b"ligase.reachable"), "");
        Some(remote.has(&probe).map(|_| ()).map_err(|error| {
            self.counters.unreachable.fetch_add(1, Ordering::Relaxed);
            error.to_string()
        }))
    }

    /// Read an object from the remote, verified, or report why not.
    ///
    /// The one place the remote is read. A mismatch is returned; anything
    /// else is swallowed into a miss and counted.
    fn read_remote(&self, key: &ObjectKey) -> Result<Option<Vec<u8>>, StoreError> {
        let Some(remote) = self.remote.as_ref() else {
            return Ok(None);
        };
        match remote.checked_get(key) {
            Ok(Some(bytes)) => Ok(Some(bytes)),
            Ok(None) => Ok(None),
            Err(error @ StoreError::Mismatch { .. }) => {
                self.counters.rejected.fetch_add(1, Ordering::Relaxed);
                Err(error)
            }
            Err(_) => {
                self.counters.unreachable.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
        }
    }

    /// Fetch an object into the local tier, verified on the way in.
    fn promote(&self, key: &ObjectKey) -> Result<Option<Vec<u8>>, StoreError> {
        let Some(bytes) = self.read_remote(key)? else {
            return Ok(None);
        };
        // `checked_get` has already compared the bytes against the key, so
        // what lands locally is what the key claims.
        self.local.put(key, &bytes)?;
        self.counters.fetched.fetch_add(1, Ordering::Relaxed);
        Ok(Some(bytes))
    }

    /// Push an object to the remote, counting what happened.
    ///
    /// Never fails the caller. The object is in the local tier, so the run
    /// is right; an upload that did not happen costs the *next* machine a
    /// recomputation, and `ligase store push` exists to make it up later.
    fn push(&self, key: &ObjectKey, bytes: Option<&[u8]>, from: Option<&Path>) {
        let Some(remote) = self.remote.as_ref() else {
            return;
        };
        match remote.has(key) {
            // Write-once: an object already held is left exactly as it is,
            // because every cache key derived from it stands on that.
            Ok(true) => return,
            Ok(false) => {}
            Err(_) => {
                self.counters.unreachable.fetch_add(1, Ordering::Relaxed);
                return;
            }
        }

        let written = match (bytes, from) {
            (Some(bytes), _) => remote.put(key, bytes),
            (None, Some(path)) => remote.put_from_path(key, path),
            (None, None) => return,
        };
        match written {
            Ok(()) => {
                self.counters.pushed.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => {
                self.counters.unreachable.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

impl BlobStore for TieredStore {
    fn has(&self, key: &ObjectKey) -> Result<bool, StoreError> {
        if self.local.has(key)? {
            return Ok(true);
        }
        let Some(remote) = self.remote.as_ref() else {
            return Ok(false);
        };
        match remote.has(key) {
            Ok(held) => Ok(held),
            Err(_) => {
                self.counters.unreachable.fetch_add(1, Ordering::Relaxed);
                Ok(false)
            }
        }
    }

    fn get(&self, key: &ObjectKey) -> Result<Option<Vec<u8>>, StoreError> {
        match self.local.get(key)? {
            Some(bytes) => Ok(Some(bytes)),
            None => self.promote(key),
        }
    }

    fn put(&self, key: &ObjectKey, bytes: &[u8]) -> Result<(), StoreError> {
        self.local.put(key, bytes)?;
        self.push(key, Some(bytes), None);
        Ok(())
    }

    fn put_file(&self, staged: &Path, suffix: &str) -> Result<ObjectKey, StoreError> {
        let key = self.local.put_file(staged, suffix)?;
        // From the installed path rather than from memory: a step's output
        // can be a forty-gigabyte BAM, and reading one into a Vec to upload
        // it would be a new way for a run to die.
        self.push(&key, None, Some(&self.local.path_of(&key)));
        Ok(key)
    }

    fn put_file_as(&self, staged: &Path, key: &ObjectKey) -> Result<(), StoreError> {
        self.local.put_file_as(staged, key)?;
        self.push(key, None, Some(&self.local.path_of(key)));
        Ok(())
    }

    fn local_path(&self, key: &ObjectKey) -> Result<Option<PathBuf>, StoreError> {
        if let Some(path) = self.local.local_path(key)? {
            return Ok(Some(path));
        }
        if self.promote(key)?.is_none() {
            return Ok(None);
        }
        self.local.local_path(key)
    }
}
