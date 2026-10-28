//! A recorded query: the same bytes forever, from the record.
//!
//! A live query's source promises nothing. Open Targets will answer
//! differently next quarter, because it is supposed to — new evidence
//! arrives — and pretending otherwise is how a paper becomes
//! unreproducible without anyone noticing. So a recorded query does not
//! offer *source* reproducibility and does not pretend to. It offers
//! *record* reproducibility: anyone with the bundle runs the pipeline again
//! and gets the same result.
//!
//! That needs exactly one thing, and it is the whole mechanism — the
//! canonicalised response **body** goes into the content-addressed store,
//! not merely its digest. The body is then an ordinary input with a content
//! hash, everything downstream is deterministic by construction, and a
//! reviewer replays the run with no network and no API key (D39).
//!
//! # Three modes, set per run
//!
//! | [`Mode`] | What happens |
//! |---|---|
//! | [`Mode::Auto`] | replay a record that exists, record one that does not. The default |
//! | [`Mode::Replay`] | replay only: a query with no record is an **error** |
//! | [`Mode::Refresh`] | fetch live anyway, compare against the record, report *drift* |
//!
//! A mode is a per-run setting rather than a per-step one, because a run
//! that replayed some of its queries and fetched the rest is the
//! plausible-but-wrong result this engine exists to refuse. In
//! [`Mode::Replay`] a query with no record is therefore an error and never
//! a silent live fetch.
//!
//! # A refresh never replaces a record
//!
//! It writes a new dated one beside the old, so every earlier run stays
//! replayable and a drift report can name both. A surveillance pipeline
//! refreshing weekly is not reproducing one run; it is a series of runs,
//! each with its own record, each replayable forever.
//!
//! The one exception is not an exception to that: a refresh that finds the
//! *same* bytes writes no new revision, because the record that already
//! exists is the record of those bytes and a second one would add a row
//! and no replay.
//!
//! # Identity, and what a record is not
//!
//! A record is keyed by its canonical request (see
//! [`super::request`]), never by the order it was asked in, so a step
//! making twenty paginated calls writes twenty records and finds them again
//! whatever order it asks in next time.
//!
//! A record holds no request headers, so it holds no token. See
//! [`super::fetch`] for why that is structural rather than a filter.

use std::fmt;
use std::path::PathBuf;

use crate::cache::{Cache, CacheError, QueryRecord};
use crate::canon::Form;
use crate::hash::ContentHash;
use crate::note::Note;
use crate::remote::fetch::{FetchError, Transport};
use crate::remote::locator::{Locator, LocatorError};
use crate::remote::request::{Auth, CanonicalRequest, Request};
use crate::remote::response;
use crate::store::{BlobStore, ObjectKey, StoreError};
use crate::types::{DataKind, Origin, RemoteVerdict};

/// The largest response body that may be recorded, in bytes.
///
/// A response bigger than this is a dataset rather than an answer, and a
/// dataset has a version to pin — so it belongs in `lg.remote` with a
/// digest, where it is reproducible from the source rather than from
/// somebody's bundle. Copying 32 MiB into every bundle is already
/// generous; copying a genome into one buys nothing that a pin does not buy
/// better (spec §8 Q4).
///
/// This is a *recorded* cap only. [`crate::remote::fetch::Policy`]'s own
/// limit stays generous, because a pinned SRA run is legitimately
/// gigabytes.
pub const MAX_RECORDED_BYTES: u64 = 32 * 1024 * 1024;

/// How this run treats its recorded queries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Replay a record that exists; record one that does not.
    ///
    /// The default, and what a plain script gets. The run report always
    /// says which of the two each query did, because "it worked" is not the
    /// same statement as "it worked without asking anybody".
    #[default]
    Auto,
    /// Replay only. A query with no record is an error.
    ///
    /// What a reviewer wants, and what makes "this run touched no network"
    /// a thing the engine asserts rather than something the user hopes.
    Replay,
    /// Fetch live anyway, and compare against the record.
    Refresh,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Auto => "auto",
            Self::Replay => "replay",
            Self::Refresh => "refresh",
        })
    }
}

/// What a refresh found when it compared the source with the record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drift {
    /// There was no record to compare against, so this is the first one.
    ///
    /// Not drift. A refresh that reported every query as changed the first
    /// time it ran would teach a user to ignore the report, which is worse
    /// than not having one.
    First,
    /// The source still says what the record says.
    Unchanged,
    /// The source says something else, and both records are kept.
    Changed {
        /// The body the earlier record holds.
        was: ContentHash,
        /// The body the new record holds.
        now: ContentHash,
        /// The revision that was current before this refresh.
        previous: u32,
        /// The revision this refresh wrote.
        revision: u32,
        /// When the earlier record was made, as seconds since the epoch.
        previous_at: i64,
    },
}

/// Where a query's answer came from on this run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Served {
    /// No record existed, so the source was asked and the answer recorded.
    Recorded,
    /// A record existed and was served, with no network touched.
    Replayed {
        /// When the record was made, as seconds since the epoch — which is
        /// what "replayed from record of <date>" prints.
        at: i64,
        /// Which revision was served.
        revision: u32,
    },
    /// The source was asked although a record existed, and the two
    /// compared.
    Refreshed {
        /// What the comparison found.
        drift: Drift,
    },
}

impl Served {
    /// What this run's answer rests on, as an audit verdict.
    ///
    /// Live for the run that writes a record and for a refresh, replayed
    /// for every run that serves one. Honest rather than reassuring: the
    /// run that made the record did reach the network, and saying so is
    /// what makes the *next* run's `replayed` worth reading.
    #[must_use]
    pub const fn verdict(&self) -> RemoteVerdict {
        match self {
            Self::Recorded | Self::Refreshed { .. } => RemoteVerdict::Live,
            Self::Replayed { .. } => RemoteVerdict::Replayed,
        }
    }
}

impl fmt::Display for Served {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Recorded => f.write_str("recorded"),
            Self::Replayed { at, .. } => {
                write!(f, "replayed from record of {}", crate::prov::day(*at))
            }
            Self::Refreshed { drift } => match drift {
                Drift::First => f.write_str("refreshed: a first record, so nothing to compare"),
                Drift::Unchanged => f.write_str("refreshed: unchanged"),
                Drift::Changed {
                    previous,
                    revision,
                    previous_at,
                    ..
                } => write!(
                    f,
                    "refreshed: changed since the record of {} \
                     (revision {previous} kept, revision {revision} written)",
                    crate::prov::day(*previous_at)
                ),
            },
        }
    }
}

/// A query, resolved to a body on this machine.
#[derive(Clone, Debug)]
pub struct ResolvedQuery {
    /// The recorded body, inside the content-addressed store.
    pub path: PathBuf,
    /// The digest of what it means, read as its kind — which is what
    /// enters a step key.
    pub digest: ContentHash,
    /// The record this run served or wrote.
    pub record: QueryRecord,
    /// Where the answer came from.
    pub served: Served,
    /// What resolving it noticed (D26).
    pub notes: Vec<Note>,
}

impl ResolvedQuery {
    /// The summary that travels with the loaded dataset.
    #[must_use]
    pub fn origin(&self) -> Origin {
        Origin {
            locator: self.record.locator.clone(),
            digest: self.record.digest,
            bytes: self.record.body.hash,
            // The record's date rather than this run's: what a reader wants
            // to know about a replayed query is when the answer was
            // obtained, not when it was read back.
            fetched_at: self.record.recorded_at,
            verdict: self.served.verdict(),
        }
    }
}

/// Why a query could not be resolved.
#[derive(Debug)]
pub enum QueryError {
    /// The locator could not be read, or could not be turned into a URL.
    Locator(LocatorError),
    /// The source could not be reached, or did not answer with bytes.
    Fetch {
        /// What was asked for.
        locator: String,
        /// What went wrong.
        source: FetchError,
    },
    /// This run may only replay, and there is no record of this query.
    ///
    /// The refusal `--replay` exists to produce. Never a silent live fetch:
    /// a run that replayed some of its queries and quietly fetched the rest
    /// is the plausible-but-wrong result this engine exists to refuse.
    NoRecord {
        /// What was asked for.
        locator: String,
        /// The canonical request that was looked for, so the record can be
        /// found by hand in a store that has one.
        request: ContentHash,
    },
    /// The answer is too big to record.
    TooLarge {
        /// What was asked for.
        locator: String,
        /// The limit it exceeded, in bytes.
        limit: u64,
    },
    /// What arrived cannot be read as the kind it was declared to be.
    ///
    /// A 200 is not a promise about the body: an API asked for something it
    /// does not have answers with an error page and a success, and reading
    /// the body is the only thing between that page and a pipeline treating
    /// it as data.
    Unreadable {
        /// What was asked for.
        locator: String,
        /// How it was being read.
        kind: DataKind,
        /// Why it could not be.
        why: String,
    },
    /// The cache metadata could not be used.
    Cache(CacheError),
    /// An object could not be held, or is not what its name says.
    Store(StoreError),
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locator(error) => error.fmt(f),
            Self::Fetch { locator, source } => {
                write!(f, "`{locator}` could not be fetched: {source}")
            }
            Self::NoRecord { locator, request } => write!(
                f,
                "this run may only replay, and there is no record of \
                 `{locator}` (request {}). Run it once without `--replay` to \
                 make the record, or point it at a store or a bundle that \
                 has one -- a run that replayed some of its queries and \
                 fetched the rest would be reporting a result nobody can \
                 repeat",
                request.to_hex()
            ),
            Self::TooLarge { locator, limit } => write!(
                f,
                "`{locator}` answered with more than the {limit} bytes a \
                 recorded response may hold. An answer that large is a \
                 dataset rather than a query result, and a dataset has a \
                 version: pin this as a release instead, with \
                 `lg.remote(..., digest=...)`, which is reproducible from \
                 the source rather than from a copy in every bundle"
            ),
            Self::Unreadable { locator, kind, why } => write!(
                f,
                "`{locator}` answered but the answer cannot be read as {}: \
                 {why}. A 200 is not a promise about the body -- an API \
                 asked for something it does not have will answer with an \
                 error page",
                kind.name()
            ),
            Self::Cache(error) => error.fmt(f),
            Self::Store(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for QueryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Locator(error) => Some(error),
            Self::Fetch { source, .. } => Some(source),
            Self::Cache(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::NoRecord { .. } | Self::TooLarge { .. } | Self::Unreadable { .. } => None,
        }
    }
}

impl From<CacheError> for QueryError {
    fn from(error: CacheError) -> Self {
        Self::Cache(error)
    }
}

impl From<StoreError> for QueryError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<LocatorError> for QueryError {
    fn from(error: LocatorError) -> Self {
        Self::Locator(error)
    }
}

/// Resolve a query, by replaying a record or by making one.
///
/// `auth` is sent and never stored. `mode` decides what happens when a
/// record does or does not exist; see [`Mode`].
///
/// # Errors
///
/// [`QueryError`] for a locator that cannot be read, a source that cannot
/// be reached, an answer too big to record or unreadable as `kind`, or —
/// under [`Mode::Replay`] — a query with no record.
pub fn resolve(
    locator: &Locator,
    kind: DataKind,
    auth: Option<Auth>,
    mode: Mode,
    cache: &Cache,
    transport: &dyn Transport,
) -> Result<ResolvedQuery, QueryError> {
    let written = locator.to_string();
    // A query is one URL. The schemes that resolve to more than one -- an
    // SRA run is a pair of FASTQs -- are archives, which is what `lg.remote`
    // is for; a query against one would be asking an archive a question it
    // does not take.
    let url = locator
        .urls()?
        .into_iter()
        .next()
        .ok_or_else(|| LocatorError::Empty {
            locator: written.clone(),
        })?;
    let request = Request::get(&url).with_auth(auth);
    let canonical = CanonicalRequest::of(&request);
    let id = canonical.hash();

    let held = cache.query_records(id)?;
    let current = held.last();

    match mode {
        Mode::Replay => {
            let Some(record) = current.filter(|record| is_held(cache, record)) else {
                return Err(QueryError::NoRecord {
                    locator: written,
                    request: id,
                });
            };
            replay(cache, record)
        }
        Mode::Auto => match current.filter(|record| is_held(cache, record)) {
            Some(record) => replay(cache, record),
            None => {
                let written_record = record(&written, kind, id, &request, cache, transport)?;
                Ok(ResolvedQuery {
                    path: path_of(cache, &written_record.record)?,
                    digest: written_record.record.digest,
                    served: Served::Recorded,
                    notes: written_record.record.notes.clone(),
                    record: written_record.record,
                })
            }
        },
        Mode::Refresh => {
            let previous = current.cloned();
            let fetched = fetch(&written, kind, &request, cache, transport)?;

            // Identical bytes are already recorded by the record that
            // exists, so a second revision of them would add a row and no
            // replay. "Never replaces" is kept by never writing over
            // anything, not by writing something every time.
            if let Some(previous) = &previous
                && previous.body.hash == fetched.key.hash
            {
                let mut served = replay(cache, previous)?;
                served.served = Served::Refreshed {
                    drift: Drift::Unchanged,
                };
                return Ok(served);
            }

            let stored = keep(cache, id, &written, kind, &request, fetched)?;
            let drift = match &previous {
                None => Drift::First,
                Some(previous) => Drift::Changed {
                    was: previous.body.hash,
                    now: stored.digest_of_body(),
                    previous: previous.revision,
                    revision: stored.record.revision,
                    previous_at: previous.recorded_at,
                },
            };

            // Drift is *reported*, which means it travels with the dataset
            // rather than being computed and dropped. A note is how this
            // engine says what it saw (D26): it reaches the run report, and
            // a source that promises nothing is entitled to change its
            // answer, so the honest form is a note and not a failure.
            let mut notes = stored.record.notes.clone();
            if let Drift::Changed { was, now, .. } = &drift {
                notes.push(Note::QueryDrifted {
                    locator: written.clone(),
                    was: was.to_hex(),
                    now: now.to_hex(),
                });
            }

            Ok(ResolvedQuery {
                path: path_of(cache, &stored.record)?,
                digest: stored.record.digest,
                served: Served::Refreshed { drift },
                notes,
                record: stored.record,
            })
        }
    }
}

/// Whether a record's body is still in the store.
///
/// A record is a fact about a past response, not a promise that the object
/// is still there: a store emptied by hand leaves a row whose body is gone,
/// and serving it would be serving nothing.
fn is_held(cache: &Cache, record: &QueryRecord) -> bool {
    cache.objects().has(&record.body).unwrap_or(false)
}

/// Serve a record.
fn replay(cache: &Cache, record: &QueryRecord) -> Result<ResolvedQuery, QueryError> {
    // `checked_get` rather than `local_path`: a recorded body is at most 32
    // MiB, so verifying it costs almost nothing, and "nothing read from a
    // store is trusted" is worth more than the read. A store a lab shares
    // is a store other people can write to.
    if cache.objects().checked_get(&record.body)?.is_none() {
        return Err(QueryError::NoRecord {
            locator: record.locator.clone(),
            request: record.request,
        });
    }
    Ok(ResolvedQuery {
        path: path_of(cache, record)?,
        digest: record.digest,
        served: Served::Replayed {
            at: record.recorded_at,
            revision: record.revision,
        },
        notes: record.notes.clone(),
        record: record.clone(),
    })
}

/// A response fetched and canonicalised, before any of it is kept.
struct Canonicalised {
    key: ObjectKey,
    bytes: Vec<u8>,
    url: String,
    status: u16,
    digest: ContentHash,
    notes: Vec<Note>,
}

/// A record as it was written.
struct Written {
    record: QueryRecord,
}

impl Written {
    /// The hash of the body this record holds.
    const fn digest_of_body(&self) -> ContentHash {
        self.record.body.hash
    }
}

/// Fetch a query, canonicalise the answer, and read it as `kind`.
///
/// Nothing is kept here: a response that is too big, or that will not read
/// as its kind, leaves the cache exactly as it found it.
fn fetch(
    written: &str,
    kind: DataKind,
    request: &Request,
    cache: &Cache,
    transport: &dyn Transport,
) -> Result<Canonicalised, QueryError> {
    let staged = cache.scratch(&format!(
        "query-{}",
        ContentHash::of_bytes(request.url.as_bytes()).to_hex()
    ));
    let fetched = transport
        .fetch(request, &staged)
        .map_err(|source| match source {
            // The transport's own cap is set to the recorded limit for a
            // query, so this is the usual door and it says the same thing
            // the explicit check says.
            FetchError::TooLarge { .. } => QueryError::TooLarge {
                locator: written.to_owned(),
                limit: MAX_RECORDED_BYTES,
            },
            source => QueryError::Fetch {
                locator: written.to_owned(),
                source,
            },
        })?;

    // The second door, for a transport handed a looser policy than a query
    // wants: an oversized answer must not become a record whichever way it
    // arrived.
    if fetched.bytes > MAX_RECORDED_BYTES {
        let _ = std::fs::remove_file(&staged);
        return Err(QueryError::TooLarge {
            locator: written.to_owned(),
            limit: MAX_RECORDED_BYTES,
        });
    }

    let body = std::fs::read(&staged).map_err(|source| {
        QueryError::Cache(CacheError::Io {
            path: staged.clone(),
            source,
        })
    })?;
    let _ = std::fs::remove_file(&staged);

    // What the answer *means* rather than what its bytes are: JSON keys
    // sorted, the server's own bookkeeping dropped, and a body that is not
    // JSON standing in for itself (D21).
    let canonical = response::canonicalise(written, &body);

    // Read as its declared kind, in a temporary file, because that reading
    // is the only thing between an API's error page and a pipeline treating
    // it as data.
    let probe = cache.scratch(&format!(
        "query-canonical-{}",
        ContentHash::of_bytes(&canonical.bytes).to_hex()
    ));
    if let Some(parent) = probe.parent() {
        std::fs::create_dir_all(parent).map_err(|source| {
            QueryError::Cache(CacheError::Io {
                path: parent.to_path_buf(),
                source,
            })
        })?;
    }
    std::fs::write(&probe, &canonical.bytes).map_err(|source| {
        QueryError::Cache(CacheError::Io {
            path: probe.clone(),
            source,
        })
    })?;
    let meaning = cache.semantic(kind, &probe);
    let _ = std::fs::remove_file(&probe);
    let meaning = meaning.map_err(|error| QueryError::Unreadable {
        locator: written.to_owned(),
        kind,
        why: error.to_string(),
    })?;
    if meaning.form == Form::Raw {
        return Err(QueryError::Unreadable {
            locator: written.to_owned(),
            kind,
            why: "it does not parse as that kind, so its bytes would have \
                  had to stand in for what it means"
                .to_owned(),
        });
    }

    Ok(Canonicalised {
        // A recorded body has no extension to speak of and must not need an
        // invented one, which is why an empty suffix is legal in the store.
        key: ObjectKey::of_bytes(&canonical.bytes, &suffix_for(kind)),
        bytes: canonical.bytes,
        url: fetched.url,
        status: fetched.status,
        digest: meaning.hash,
        notes: canonical.notes,
    })
}

/// Put the body in the store and the record in the index.
fn keep(
    cache: &Cache,
    id: ContentHash,
    written: &str,
    kind: DataKind,
    request: &Request,
    fetched: Canonicalised,
) -> Result<Written, QueryError> {
    cache.objects().put(&fetched.key, &fetched.bytes)?;
    let record = cache.record_query(&QueryRecord {
        request: id,
        // Assigned by the cache, which is what stops "a refresh writes
        // beside" from being something each caller has to get right.
        revision: 0,
        recorded_at: crate::prov::now(),
        locator: written.to_owned(),
        kind,
        method: request.method,
        url: fetched.url,
        status: fetched.status,
        body: fetched.key,
        digest: fetched.digest,
        notes: fetched.notes,
    })?;
    Ok(Written { record })
}

/// Fetch, canonicalise and keep, in that order.
fn record(
    written: &str,
    kind: DataKind,
    id: ContentHash,
    request: &Request,
    cache: &Cache,
    transport: &dyn Transport,
) -> Result<Written, QueryError> {
    let fetched = fetch(written, kind, request, cache, transport)?;
    keep(cache, id, written, kind, request, fetched)
}

/// Where a record's body sits on this filesystem.
fn path_of(cache: &Cache, record: &QueryRecord) -> Result<PathBuf, QueryError> {
    Ok(cache
        .objects()
        .local_path(&record.body)?
        .unwrap_or_else(|| cache.objects().path_of(&record.body)))
}

/// The extension a recorded body is stored under.
///
/// Taken from the kind rather than from the URL, because a query's URL
/// usually ends in a path segment with no extension at all — and a loader
/// that reads a file's format off its suffix has to be handed a name it can
/// read.
fn suffix_for(kind: DataKind) -> String {
    match kind {
        DataKind::EntitySet
        | DataKind::RankedList
        | DataKind::MoleculeSet
        | DataKind::MoleculeRanking => ".tsv".to_owned(),
        DataKind::Reference => ".fasta".to_owned(),
        DataKind::Variants => ".vcf".to_owned(),
        DataKind::Intervals => ".bed".to_owned(),
        DataKind::Alignment => ".sam".to_owned(),
        DataKind::Reads => ".fastq".to_owned(),
        DataKind::KnowledgeGraph => ".json".to_owned(),
        DataKind::ResistanceReport => String::new(),
    }
}
