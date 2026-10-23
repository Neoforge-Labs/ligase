//! The cache: what has been computed, and what it produced.
//!
//! Two pieces, both under one directory (`.ligase/` by default).
//!
//! **A content-addressed store.** An output's name is the BLAKE3 hash of its
//! bytes, so two steps that produced identical results hold one object
//! between them and an object's name is a claim that can be checked. Stored
//! objects are read-only: the store is the cache's evidence, and a step that
//! wrote back through one would invalidate every key derived from it without
//! anything noticing.
//!
//! **SQLite metadata** (D6). Zero-config, embedded, portable, and readable
//! with `sqlite3` when a result needs explaining — which for a tool whose
//! whole purpose is reproducibility is worth more than a faster key-value
//! store.
//!
//! # The prefilter
//!
//! Hashing every input on every run is what makes content addressing honest,
//! and on a 40 GB BAM it is also what makes it slow. So a file's size and
//! mtime are remembered beside its hash, and a file whose size and mtime are
//! unchanged is not read again (D3). Note what this does and does not claim:
//! a *match* skips work, a *mismatch* only causes a rehash. The hash itself
//! is never taken from a timestamp, so the pathological case — a file
//! rewritten in place within the same mtime granularity, with the same
//! length — is the only one that can mislead, and `--force` exists for it.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, params};

use crate::canon::{self, CANON_VERSION, Canonical, Form, Note};
use crate::hash::ContentHash;
use crate::key::{KEY_VERSION, StepKey};
use crate::remote::request::Method;
use crate::store::published::{self, REMOTES, STEPS};
use crate::store::{BlobStore, ObjectKey, StoreError, TieredStore, suffix_of as suffix};
use crate::types::DataKind;

/// The version of the on-disk layout of the cache directory.
///
/// A directory written by another version is discarded rather than read:
/// everything in it can be recomputed, so there is nothing to migrate and
/// nothing to lose.
///
/// 2 added the `notes` column, for what canonicalising a file noticed
/// (D26). 3 added one to `steps`, for what a step noticed about its own
/// work (D32). 4 added the `remotes` table, for what was fetched from
/// where (D39). 5 added the `records` table beside it, for a recorded
/// query -- a separate table rather than a flag, because a pinned remote's
/// record is replaced and a query's must never be.
const SCHEMA_VERSION: u32 = 5;

/// An output held in the store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stored {
    /// The hash of its contents, which is also its name.
    pub hash: ContentHash,
    /// Where it sits in the store.
    pub path: PathBuf,
}

/// What a step's cache entry remembers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The output it produced.
    pub output: Stored,
    /// How long the step took when it did run, in milliseconds.
    pub duration_ms: u64,
    /// What the step noticed about its own work ([`Note`]).
    ///
    /// Empty for almost every step, and for every shell step: only a body
    /// can say what its work rested on. Stored rather than recomputed,
    /// because a cache hit does not rerun the body and a hedge that stopped
    /// appearing on the second run would be worse than none (D32).
    pub notes: Vec<Note>,
}

/// One object a remote resolved to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteFile {
    /// Its name in the store.
    pub key: ObjectKey,
    /// The URL that answered, after any redirects — which is not the
    /// locator, and is worth keeping precisely because it can differ.
    pub url: String,
    /// How many bytes arrived.
    pub bytes: u64,
}

/// What a past fetch produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteRow {
    /// The digest of what the files *mean*, folded over them in order —
    /// which is what a pin is compared against (D4, D21).
    pub digest: ContentHash,
    /// The digest of their *bytes*, folded the same way.
    ///
    /// Kept beside the other so that a source which rewrote a file without
    /// changing its meaning can be noticed rather than either ignored or
    /// treated as a failure.
    pub bytes: ContentHash,
    /// The objects, in order. Mate order for a paired run.
    pub files: Vec<RemoteFile>,
    /// When it was fetched, as seconds since the epoch.
    pub fetched_at: i64,
}

/// One recorded response to one query.
///
/// Identified by the hash of its canonical request, so a step making twenty
/// paginated calls writes twenty of these and finds them again whatever
/// order it asks in next time (D39).
///
/// Note what is **not** here: no request headers, so no `Authorization`
/// token. The transport's [`crate::remote::fetch::Fetched`] has no field
/// for one and neither does this, which is how "a bundle never carries a
/// secret" survives a later field being added by someone who has not read
/// D39.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryRecord {
    /// The hash of the canonical request, which is this record's identity.
    pub request: ContentHash,
    /// Which recording of that request this is: 1, then 2 for the first
    /// refresh, and so on.
    ///
    /// The revision rather than the date is what makes a record unique,
    /// because two records a second apart must still be two records and a
    /// clock is the wrong thing to rest that on.
    pub revision: u32,
    /// When it was recorded, as seconds since the epoch. What "a new dated
    /// record" and "replayed from record of <date>" both read.
    pub recorded_at: i64,
    /// The locator as the pipeline wrote it.
    pub locator: String,
    /// How the body is read.
    pub kind: DataKind,
    /// The method that was used, which is part of the identity and is kept
    /// so a record can be explained as well as found.
    pub method: Method,
    /// The URL that answered, after any redirects — not the locator, and
    /// worth keeping precisely because it can differ.
    pub url: String,
    /// The status it answered with. Always a success: a record of a failure
    /// would replay the failure forever as though it were an answer.
    pub status: u16,
    /// The canonical response body, in the blob store.
    pub body: ObjectKey,
    /// The digest of what the body *means*, read as `kind`, which is what
    /// enters a step key.
    pub digest: ContentHash,
    /// What canonicalising the response noticed: a field dropped from it.
    pub notes: Vec<Note>,
}

/// How much work the prefilter saved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Files read and hashed.
    pub hashed: u64,
    /// Files whose hash was already known from their size and mtime.
    pub prefiltered: u64,
    /// Files parsed to find what they mean, rather than only read.
    pub canonicalised: u64,
}

/// Why the cache could not do something.
#[derive(Debug)]
pub enum CacheError {
    /// The cache directory could not be read or written.
    Io {
        /// What was being read or written.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// The metadata database could not be read or written.
    Database(rusqlite::Error),
    /// A thread holding the database panicked.
    Poisoned,
    /// The blob tier could not hold or produce an object.
    Store(StoreError),
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Database(error) => write!(f, "cache metadata: {error}"),
            Self::Poisoned => f.write_str("the cache was left locked by a panicking thread"),
            Self::Store(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for CacheError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Database(error) => Some(error),
            Self::Poisoned => None,
            Self::Store(error) => Some(error),
        }
    }
}

impl From<rusqlite::Error> for CacheError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl From<StoreError> for CacheError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// Everything already computed, and where it was put.
#[derive(Debug)]
pub struct Cache {
    root: PathBuf,
    objects: TieredStore,
    database: Mutex<Connection>,
    hashed: AtomicU64,
    prefiltered: AtomicU64,
    canonicalised: AtomicU64,
}

impl Cache {
    /// Open the cache at `root`, creating it if it is not there.
    ///
    /// A cache written by another version of the layout, or holding keys
    /// from another key format, is cleared rather than read: both describe
    /// work that was combined differently, and all of it can be recomputed.
    ///
    /// # Errors
    ///
    /// If the directory cannot be created or the database cannot be opened.
    pub fn open(root: &Path) -> Result<Self, CacheError> {
        Self::open_with(root, crate::store::configured().as_deref())
    }

    /// The same, with the remote tier named directly.
    ///
    /// [`open`](Self::open) reads `LIGASE_STORE`, which is what the CLI
    /// wants: one variable configures every entry point. This is for a
    /// caller that has the URL in its hand already — a test, above all,
    /// which must be able to point at a store without touching a
    /// process-wide variable every other test is reading.
    ///
    /// # Errors
    ///
    /// As [`open`](Self::open), and if the store URL cannot be opened.
    pub fn open_with(root: &Path, store: Option<&str>) -> Result<Self, CacheError> {
        create_dir(root)?;
        let objects = TieredStore::open(&root.join("store"), store)?;
        create_dir(&root.join("tmp"))?;

        let database = Connection::open(root.join("cache.sqlite")).map_err(CacheError::from)?;
        // WAL so a reader and a writer do not block each other, and a
        // timeout so workers contending for the same row wait rather than
        // fail.
        database.pragma_update(None, "journal_mode", "WAL")?;
        database.busy_timeout(std::time::Duration::from_secs(30))?;
        prepare(&database)?;

        Ok(Self {
            root: root.to_path_buf(),
            objects,
            database: Mutex::new(database),
            hashed: AtomicU64::new(0),
            prefiltered: AtomicU64::new(0),
            canonicalised: AtomicU64::new(0),
        })
    }

    /// The directory this cache lives in.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// How much work the prefilter has saved since this cache was opened.
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hashed: self.hashed.load(Ordering::Relaxed),
            prefiltered: self.prefiltered.load(Ordering::Relaxed),
            canonicalised: self.canonicalised.load(Ordering::Relaxed),
        }
    }

    /// What a file *means*, as the hash a cache key is built from (D4).
    ///
    /// The canonical form of a file read as `kind`, which is what two runs
    /// are compared on: records sorted, volatile headers stripped
    /// ([`crate::canon`]).
    ///
    /// Remembered against the hash of the file's bytes rather than against
    /// its path, because that is what it is a fact about — so a file moved,
    /// copied or produced again identically is not parsed twice, and a
    /// second run of a pipeline reads nothing at all.
    ///
    /// # Errors
    ///
    /// If the file cannot be read, or the metadata cannot be written. A
    /// file that reads but does not parse is not an error: its bytes stand
    /// in for its meaning.
    pub fn semantic_hash(&self, kind: DataKind, path: &Path) -> Result<ContentHash, CacheError> {
        Ok(self.semantic(kind, path)?.hash)
    }

    /// The same, with how it was arrived at and what it noticed.
    ///
    /// For a caller that has somewhere to put a [`Note`] — a run report, a
    /// provenance record, a bundle. Notes are remembered with the hash, so
    /// a file served from the cache still reports what was seen the first
    /// time it was read.
    ///
    /// # Errors
    ///
    /// As [`Self::semantic_hash`].
    pub fn semantic(&self, kind: DataKind, path: &Path) -> Result<Canonical, CacheError> {
        let raw = self.content_hash(path)?;

        if let Some(known) = self.remembered_canonical(raw, kind)? {
            return Ok(known);
        }

        let canonical = canon::hash(kind, path, raw).map_err(|source| CacheError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        self.canonicalised.fetch_add(1, Ordering::Relaxed);
        self.remember_canonical(raw, kind, &canonical)?;
        Ok(canonical)
    }

    /// The hash of a file's contents, rereading it only when it may have
    /// changed.
    ///
    /// # Errors
    ///
    /// If the file cannot be read, or the metadata cannot be written.
    pub fn content_hash(&self, path: &Path) -> Result<ContentHash, CacheError> {
        let before = stat(path)?;
        let key = path.to_string_lossy().into_owned();

        if let Some(known) = self.remembered_hash(&key, before)? {
            self.prefiltered.fetch_add(1, Ordering::Relaxed);
            return Ok(known);
        }

        let hash = ContentHash::of_file(path).map_err(|source| CacheError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        self.hashed.fetch_add(1, Ordering::Relaxed);

        // Re-stat after reading: a file written while we hashed it would
        // otherwise be remembered under the stat it had before, and every
        // later run would trust that hash.
        let after = stat(path)?;
        if after == before {
            self.remember_hash(&key, before, hash)?;
        }
        Ok(hash)
    }

    /// Move a finished file into the store, under the hash of its contents.
    ///
    /// The staged file is consumed. An object that is already there is kept
    /// as it is: identical bytes are identical bytes.
    ///
    /// # Errors
    ///
    /// If the file cannot be read, hashed or moved.
    pub fn store(&self, staged: &Path) -> Result<Stored, CacheError> {
        let key = self.objects.put_file(staged, &suffix(staged))?;
        Ok(Stored {
            hash: key.hash,
            path: self.objects.path_of(&key),
        })
    }

    /// What was fetched for this locator, read as this kind, if anything
    /// has been.
    ///
    /// A record is a fact about a past fetch, not a promise about the
    /// present: whoever reads one checks that its objects are still in the
    /// store and that its digest still satisfies whatever the pipeline
    /// declares, because both can have moved since.
    ///
    /// # Errors
    ///
    /// If the metadata cannot be read.
    pub fn remote(&self, locator: &str, kind: DataKind) -> Result<Option<RemoteRow>, CacheError> {
        let row: Option<(String, String, String, i64)> = self.with_database(|database| {
            database
                .query_row(
                    "SELECT digest, bytes, files, fetched_at FROM remotes
                         WHERE locator = ?1 AND kind = ?2",
                    params![locator, kind.name()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
        })??;

        let (row, from_store) = match row {
            Some(row) => (Some(row), false),
            None => (self.published_remote(locator, kind)?, true),
        };
        let Some((digest, bytes, files, fetched_at)) = row else {
            return Ok(None);
        };
        // A row this version cannot read is treated as absent, which costs
        // one fetch and cannot serve a half-understood answer.
        let Some(digest) = ContentHash::parse(&digest) else {
            return Ok(None);
        };
        let Some(bytes) = ContentHash::parse(&bytes) else {
            return Ok(None);
        };
        let row = RemoteRow {
            digest,
            bytes,
            files: decode_remote_files(&files),
            fetched_at,
        };
        if from_store {
            self.remember_remote(locator, kind, &row)?;
        }
        Ok(Some(row))
    }

    /// What the store says this locator resolved to, if it says anything.
    ///
    /// Without this a clean checkout pointed at a lab's bucket would find
    /// every object it needed sitting there and download the reads again
    /// anyway, because nothing would connect `sra:SRR8525904` to the
    /// objects it produced.
    fn published_remote(
        &self,
        locator: &str,
        kind: DataKind,
    ) -> Result<Option<(String, String, String, i64)>, CacheError> {
        let Some(fields) =
            published::fetch(&self.objects, REMOTES, &remote_row_key(locator, kind))?
        else {
            return Ok(None);
        };
        let [digest, bytes, files, fetched_at] = fields.as_slice() else {
            return Ok(None);
        };
        Ok(Some((
            digest.clone(),
            bytes.clone(),
            files.clone(),
            fetched_at.parse().unwrap_or_default(),
        )))
    }

    /// Record what was fetched for this locator, read as this kind.
    ///
    /// Replaces any earlier record of the same locator and kind. That is
    /// right for a *pinned* remote, where a later fetch of the same locator
    /// is either the same answer or a failure; it is emphatically not how a
    /// recorded query behaves, and that is why the two are different tables
    /// rather than one with a flag (D39).
    ///
    /// # Errors
    ///
    /// If the metadata cannot be written.
    pub fn record_remote(
        &self,
        locator: &str,
        kind: DataKind,
        row: &RemoteRow,
    ) -> Result<(), CacheError> {
        self.remember_remote(locator, kind, row)?;
        published::publish(
            &self.objects,
            REMOTES,
            &remote_row_key(locator, kind),
            &[
                row.digest.to_hex(),
                row.bytes.to_hex(),
                encode_remote_files(&row.files),
                row.fetched_at.to_string(),
            ],
        )?;
        Ok(())
    }

    /// The index half of [`record_remote`](Self::record_remote).
    fn remember_remote(
        &self,
        locator: &str,
        kind: DataKind,
        row: &RemoteRow,
    ) -> Result<(), CacheError> {
        self.with_database(|database| {
            database.execute(
                "INSERT OR REPLACE INTO remotes
                     (locator, kind, digest, bytes, files, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    locator,
                    kind.name(),
                    row.digest.to_hex(),
                    row.bytes.to_hex(),
                    encode_remote_files(&row.files),
                    row.fetched_at,
                ],
            )
        })??;
        Ok(())
    }

    /// Forget what was fetched for this locator, read as this kind.
    ///
    /// For a fetch that turned out not to be what was promised: a record of
    /// it would be a record of the wrong bytes, which a later run might
    /// serve without asking.
    ///
    /// # Errors
    ///
    /// If the metadata cannot be written.
    pub fn forget_remote(&self, locator: &str, kind: DataKind) -> Result<(), CacheError> {
        self.with_database(|database| {
            database.execute(
                "DELETE FROM remotes WHERE locator = ?1 AND kind = ?2",
                params![locator, kind.name()],
            )
        })??;
        Ok(())
    }

    /// Every recording of this request, oldest revision first.
    ///
    /// All of them rather than only the current one, because a refresh
    /// never replaces what it refreshes: an earlier run replays from the
    /// revision it recorded, and a drift report names both.
    ///
    /// # Errors
    ///
    /// If the metadata cannot be read.
    pub fn query_records(&self, request: ContentHash) -> Result<Vec<QueryRecord>, CacheError> {
        let rows: Vec<Row> = self.with_database(|database| {
            let mut statement = database.prepare(
                "SELECT revision, recorded_at, locator, kind, method, url, status,
                        body, suffix, digest, notes
                     FROM records WHERE request = ?1 ORDER BY revision",
            )?;
            statement
                .query_map(params![request.to_hex()], |row| -> rusqlite::Result<Row> {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                    ))
                })?
                .collect::<rusqlite::Result<Vec<Row>>>()
        })??;

        Ok(rows
            .into_iter()
            .filter_map(|row| decode_query_record(request, row))
            .collect())
    }

    /// The current recording of this request, if there is one.
    ///
    /// The highest revision, which is what a replay serves.
    ///
    /// # Errors
    ///
    /// If the metadata cannot be read.
    pub fn query_record(&self, request: ContentHash) -> Result<Option<QueryRecord>, CacheError> {
        Ok(self.query_records(request)?.pop())
    }

    /// Record a response, as the next revision of its request.
    ///
    /// The revision is assigned here rather than by the caller, so that
    /// "a refresh writes beside" cannot be got wrong by whoever is doing
    /// the refreshing. Returns the record as it was written.
    ///
    /// # Errors
    ///
    /// If the metadata cannot be read or written.
    pub fn record_query(&self, record: &QueryRecord) -> Result<QueryRecord, CacheError> {
        let next = self
            .query_records(record.request)?
            .last()
            .map_or(1, |previous| previous.revision + 1);
        let written = QueryRecord {
            revision: next,
            ..record.clone()
        };
        self.with_database(|database| {
            database.execute(
                "INSERT INTO records
                     (request, revision, recorded_at, locator, kind, method, url,
                      status, body, suffix, digest, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    written.request.to_hex(),
                    written.revision,
                    written.recorded_at,
                    written.locator,
                    written.kind.name(),
                    written.method.name(),
                    written.url,
                    written.status,
                    written.body.hash.to_hex(),
                    written.body.suffix,
                    written.digest.to_hex(),
                    encode_notes(&written.notes),
                ],
            )
        })??;
        Ok(written)
    }

    /// Every recorded response this cache holds, oldest first.
    ///
    /// For a bundle, which carries them so that a reviewer with no network
    /// and no API key can replay the run (D39).
    ///
    /// # Errors
    ///
    /// If the metadata cannot be read.
    pub fn all_query_records(&self) -> Result<Vec<QueryRecord>, CacheError> {
        let requests: Vec<String> = self.with_database(|database| {
            let mut statement =
                database.prepare("SELECT DISTINCT request FROM records ORDER BY request")?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<String>>>()
        })??;

        let mut records = Vec::new();
        for request in requests {
            let Some(request) = ContentHash::parse(&request) else {
                continue;
            };
            records.extend(self.query_records(request)?);
        }
        Ok(records)
    }

    /// The blob tier this cache keeps its objects in.
    ///
    /// Exposed because a recorded remote is written here rather than through
    /// a step's output, and because a run report has to be able to say what
    /// the store did.
    #[must_use]
    pub fn objects(&self) -> &TieredStore {
        &self.objects
    }

    /// Every object this index refers to, in no particular order.
    ///
    /// What `ligase store pull` fetches, and what makes it a bounded job
    /// rather than a bucket scan: a store cannot be listed cheaply, and
    /// the index is the one thing that knows which objects this project's
    /// runs actually depend on.
    ///
    /// Note what this means for a cold machine: an empty index refers to
    /// nothing, so there is nothing to pull. `ligase run` is what warms a
    /// cold machine, because the run is what knows the keys it needs. The
    /// help text says so.
    ///
    /// # Errors
    ///
    /// If the metadata cannot be read.
    pub fn referenced_objects(&self) -> Result<Vec<ObjectKey>, CacheError> {
        let mut keys = Vec::new();

        let steps: Vec<(String, String)> = self.with_database(|database| {
            let mut statement = database.prepare("SELECT output_hash, suffix FROM steps")?;
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })??;
        for (hex, suffix) in steps {
            if let Some(hash) = ContentHash::parse(&hex) {
                keys.push(ObjectKey::new(hash, &suffix));
            }
        }

        let remotes: Vec<String> = self.with_database(|database| {
            let mut statement = database.prepare("SELECT files FROM remotes")?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })??;
        for files in remotes {
            keys.extend(decode_remote_files(&files).into_iter().map(|file| file.key));
        }

        let bodies: Vec<(String, String)> = self.with_database(|database| {
            let mut statement = database.prepare("SELECT body, suffix FROM records")?;
            statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()
        })??;
        for (hex, suffix) in bodies {
            if let Some(hash) = ContentHash::parse(&hex) {
                keys.push(ObjectKey::new(hash, &suffix));
            }
        }

        keys.sort_unstable();
        keys.dedup();
        Ok(keys)
    }

    /// Where a step should write while it runs.
    ///
    /// Inside the cache directory, so moving the finished file into the
    /// store is a rename rather than a copy, and so a crash leaves its
    /// debris somewhere a user can delete wholesale.
    #[must_use]
    pub fn staging(&self, key: StepKey, suffix: &str) -> PathBuf {
        self.root.join("tmp").join(format!("{key}{suffix}"))
    }

    /// Where something that is not a step's output should be written.
    ///
    /// For [`crate::audit`], whose two runs are thrown away: beside the
    /// staging files, so the same single directory holds everything a crash
    /// could leave behind.
    #[must_use]
    pub fn scratch(&self, name: &str) -> PathBuf {
        self.root.join("tmp").join(name)
    }

    /// What a step with this key produced, if it has been run before.
    ///
    /// An entry whose object has gone missing — a store deleted by hand,
    /// say — is treated as no entry at all and forgotten.
    ///
    /// # Errors
    ///
    /// If the metadata cannot be read.
    pub fn lookup(&self, key: StepKey) -> Result<Option<Entry>, CacheError> {
        let row: Option<(String, String, i64, String)> = self.with_database(|database| {
            database
                .query_row(
                    "SELECT output_hash, suffix, duration_ms, notes FROM steps WHERE key = ?1",
                    params![key.to_hex()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .optional()
        })??;

        // An index miss is not the end of the question. Everything a hit
        // depends on is published into the store, so a machine whose
        // `cache.sqlite` has been deleted -- or which never had one, being
        // a fresh checkout pointed at a lab's bucket -- can learn it back
        // (D43).
        let (row, from_store) = match row {
            Some(row) => (Some(row), false),
            None => (self.published_entry(key)?, true),
        };

        let Some((hex, suffix, duration_ms, notes)) = row else {
            return Ok(None);
        };
        let Some(hash) = ContentHash::parse(&hex) else {
            self.forget(key)?;
            return Ok(None);
        };

        // Through the store rather than off the filesystem, because the
        // object may be a tier down. `local_path` fetches it, verifies it
        // against the name it was asked for, and only then installs it.
        let object = ObjectKey::new(hash, &suffix);
        let path = match self.objects.local_path(&object) {
            Ok(Some(path)) => path,
            // Gone, or refused for a hash mismatch. Either way there is
            // nothing to serve, so the entry is dropped and the step runs.
            // A refusal is *not* silent: it is counted and named by the
            // store, and the run report prints it.
            Ok(None) | Err(StoreError::Mismatch { .. }) => {
                self.forget(key)?;
                return Ok(None);
            }
            Err(error) => return Err(error.into()),
        };

        if from_store {
            // Learned once, remembered locally. The rebuild is lazy, so a
            // cold machine pays one store read per step it actually needs
            // rather than a scan of the whole bucket.
            self.remember_published(key, &hex, &suffix, duration_ms, &notes)?;
        }

        Ok(Some(Entry {
            output: Stored { hash, path },
            duration_ms: duration_ms.try_into().unwrap_or_default(),
            notes: decode_notes(&notes),
        }))
    }

    /// The entry the store holds for this key, if it holds one.
    fn published_entry(
        &self,
        key: StepKey,
    ) -> Result<Option<(String, String, i64, String)>, CacheError> {
        let Some(fields) = published::fetch(&self.objects, STEPS, &key.to_hex())? else {
            return Ok(None);
        };
        let [hash, suffix, duration, notes] = fields.as_slice() else {
            return Ok(None);
        };
        Ok(Some((
            hash.clone(),
            suffix.clone(),
            duration.parse().unwrap_or_default(),
            notes.clone(),
        )))
    }

    /// Write back into the index what the store taught us.
    fn remember_published(
        &self,
        key: StepKey,
        hash: &str,
        suffix: &str,
        duration_ms: i64,
        notes: &str,
    ) -> Result<(), CacheError> {
        self.with_database(|database| {
            database.execute(
                "INSERT OR REPLACE INTO steps
                     (key, step, output_hash, suffix, tools, duration_ms, created_at, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    key.to_hex(),
                    // The step's name and its tools are not published:
                    // they describe how the work was done and nothing a
                    // hit depends on reads them back. A row rebuilt from
                    // the store says so rather than inventing them.
                    "(from the store)",
                    hash,
                    suffix,
                    "",
                    duration_ms,
                    now(),
                    notes,
                ],
            )
        })??;
        Ok(())
    }

    /// Record that a step with this key produced this output.
    ///
    /// # Errors
    ///
    /// If the metadata cannot be written.
    pub fn record(
        &self,
        key: StepKey,
        step: &str,
        output: &Stored,
        tools: &str,
        duration_ms: u64,
        notes: &[Note],
    ) -> Result<(), CacheError> {
        let duration = i64::try_from(duration_ms).unwrap_or(i64::MAX);
        self.with_database(|database| {
            database.execute(
                "INSERT OR REPLACE INTO steps
                     (key, step, output_hash, suffix, tools, duration_ms, created_at, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    key.to_hex(),
                    step,
                    output.hash.to_hex(),
                    suffix(&output.path),
                    tools,
                    duration,
                    now(),
                    encode_notes(notes),
                ],
            )
        })??;

        // And into the store, so the next machine to compute this key
        // finds the answer rather than the ingredients (D43). Failing to
        // publish is not failing the run: the work is done and the row is
        // local, and `ligase store push` makes it up.
        published::publish(
            &self.objects,
            STEPS,
            &key.to_hex(),
            &[
                output.hash.to_hex(),
                suffix(&output.path),
                duration.to_string(),
                encode_notes(notes),
            ],
        )?;
        Ok(())
    }

    fn forget(&self, key: StepKey) -> Result<(), CacheError> {
        self.with_database(|database| {
            database.execute("DELETE FROM steps WHERE key = ?1", params![key.to_hex()])
        })??;
        Ok(())
    }

    fn remembered_hash(&self, path: &str, stat: Stat) -> Result<Option<ContentHash>, CacheError> {
        let hex: Option<String> = self.with_database(|database| {
            database
                .query_row(
                    "SELECT hash FROM files
                         WHERE path = ?1 AND size = ?2 AND mtime_ns = ?3",
                    params![path, stat.size, stat.mtime_ns],
                    |row| row.get(0),
                )
                .optional()
        })??;
        Ok(hex.as_deref().and_then(ContentHash::parse))
    }

    fn remembered_canonical(
        &self,
        raw: ContentHash,
        kind: DataKind,
    ) -> Result<Option<Canonical>, CacheError> {
        let held: Option<(String, String, String)> = self.with_database(|database| {
            database
                .query_row(
                    "SELECT canonical, form, notes FROM canonical
                         WHERE raw = ?1 AND kind = ?2",
                    params![raw.to_hex(), kind.name()],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .optional()
        })??;

        let Some((hex, form, notes)) = held else {
            return Ok(None);
        };
        // A row this version cannot read is treated as absent, which costs
        // one canonicalisation and cannot report the wrong form.
        let Some(form) = Form::parse(&form) else {
            return Ok(None);
        };
        Ok(ContentHash::parse(&hex).map(|hash| Canonical {
            hash,
            form,
            notes: decode_notes(&notes),
        }))
    }

    fn remember_canonical(
        &self,
        raw: ContentHash,
        kind: DataKind,
        canonical: &Canonical,
    ) -> Result<(), CacheError> {
        self.with_database(|database| {
            database.execute(
                "INSERT OR REPLACE INTO canonical (raw, kind, canonical, form, notes)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    raw.to_hex(),
                    kind.name(),
                    canonical.hash.to_hex(),
                    canonical.form.to_string(),
                    encode_notes(&canonical.notes),
                ],
            )
        })??;
        Ok(())
    }

    fn remember_hash(&self, path: &str, stat: Stat, hash: ContentHash) -> Result<(), CacheError> {
        self.with_database(|database| {
            database.execute(
                "INSERT OR REPLACE INTO files (path, size, mtime_ns, hash)
                 VALUES (?1, ?2, ?3, ?4)",
                params![path, stat.size, stat.mtime_ns, hash.to_hex()],
            )
        })??;
        Ok(())
    }

    fn with_database<T>(&self, work: impl FnOnce(&Connection) -> T) -> Result<T, CacheError> {
        let database = self.database.lock().map_err(|_| CacheError::Poisoned)?;
        Ok(work(&database))
    }
}

/// What the prefilter compares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stat {
    size: i64,
    mtime_ns: i64,
}

fn stat(path: &Path) -> Result<Stat, CacheError> {
    let metadata = fs::metadata(path).map_err(|source| CacheError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mtime_ns = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_nanos()).ok())
        // A file whose mtime cannot be read is one the prefilter never
        // matches, which costs a rehash and risks nothing.
        .unwrap_or(-1);
    Ok(Stat {
        size: i64::try_from(metadata.len()).unwrap_or(i64::MAX),
        mtime_ns,
    })
}

fn now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    seconds.to_string()
}

fn create_dir(path: &Path) -> Result<(), CacheError> {
    fs::create_dir_all(path).map_err(|source| CacheError::Io {
        path: path.to_path_buf(),
        source,
    })
}

/// Notes as one column: `code<TAB>value`, one note a line.
///
/// Both SAM and VCF forbid a tab or a newline inside a field, so a note's
/// value cannot hold either and nothing has to be escaped. A value that
/// somehow did would lose its tail rather than corrupt the row.
/// One object per line: key, the URL that answered, and the byte count.
///
/// Tab-separated and readable with `sqlite3`, because a cache whose whole
/// purpose is explaining a result should be explicable itself.
/// One `records` row as SQLite hands it back.
///
/// A named alias because the tuple is eleven columns wide and a signature
/// spelling it out twice would be two places for it to drift.
type Row = (
    u32,
    i64,
    String,
    String,
    String,
    String,
    u16,
    String,
    String,
    String,
    String,
);

/// A `records` row as a [`QueryRecord`].
///
/// `None` for a row this version cannot read -- an unknown kind or method,
/// a hash that will not parse -- which is treated as no record at all. That
/// costs one fetch and cannot serve a half-understood answer, which is how
/// `remote` already treats a row it does not recognise.
fn decode_query_record(request: ContentHash, row: Row) -> Option<QueryRecord> {
    let (revision, recorded_at, locator, kind, method, url, status, body, suffix, digest, notes) =
        row;
    Some(QueryRecord {
        request,
        revision,
        recorded_at,
        locator,
        kind: DataKind::ALL
            .iter()
            .copied()
            .find(|known| known.name() == kind)?,
        method: Method::parse(&method)?,
        url,
        status,
        body: ObjectKey::new(ContentHash::parse(&body)?, &suffix),
        digest: ContentHash::parse(&digest)?,
        notes: decode_notes(&notes),
    })
}

fn encode_remote_files(files: &[RemoteFile]) -> String {
    files
        .iter()
        .map(|file| format!("{}\t{}\t{}", file.key, file.url, file.bytes))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The objects a column holds, dropping any line this version cannot read.
fn decode_remote_files(column: &str) -> Vec<RemoteFile> {
    column
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\t');
            let key = parts.next()?;
            let url = parts.next()?;
            let bytes = parts.next()?.parse().ok()?;
            let hash = ContentHash::parse(key.get(..64)?)?;
            Some(RemoteFile {
                key: ObjectKey::new(hash, key.get(64..).unwrap_or_default()),
                url: url.to_owned(),
                bytes,
            })
        })
        .collect()
}

/// What a published `remotes` row is keyed on.
///
/// Locator *and* kind, exactly as the index is: the same URL read as two
/// kinds is two facts, because a digest is over what a file means and what
/// it means depends on how it is read.
fn remote_row_key(locator: &str, kind: DataKind) -> String {
    format!("{locator}\t{}", kind.name())
}

fn encode_notes(notes: &[Note]) -> String {
    notes
        .iter()
        .map(|note| format!("{}\t{}", note.code(), note.value()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The notes a column holds, skipping any this version does not know.
fn decode_notes(column: &str) -> Vec<Note> {
    column
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .filter_map(|(code, value)| Note::parse(code, value))
        .collect()
}

/// Create the tables, discarding anything written under another format.
fn prepare(database: &Connection) -> Result<(), CacheError> {
    database.execute(
        "CREATE TABLE IF NOT EXISTS meta (name TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    )?;

    let schema: Option<String> = database
        .query_row(
            "SELECT value FROM meta WHERE name = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if schema.is_some_and(|version| version != SCHEMA_VERSION.to_string()) {
        database.execute("DROP TABLE IF EXISTS files", [])?;
        database.execute("DROP TABLE IF EXISTS steps", [])?;
        database.execute("DROP TABLE IF EXISTS canonical", [])?;
        database.execute("DROP TABLE IF EXISTS remotes", [])?;
        database.execute("DROP TABLE IF EXISTS records", [])?;
    }

    database.execute(
        "CREATE TABLE IF NOT EXISTS files (
             path     TEXT PRIMARY KEY,
             size     INTEGER NOT NULL,
             mtime_ns INTEGER NOT NULL,
             hash     TEXT NOT NULL
         )",
        [],
    )?;
    database.execute(
        "CREATE TABLE IF NOT EXISTS steps (
             key         TEXT PRIMARY KEY,
             step        TEXT NOT NULL,
             output_hash TEXT NOT NULL,
             suffix      TEXT NOT NULL,
             tools       TEXT NOT NULL,
             duration_ms INTEGER NOT NULL,
             created_at  TEXT NOT NULL,
             notes       TEXT NOT NULL DEFAULT ''
         )",
        [],
    )?;

    // What was fetched from where, and what it turned out to be (D39).
    // Keyed by locator and kind, because the same URL read as two kinds is
    // two facts: a digest is over what a file *means*, and what it means
    // depends on how it is read.
    //
    // `files` holds one line per object -- key, the URL that answered, and
    // the byte count -- so a run's inputs can be read back with `sqlite3`
    // by anyone explaining a result.
    database.execute(
        "CREATE TABLE IF NOT EXISTS remotes (
             locator    TEXT NOT NULL,
             kind       TEXT NOT NULL,
             digest     TEXT NOT NULL,
             bytes      TEXT NOT NULL,
             files      TEXT NOT NULL,
             fetched_at INTEGER NOT NULL,
             PRIMARY KEY (locator, kind)
         )",
        [],
    )?;

    // Every recorded response, keyed by the hash of its canonical request
    // (D39). A separate table from `remotes` rather than one with a flag,
    // and the difference is the whole of why: `remotes` is
    // `INSERT OR REPLACE`, because a later fetch of a pinned locator is
    // either the same answer or a failure, and a refresh of a query must
    // *never* replace what it refreshes -- every earlier run stays
    // replayable.
    //
    // So the key is `(request, revision)` and a refresh inserts the next
    // revision. Not `(request, recorded_at)`: two records a second apart
    // must still be two records, and keying on a clock would let a refresh
    // inside the same second overwrite the thing it exists not to
    // overwrite. Nothing supersedes anything explicitly either -- revision
    // `n` supersedes `n - 1` by construction, which cannot drift out of
    // step with itself.
    database.execute(
        "CREATE TABLE IF NOT EXISTS records (
             request     TEXT NOT NULL,
             revision    INTEGER NOT NULL,
             recorded_at INTEGER NOT NULL,
             locator     TEXT NOT NULL,
             kind        TEXT NOT NULL,
             method      TEXT NOT NULL,
             url         TEXT NOT NULL,
             status      INTEGER NOT NULL,
             body        TEXT NOT NULL,
             suffix      TEXT NOT NULL,
             digest      TEXT NOT NULL,
             notes       TEXT NOT NULL DEFAULT '',
             PRIMARY KEY (request, revision)
         )",
        [],
    )?;

    // What a file means, against the hash of its bytes. Not keyed by path:
    // this is a fact about the bytes, so a file copied or produced again
    // identically is not parsed a second time.
    //
    // `notes` holds what canonicalising the file noticed, which is almost
    // always nothing. It is stored rather than recomputed because a cache
    // hit does not reopen the file, and a warning that stopped appearing on
    // the second run would be worse than none at all.
    database.execute(
        "CREATE TABLE IF NOT EXISTS canonical (
             raw       TEXT NOT NULL,
             kind      TEXT NOT NULL,
             canonical TEXT NOT NULL,
             form      TEXT NOT NULL,
             notes     TEXT NOT NULL DEFAULT '',
             PRIMARY KEY (raw, kind)
         )",
        [],
    )?;

    // Keys from another key format describe ingredients that were combined
    // differently. The files table survives: a file's hash is a fact about
    // the file, not about how keys are made.
    let keys: Option<String> = database
        .query_row(
            "SELECT value FROM meta WHERE name = 'key_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if keys.is_some_and(|version| version != KEY_VERSION.to_string()) {
        database.execute("DELETE FROM steps", [])?;
    }

    // A canonical hash is a fact about a file *given the rules that made
    // it*, so rules from another version describe something else. The step
    // entries go with it, since their keys were built from those hashes.
    let canon: Option<String> = database
        .query_row(
            "SELECT value FROM meta WHERE name = 'canon_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if canon.is_some_and(|version| version != CANON_VERSION.to_string()) {
        database.execute("DELETE FROM canonical", [])?;
        database.execute("DELETE FROM steps", [])?;
    }

    database.execute(
        "INSERT OR REPLACE INTO meta (name, value) VALUES
             ('schema_version', ?1), ('key_version', ?2), ('canon_version', ?3)",
        params![
            SCHEMA_VERSION.to_string(),
            KEY_VERSION.to_string(),
            CANON_VERSION.to_string(),
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn a_suffix_keeps_every_extension_a_tool_reads() {
        assert_eq!(suffix(Path::new("out.bam")), ".bam");
        assert_eq!(suffix(Path::new("out.vcf.gz")), ".vcf.gz");
        assert_eq!(suffix(Path::new("out")), "");
        assert_eq!(suffix(Path::new("/tmp/a.b/out.tsv")), ".tsv");
    }
}
