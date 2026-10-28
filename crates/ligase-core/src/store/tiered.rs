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

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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
    /// Distinct objects the remote served that were not what their names
    /// said.
    ///
    /// Distinct, not occurrences: one bad object read twice in a run is
    /// one bad object, and a report saying "and 1 further object" about
    /// the object it has just named would send someone looking for a
    /// second problem that is not there.
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
}

/// A local store, and a remote one consulted on a miss.
pub struct TieredStore {
    local: FsStore,
    /// `None` is the default install: one tier, exactly as before.
    remote: Option<Box<dyn BlobStore + Send + Sync>>,
    url: Option<String>,
    counters: Counters,
    /// Every object the remote served that was not what its name said:
    /// its key, and the first error each produced.
    ///
    /// Kept rather than merely counted because the clause is that a
    /// corrupted object is "rejected on read with an error naming the
    /// hash mismatch", and a count names nothing. Keyed, because one bad
    /// object read twice in a run is one bad object.
    rejections: Mutex<BTreeSet<(String, String)>>,
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
            rejections: Mutex::new(BTreeSet::new()),
        })
    }

    fn alone(local: FsStore) -> Self {
        Self {
            local,
            remote: None,
            url: None,
            counters: Counters::default(),
            rejections: Mutex::new(BTreeSet::new()),
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

    /// The tier every machine sees, for the one thing that must not be
    /// cached.
    ///
    /// Everything else in this module is built on the opposite premise: a
    /// local copy of an object is as good as the remote's, because an
    /// object's name fixes its contents, so reading locally is right and
    /// cheap. A **lease** is the exception, and it is worth being explicit
    /// about why, because the first implementation of M18 got it wrong and
    /// the tests caught it.
    ///
    /// A lease row says "worker W is computing key K". That is not a fact
    /// about K, which is what content addressing makes cacheable — it is a
    /// fact about *the other machines*, and it changes without anything
    /// here being told. Written through the tiering, each worker's claim
    /// landed in its own local tier and every read was served from it, so
    /// every worker saw only its own claims, found every key free, and
    /// computed the whole graph. The feature was silently absent, and the
    /// run was still *correct* — which is D50 being true and is exactly
    /// why this needed a test rather than a reading.
    ///
    /// `None` when no remote is configured. A worker with nothing shared
    /// has nothing to divide and no one to divide it with; the surfaces
    /// refuse that configuration before a run starts (D49).
    #[must_use]
    pub fn shared(&self) -> Option<&(dyn BlobStore + Send + Sync)> {
        self.remote.as_deref()
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
            rejected: self
                .rejections
                .lock()
                .map(|held| held.len() as u64)
                .unwrap_or_default(),
        }
    }

    /// What the remote served that was not what its name said, if
    /// anything was.
    ///
    /// For the run report. A rejection is the one store event a user has
    /// to read rather than merely count: it means corruption, or a bucket
    /// someone else can write to.
    #[must_use]
    pub fn rejection(&self) -> Option<String> {
        self.rejections
            .lock()
            .ok()
            .and_then(|held| held.iter().next().map(|(_, error)| error.clone()))
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
            Err(StoreError::Mismatch {
                expected,
                found,
                at: _,
            }) => {
                // Re-pointed at the store it actually came from. The
                // default says only what the object is called, and the
                // question a user has is *which bucket is serving me
                // this*.
                let error = StoreError::Mismatch {
                    expected,
                    found,
                    at: PathBuf::from(format!(
                        "{}/{key}",
                        self.url.as_deref().unwrap_or("the remote store")
                    )),
                };
                if let Ok(mut held) = self.rejections.lock() {
                    held.insert((key.to_string(), error.to_string()));
                }
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

/// What `ligase store status` found.
///
/// The `local_only` count is a diff between two stores, which on a large
/// bucket is not free, so it carries [`exact`](Survey::exact) beside it.
/// A cheap number read as an exact one is its own kind of wrong answer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Survey {
    /// Objects in the local tier.
    pub objects: usize,
    /// Published index rows in the local tier.
    pub rows: usize,
    /// How many of them were actually asked about.
    pub checked: usize,
    /// How many of those the remote does not hold.
    pub local_only: usize,
    /// Whether `local_only` is the whole truth or a sample of it.
    pub exact: bool,
}

impl TieredStore {
    /// Count what is here, and how much of it the remote has not got.
    ///
    /// One `has` per object, which is one round trip per object, so the
    /// walk stops after `bound` of them and says so. There is no cheaper
    /// honest answer: a bucket does not keep a count, and listing one to
    /// get it is the same work with a worse failure mode.
    ///
    /// # Errors
    ///
    /// If the local tier cannot be walked.
    pub fn survey(&self, bound: usize) -> Result<Survey, StoreError> {
        let held = self.local.keys()?;
        let mut survey = Survey {
            objects: held
                .iter()
                .filter(|key| key.suffix != super::ROW_SUFFIX)
                .count(),
            rows: held
                .iter()
                .filter(|key| key.suffix == super::ROW_SUFFIX)
                .count(),
            ..Survey::default()
        };
        survey.exact = held.len() <= bound;

        let Some(remote) = self.remote.as_ref() else {
            // Everything is local only, exactly and for free.
            survey.checked = held.len();
            survey.local_only = held.len();
            survey.exact = true;
            return Ok(survey);
        };

        for key in held.iter().take(bound) {
            match remote.has(key) {
                Ok(true) => survey.checked += 1,
                Ok(false) => {
                    survey.checked += 1;
                    survey.local_only += 1;
                }
                Err(_) => {
                    self.counters.unreachable.fetch_add(1, Ordering::Relaxed);
                    survey.exact = false;
                }
            }
        }
        Ok(survey)
    }

    /// Push everything in the local tier that the remote has not got.
    ///
    /// Rows as well as objects: an object the next machine cannot find is
    /// an object it recomputes.
    ///
    /// # Errors
    ///
    /// If the local tier cannot be walked. A single object that will not
    /// upload is counted rather than fatal, because stopping half way
    /// through a lab's cache helps nobody.
    pub fn push_all(&self) -> Result<(usize, usize), StoreError> {
        let before = self.stats();
        let held = self.local.keys()?;
        for key in &held {
            self.push(key, None, Some(&self.local.path_of(key)));
        }
        let after = self.stats();
        Ok((
            (after.pushed - before.pushed) as usize,
            (after.unreachable - before.unreachable) as usize,
        ))
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

    fn get_row(&self, key: &ObjectKey) -> Result<Option<Vec<u8>>, StoreError> {
        if key.suffix != super::ROW_SUFFIX {
            return self.checked_get(key);
        }
        if let Some(bytes) = self.local.get(key)? {
            return Ok(Some(bytes));
        }
        let Some(remote) = self.remote.as_ref() else {
            return Ok(None);
        };
        // The one read of the remote that does not go through
        // `checked_get`, because a row's name is not the hash of its
        // contents and the comparison would refuse every row there is.
        // Two things make it safe. The bytes are checked by the rule that
        // *does* apply -- `published::fetch` requires the body to restate
        // the table and key it was found under -- and they are **not
        // promoted**: nothing enters the trusted local tier on this path,
        // so an unverified byte cannot later be read as a verified one.
        match remote.get(key) {
            Ok(bytes) => Ok(bytes),
            Err(_) => {
                self.counters.unreachable.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
        }
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
