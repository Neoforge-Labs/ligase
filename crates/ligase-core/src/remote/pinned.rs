//! A pinned remote: the same bytes forever, from the source.
//!
//! An SRA run, a PDB entry, HXB2 by accession. The source promises not to
//! change, the user pins the promise with a digest, and the engine checks it
//! **before any step runs** — because a digest that is only checked after a
//! result has been produced is a digest that explains a wrong answer rather
//! than preventing one.
//!
//! # What a digest is over
//!
//! What the file *means*, not what its bytes are: the canonical form of the
//! fetched file read as its declared kind, folded over the files in order
//! (D4). For a kind with no canonical form the fold is over the bytes, which
//! needs no special case because canonicalisation already falls back to them
//! (D21).
//!
//! This is the one choice here worth arguing about, and the argument is
//! `scripts/fetch_test_data.sh`'s, which M10 deletes: it pinned HXB2 by the
//! checksum of its *sequence* rather than of the file, because "NCBI may
//! reflow lines or reword a defline without the biology changing, and that
//! must not break the build". An alarm that fires on a cosmetic reformat
//! teaches people to bump pins without reading, and then the one that
//! matters gets bumped too. So:
//!
//! - the canonical digest differs from the pin → **the remote moved**, a
//!   loud failure naming both digests;
//! - the canonical digest matches and the bytes differ → the source rewrote
//!   the file and the biology is unchanged, which is a note (D26), because
//!   that is the honest statement and "this pipeline is broken" is not.
//!
//! Folding even a single file means a one-file dataset and a paired one are
//! described the same way, and that swapping a pair's mates is a different
//! digest — which it must be, since it is a different dataset.
//!
//! # Why resolution happens at graph-build time
//!
//! A remote's dimensions cannot be known until its bytes are in hand: you
//! cannot type-check data you have not got. So a remote resolves when the
//! pipeline declares it, and the result is an ordinary file with ordinary
//! dimensions, which is also why "a wrong digest fails before any step runs"
//! needs no arranging. On a warm store resolution touches no network at all.

use std::fmt;
use std::path::PathBuf;

use crate::cache::{Cache, CacheError, RemoteFile, RemoteRow};
use crate::canon::Form;
use crate::hash::ContentHash;
use crate::note::Note;
use crate::remote::fetch::{FetchError, Transport};
use crate::remote::locator::{Locator, LocatorError};
use crate::store::{BlobStore, ObjectKey, StoreError};
use crate::types::DataKind;

/// Where a resolved remote's bytes came from on this run.
///
/// Which is also its audit verdict: a pinned remote served from the store is
/// as pinned as one just fetched, because the digest was checked either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Already held, with a digest that satisfies the declaration. No
    /// network was touched.
    Store,
    /// Fetched on this run.
    Network,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Store => "from the store",
            Self::Network => "fetched",
        })
    }
}

/// A remote, resolved to files on this machine.
#[derive(Clone, Debug)]
pub struct Resolved {
    /// The files, in order, inside the content-addressed store.
    pub paths: Vec<PathBuf>,
    /// The digest of what they mean, which is what a pin is compared
    /// against.
    pub digest: ContentHash,
    /// What was recorded about the fetch.
    pub record: Record,
    /// Whether this run touched the network.
    pub from: Source,
    /// What resolving it noticed (D26).
    pub notes: Vec<Note>,
}

/// What is known about a fetch, for provenance and the methods paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// The locator as the user wrote it, which is what a reader can look up.
    pub locator: String,
    /// How the files were read.
    pub kind: DataKind,
    /// The digest of what they mean.
    pub digest: ContentHash,
    /// The digest of their bytes.
    pub bytes: ContentHash,
    /// The objects, in order.
    pub files: Vec<RemoteFile>,
    /// When they were fetched, as seconds since the epoch.
    pub fetched_at: i64,
}

/// Why a remote could not be resolved.
#[derive(Debug)]
pub enum RemoteError {
    /// The locator could not be read, or could not be turned into a URL.
    Locator(LocatorError),
    /// The source could not be reached, or did not answer with bytes.
    Fetch {
        /// What was asked for.
        locator: String,
        /// What went wrong.
        source: FetchError,
    },
    /// What arrived is not what the pin says it should be.
    ///
    /// The failure a pin exists to produce. Loud, before any step runs, and
    /// naming both digests so the two can be compared rather than merely
    /// disagreed about.
    Moved {
        /// What was asked for.
        locator: String,
        /// The digest the pipeline declared.
        expected: ContentHash,
        /// The digest of what arrived.
        found: ContentHash,
        /// Whether the bytes alone moved, with the meaning intact.
        ///
        /// False here by construction — a meaning-preserving rewrite is a
        /// note rather than this error — and kept so the message can say so
        /// where it is true in some future caller.
        cosmetic: bool,
    },
    /// What arrived cannot be read as the kind it was declared to be.
    ///
    /// NCBI answers a bad accession with a 200 and an HTML page. Nothing in
    /// the status says so, and with no digest declared there is nothing to
    /// compare against, so reading the file is the only thing between that
    /// page and a pipeline treating it as a reference.
    Unreadable {
        /// What was asked for.
        locator: String,
        /// How it was being read.
        kind: DataKind,
        /// Why it could not be.
        why: String,
    },
    /// The store or the cache metadata could not be used.
    Cache(CacheError),
    /// An object could not be held, or is not what its name says.
    Store(StoreError),
}

impl fmt::Display for RemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Locator(error) => error.fmt(f),
            Self::Fetch { locator, source } => {
                write!(f, "`{locator}` could not be fetched: {source}")
            }
            Self::Moved {
                locator,
                expected,
                found,
                cosmetic,
            } => {
                write!(
                    f,
                    "the remote `{locator}` has moved: it was pinned to {} \
                     and it is now {}.",
                    expected.to_hex(),
                    found.to_hex()
                )?;
                if *cosmetic {
                    return f.write_str(
                        " Only its bytes differ; what it means is unchanged, \
                         so the pin can be updated without the result moving.",
                    );
                }
                f.write_str(
                    " This is a change in what the data means, not a \
                     reformatting, so check what changed at the source \
                     before updating the pin.",
                )
            }
            Self::Unreadable { locator, kind, why } => write!(
                f,
                "`{locator}` was fetched but cannot be read as {}: {why}. A \
                 200 is not a promise about the body -- an archive asked for \
                 an accession it does not have will answer with an error \
                 page",
                kind.name()
            ),
            Self::Cache(error) => error.fmt(f),
            Self::Store(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for RemoteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Locator(error) => Some(error),
            Self::Fetch { source, .. } => Some(source),
            Self::Cache(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::Moved { .. } | Self::Unreadable { .. } => None,
        }
    }
}

impl From<CacheError> for RemoteError {
    fn from(error: CacheError) -> Self {
        Self::Cache(error)
    }
}

impl From<StoreError> for RemoteError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

impl From<LocatorError> for RemoteError {
    fn from(error: LocatorError) -> Self {
        Self::Locator(error)
    }
}

/// Resolve a pinned remote to files on this machine.
///
/// A record whose objects are all still held and whose digest satisfies
/// `declared` is the answer, and no network is touched. Otherwise the source
/// is asked, what arrives is read as `kind`, and the digest is checked
/// before anything is stored or recorded — so a fetch that was not what the
/// pin promised leaves the cache exactly as it found it.
///
/// # Errors
///
/// [`RemoteError`] for a locator that cannot be read, a source that cannot
/// be reached, a body that cannot be read as `kind`, or a digest that does
/// not match `declared`.
pub fn resolve(
    locator: &Locator,
    kind: DataKind,
    declared: Option<ContentHash>,
    cache: &Cache,
    transport: &dyn Transport,
) -> Result<Resolved, RemoteError> {
    let written = locator.to_string();

    if let Some(resolved) = from_store(&written, kind, declared, cache)? {
        return Ok(resolved);
    }

    let previous = cache.remote(&written, kind)?;
    let fetched = fetch_all(locator, &written, kind, cache, transport)?;

    // Checked before anything is stored, so a fetch that is not what was
    // promised leaves nothing behind for a later run to find and trust.
    if let Some(expected) = declared
        && fetched.digest != expected
    {
        cache.forget_remote(&written, kind)?;
        return Err(RemoteError::Moved {
            locator: written,
            expected,
            found: fetched.digest,
            cosmetic: false,
        });
    }

    let mut notes = Vec::new();
    if let Some(previous) = previous
        && previous.digest == fetched.digest
        && previous.bytes != fetched.bytes
    {
        // The source rewrote the file and the biology is unchanged. Worth
        // saying, because a user who is told nothing will not know that the
        // archive has started serving different bytes; not worth failing
        // on, because nothing downstream moves.
        notes.push(Note::RemoteRewritten {
            locator: written.clone(),
        });
    }

    let row = RemoteRow {
        digest: fetched.digest,
        bytes: fetched.bytes,
        files: fetched.files.clone(),
        fetched_at: crate::prov::now(),
    };
    for (staged, file) in fetched.staged.iter().zip(&fetched.files) {
        cache.objects().put_file_as(staged, &file.key)?;
    }
    cache.record_remote(&written, kind, &row)?;

    Ok(Resolved {
        paths: paths_of(cache, &row)?,
        digest: row.digest,
        record: record_of(&written, kind, &row),
        from: Source::Network,
        notes,
    })
}

/// The answer from what is already held, if there is one.
fn from_store(
    written: &str,
    kind: DataKind,
    declared: Option<ContentHash>,
    cache: &Cache,
) -> Result<Option<Resolved>, RemoteError> {
    let Some(row) = cache.remote(written, kind)? else {
        return Ok(None);
    };
    // A record that does not satisfy the declaration is not the answer: the
    // user may have bumped the pin because the source legitimately changed,
    // and in that case the source is what to ask.
    if declared.is_some_and(|expected| expected != row.digest) {
        return Ok(None);
    }
    if row.files.is_empty() {
        return Ok(None);
    }
    let mut paths = Vec::with_capacity(row.files.len());
    for file in &row.files {
        // `local_path` rather than `checked_get`: the objects can be
        // gigabytes, and the cache's own prefilter is what decides when to
        // read them again. An object that has gone missing sends this back
        // to the source, which is the same answer a cold cache gives.
        match cache.objects().local_path(&file.key)? {
            Some(path) => paths.push(path),
            None => return Ok(None),
        }
    }
    Ok(Some(Resolved {
        paths,
        digest: row.digest,
        record: record_of(written, kind, &row),
        from: Source::Store,
        notes: Vec::new(),
    }))
}

/// What a fetch produced, before any of it is kept.
struct Fetched {
    staged: Vec<PathBuf>,
    files: Vec<RemoteFile>,
    digest: ContentHash,
    bytes: ContentHash,
}

/// Fetch every file a locator names, read each as `kind`, and fold the
/// digests in order.
fn fetch_all(
    locator: &Locator,
    written: &str,
    kind: DataKind,
    cache: &Cache,
    transport: &dyn Transport,
) -> Result<Fetched, RemoteError> {
    let urls = locator.urls()?;
    let suffixes = locator.suffixes();

    let mut staged = Vec::with_capacity(urls.len());
    let mut files = Vec::with_capacity(urls.len());
    let mut meanings = Vec::with_capacity(urls.len());
    let mut byte_hashes = Vec::with_capacity(urls.len());

    for (at, url) in urls.iter().enumerate() {
        let suffix = suffixes.get(at).cloned().unwrap_or_default();
        // Staged inside the cache directory, so moving it into the store is
        // a rename and a crash leaves its debris somewhere deletable.
        let into = cache.scratch(&format!(
            "fetch-{}-{at}{suffix}",
            ContentHash::of_bytes(written.as_bytes()).to_hex()
        ));
        let fetched = transport
            .get(url, &into)
            .map_err(|source| RemoteError::Fetch {
                locator: written.to_owned(),
                source,
            })?;

        let bytes = ContentHash::of_file(&into).map_err(|source| {
            RemoteError::Cache(CacheError::Io {
                path: into.clone(),
                source,
            })
        })?;
        byte_hashes.push(bytes);

        // What the file means, which is what the pin is over. Reading it is
        // also the only thing between an archive's error page and a
        // pipeline that treats it as data: see `Unreadable`.
        let canonical = cache
            .semantic(kind, &into)
            .map_err(|error| RemoteError::Unreadable {
                locator: written.to_owned(),
                kind,
                why: error.to_string(),
            })?;
        if canonical.form == Form::Raw {
            return Err(RemoteError::Unreadable {
                locator: written.to_owned(),
                kind,
                why: "it does not parse as that kind, so its bytes would \
                      have had to stand in for what it means"
                    .to_owned(),
            });
        }
        meanings.push(canonical.hash);

        files.push(RemoteFile {
            key: ObjectKey::new(bytes, &suffix),
            url: fetched.url,
            bytes: fetched.bytes,
        });
        staged.push(into);
    }

    Ok(Fetched {
        digest: ContentHash::of_hashes(&meanings),
        bytes: ContentHash::of_hashes(&byte_hashes),
        staged,
        files,
    })
}

/// Where the objects a record names sit on this filesystem.
fn paths_of(cache: &Cache, row: &RemoteRow) -> Result<Vec<PathBuf>, RemoteError> {
    row.files
        .iter()
        .map(|file| {
            cache
                .objects()
                .local_path(&file.key)
                .map_err(RemoteError::Store)
                .map(|found| found.unwrap_or_else(|| cache.objects().path_of(&file.key)))
        })
        .collect()
}

fn record_of(written: &str, kind: DataKind, row: &RemoteRow) -> Record {
    Record {
        locator: written.to_owned(),
        kind,
        digest: row.digest,
        bytes: row.bytes,
        files: row.files.clone(),
        fetched_at: row.fetched_at,
    }
}
