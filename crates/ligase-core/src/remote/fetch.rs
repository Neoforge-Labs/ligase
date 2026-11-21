//! Getting the bytes.
//!
//! One blocking GET, streamed to a staged file. Blocking because the
//! executor is a rayon thread pool that deliberately never touches an async
//! runtime, and because the caller cannot do anything until the answer
//! arrives anyway.
//!
//! # What the client is for, and what it is not
//!
//! It is dull on purpose. Its whole job is restraint:
//!
//! - **Never retry something that will not change.** A 404 is retried zero
//!   times; a 503, a 429 or a 403 is retried with backoff and jitter.
//!   Retrying something final wastes time, and against a rate-limited
//!   archive it spends a budget the next real request needs.
//! - **Never retry forever.** Two bounds, not one: a number of attempts,
//!   and how long the waiting between them may add up to. The second exists
//!   because attempts alone cannot stop a doubling backoff from turning a
//!   refusal that never lets up into an hour of silence. It bounds the
//!   sleeping and not the asking — an attempt that hangs is bounded by
//!   [`Policy::timeout`] instead, which stays generous because a fetch that
//!   is still arriving is not a fetch to abandon.
//! - **Never follow a redirect forever**, and record *which* URL answered —
//!   the locator is what the user asked for, the final URL is what replied,
//!   and provenance wants both because they are different facts.
//! - **Never buffer the response.** An SRA run is gigabytes; it goes to disk
//!   as it arrives.
//! - **Never report success for bytes it did not finish reading**, and leave
//!   no staged file behind when it fails, so a half-fetch cannot look like a
//!   whole file to whatever runs next.
//!
//! # A secret has nowhere to go
//!
//! A [`Request`] carries its [`crate::remote::request::Auth`], because the
//! token has to be sent. Nothing else does: [`Fetched`] has no field for
//! request headers, and neither does the record written from it or the
//! bundle written from that. An `Authorization` header is therefore not
//! *stripped* before storage — it is never representable in anything the
//! engine stores, which is the only version of that guarantee a later field
//! cannot quietly undo (D39: a bundle never carries a secret). The one door
//! left is a message that formats a request, which `Auth`'s own `Debug`
//! closes by redacting the token.
//!
//! # Why a trait
//!
//! [`Transport`] exists so the tests can take the network away, and so M14's
//! store and M10b's replay can serve a request without one. The default and
//! only networked implementation is [`Network`].

use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::remote::request::{Method, Request};

/// How hard to try, and how much to accept.
#[derive(Clone, Copy, Debug)]
pub struct Policy {
    /// How many times to ask, counting the first. Only a failure that could
    /// plausibly differ next time is retried.
    pub attempts: u32,
    /// How long to wait after a failed attempt, doubling each time and
    /// jittered so that a fleet of runners refused together does not come
    /// back together.
    pub backoff: Duration,
    /// How long all the waiting between attempts may add up to.
    ///
    /// The second bound, and the one that does the work: attempts alone
    /// cannot stop a doubling backoff from turning a dead URL into an hour
    /// of silence. Whichever bound is reached first ends the fetch.
    pub max_total_wait: Duration,
    /// How long the whole operation may take, redirects and body included.
    pub timeout: Duration,
    /// How many redirects to follow before giving up.
    pub max_redirects: u32,
    /// The largest response to accept, in bytes.
    ///
    /// Generous by default: a pinned SRA run is legitimately gigabytes. The
    /// 32 MiB cap on a *recorded* body is a different limit for a different
    /// reason, and lives with the recording.
    pub max_bytes: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            // Six and a second: ENA refuses an automated client for tens of
            // seconds at a time, so three attempts half a second apart
            // reliably spent themselves inside one refusal and reported it
            // as though the object were gone.
            attempts: 6,
            backoff: Duration::from_secs(1),
            max_total_wait: Duration::from_secs(120),
            timeout: Duration::from_secs(600),
            max_redirects: 5,
            max_bytes: 64 * 1024 * 1024 * 1024,
        }
    }
}

/// What arrived.
///
/// Note what is *not* here: no request headers, no response headers, nothing
/// a secret could be recorded in. See the module documentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetched {
    /// The URL that answered, after any redirects.
    pub url: String,
    /// The status it answered with.
    pub status: u16,
    /// How many bytes were written to the staged path.
    pub bytes: u64,
}

/// Why a fetch did not produce bytes.
#[derive(Debug)]
pub enum FetchError {
    /// The server could not be reached, or did not finish answering.
    Unreachable {
        /// What was asked for.
        url: String,
        /// What went wrong, as the transport put it.
        why: String,
    },
    /// The server answered, and the answer was not a success.
    Status {
        /// What was asked for.
        url: String,
        /// The status it answered with.
        status: u16,
    },
    /// The response was larger than the policy allows.
    TooLarge {
        /// What was asked for.
        url: String,
        /// The limit it exceeded, in bytes.
        limit: u64,
    },
    /// The staged file could not be written.
    Io {
        /// Where it was being written.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// Every attempt was spent, and the last one still failed.
    ///
    /// Carries the failure that ended it, because *what* kept failing is
    /// the actionable half: an archive that rate-limits under a 403 is a
    /// different thing to go and look at than a connection that never
    /// opened. Only a failure that was actually retried arrives wrapped
    /// this way -- a 404 is reported as itself, so that "gave up after six
    /// attempts" is never said about one attempt.
    Exhausted {
        /// What was asked for.
        url: String,
        /// How many times it was asked.
        attempts: u32,
        /// The failure the last attempt ended with.
        last: Box<FetchError>,
    },
    /// The request asks for a method this transport cannot issue.
    ///
    /// A method is part of a record's identity, so a `POST` is hashable
    /// before it is sendable. Refused here rather than downgraded to a
    /// `GET`, which would ask a different question and then record the
    /// answer as though it were the one wanted.
    Unsendable {
        /// What was asked for.
        url: String,
        /// The method that cannot be issued.
        method: Method,
    },
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreachable { url, why } => {
                write!(f, "could not fetch {url}: {why}")
            }
            Self::Status { url, status } => {
                write!(f, "{url} answered {status}")
            }
            Self::TooLarge { url, limit } => write!(
                f,
                "{url} is larger than the {limit} bytes this fetch allows"
            ),
            Self::Exhausted {
                url,
                attempts,
                last,
            } => {
                let times = if *attempts == 1 {
                    "attempt"
                } else {
                    "attempts"
                };
                write!(f, "gave up on {url} after {attempts} {times}; the last ")?;
                match last.status() {
                    Some(status) => write!(f, "answered {status}"),
                    None => write!(f, "failed: {last}"),
                }
            }
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Unsendable { url, method } => write!(
                f,
                "{url} would have to be asked with {method}, which this \
                 engine cannot send yet: a request body is part of what \
                 identifies a record, so the method is understood before it \
                 can be issued"
            ),
        }
    }
}

impl std::error::Error for FetchError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Exhausted { last, .. } => Some(last.as_ref()),
            _ => None,
        }
    }
}

impl FetchError {
    /// Whether asking again could plausibly give a different answer.
    ///
    /// A connection that dropped and a server that is overloaded might; a
    /// 404 will not, and neither will a response that is too big.
    ///
    /// A 403 is in the retryable set, which looks wrong until you watch
    /// ENA: it answers a client it has had enough of for the moment with a
    /// 403 rather than a 429, on a different file each time. A file that is
    /// really not yours stays a 403 through every attempt and is reported
    /// with its status, so the cost of being wrong about one is the wait --
    /// whereas the cost of treating a rate limit as final was a demo that
    /// could not fetch its own pinned inputs.
    #[must_use]
    pub const fn worth_retrying(&self) -> bool {
        match self {
            Self::Unreachable { .. } => true,
            Self::Status { status, .. } => matches!(*status, 403 | 408 | 429) || *status >= 500,
            // Already spent; the bounds are the bounds.
            Self::Exhausted { .. }
            | Self::TooLarge { .. }
            | Self::Io { .. }
            | Self::Unsendable { .. } => false,
        }
    }

    /// The status a server answered with, where one did.
    ///
    /// Sees through an [`Self::Exhausted`] to the failure underneath, so a
    /// caller that wants to branch on the status does not have to know
    /// whether the fetch was retried on its way to failing.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Self::Status { status, .. } => Some(*status),
            Self::Exhausted { last, .. } => last.status(),
            Self::Unreachable { .. }
            | Self::TooLarge { .. }
            | Self::Io { .. }
            | Self::Unsendable { .. } => None,
        }
    }
}

/// Somewhere bytes come from.
///
/// Implemented by [`Network`] for the real thing, and by the tests for
/// everything else. The staged path is the caller's: the transport writes it
/// and removes it on failure, and never decides where it goes.
pub trait Transport: Send + Sync {
    /// Make this request, writing the body into `into` and replacing
    /// whatever was there.
    ///
    /// Takes the whole [`Request`] rather than a URL because a recorded
    /// query may be authenticated, and one method taking one type is what
    /// stops a second one existing whose default implementation quietly
    /// drops the authentication.
    ///
    /// # Errors
    ///
    /// [`FetchError`] if the server cannot be reached, answers with a
    /// failure, answers with too much, is asked for a method this transport
    /// cannot issue, or the staged file cannot be written. On any error the
    /// staged file is removed.
    fn fetch(&self, request: &Request, into: &Path) -> Result<Fetched, FetchError>;
}

/// The networked transport.
#[derive(Debug)]
pub struct Network {
    agent: ureq::Agent,
    policy: Policy,
}

impl Network {
    /// A client that behaves as `policy` says.
    #[must_use]
    pub fn new(policy: Policy) -> Self {
        let config = ureq::Agent::config_builder()
            .max_redirects(policy.max_redirects)
            // Exhausting the redirect budget is an error rather than the
            // last 3xx handed back as if it were an answer.
            .max_redirects_will_error(true)
            // Statuses are ours to interpret: which are worth retrying is a
            // decision this module makes, not one the client makes for it.
            .http_status_as_error(false)
            .timeout_global(Some(policy.timeout))
            .build();
        Self {
            agent: config.into(),
            policy,
        }
    }

    /// One attempt, with no retrying.
    fn attempt(&self, request: &Request, into: &Path) -> Result<Fetched, FetchError> {
        use ureq::ResponseExt as _;
        use ureq::http::Uri;

        let url = request.url.as_str();
        let mut call = self.agent.get(url);
        // The one place a token leaves `Auth`, handed straight to the
        // client: nothing in between holds it, and nothing downstream has a
        // field it could be written to.
        if let Some(auth) = &request.auth {
            let (name, value) = auth.header();
            call = call.header(name, value);
        }
        let response = call.call().map_err(|error| FetchError::Unreachable {
            url: url.to_owned(),
            why: error.to_string(),
        })?;

        let status = response.status().as_u16();
        let answered: &Uri = response.get_uri();
        let answered = answered.to_string();

        if !(200..300).contains(&status) {
            return Err(FetchError::Status {
                url: url.to_owned(),
                status,
            });
        }

        // One byte over the limit is enough to know, so nothing reads
        // further than that: an enormous body is refused without being read
        // to its end.
        let limit = self.policy.max_bytes;
        let cap = limit.saturating_add(1);
        let mut reader = response.into_body().into_with_config().limit(cap).reader();
        let written = stream(&mut reader, into, cap).map_err(|source| FetchError::Io {
            path: into.to_path_buf(),
            source,
        })?;

        if written > limit {
            return Err(FetchError::TooLarge {
                url: url.to_owned(),
                limit,
            });
        }

        Ok(Fetched {
            url: answered,
            status,
            bytes: written,
        })
    }
}

impl Transport for Network {
    fn fetch(&self, request: &Request, into: &Path) -> Result<Fetched, FetchError> {
        let url = request.url.as_str();
        if request.method != Method::Get {
            // Refused rather than silently downgraded to a GET, which would
            // ask a different question and record the answer as though it
            // were the one wanted.
            return Err(FetchError::Unsendable {
                url: url.to_owned(),
                method: request.method,
            });
        }
        let attempts = self.policy.attempts.max(1);
        let mut waited = self.policy.backoff;
        let mut spent = Duration::ZERO;
        for attempt in 1..=attempts {
            match self.attempt(request, into) {
                Ok(fetched) => return Ok(fetched),
                Err(error) => {
                    // Whatever happened, nothing half-written is left where
                    // something whole is expected.
                    let _ = fs::remove_file(into);
                    if !error.worth_retrying() {
                        // Reported as itself: a 404 wrapped in "gave up
                        // after six attempts" would send someone looking
                        // for a flake that is not there.
                        return Err(error);
                    }
                    let remaining = self.policy.max_total_wait.saturating_sub(spent);
                    if attempt == attempts || remaining.is_zero() {
                        // Wrapped only once a retry has actually been
                        // spent. A policy of one attempt did not give up on
                        // anything, and saying so would make a single
                        // refused request read like a flake that was
                        // chased.
                        return Err(if attempt > 1 {
                            FetchError::Exhausted {
                                url: url.to_owned(),
                                attempts: attempt,
                                last: Box::new(error),
                            }
                        } else {
                            error
                        });
                    }
                    let pause = jittered(waited).min(remaining);
                    std::thread::sleep(pause);
                    spent = spent.saturating_add(pause);
                    waited = waited.saturating_mul(2);
                }
            }
        }
        // `attempts.max(1)` guarantees at least one pass, each of which
        // either returns or sleeps, so this is unreachable by construction;
        // saying so costs less than a panic would.
        Err(FetchError::Unreachable {
            url: url.to_owned(),
            why: "no attempt was made".to_owned(),
        })
    }
}

/// A transport that fetches nothing.
///
/// For a run that must not reach the network: a reviewer with no connection,
/// a test that wants to prove its inputs came out of the store, or a machine
/// on a plane. Resolution still succeeds for anything the store already
/// holds with a digest that satisfies the declaration — which is the whole
/// point, and is why this is a transport rather than a flag threaded through
/// the resolver.
#[derive(Clone, Copy, Debug, Default)]
pub struct Offline;

impl Transport for Offline {
    fn fetch(&self, request: &Request, _into: &Path) -> Result<Fetched, FetchError> {
        Err(FetchError::Unreachable {
            url: request.url.clone(),
            why: "this run may not fetch anything, and the store does not \
                  hold it. Run it once with the network available, or point \
                  it at a store that has been"
                .to_owned(),
        })
    }
}

/// A wait with up to as much again added.
///
/// Jitter rather than a bare doubling because the clients that get refused
/// together are the ones that asked together: a CI matrix refused by ENA in
/// the same second and backing off in lockstep arrives back in the same
/// second and is refused again. Spreading the returns over a band is what
/// turns a synchronised retry into independent ones.
///
/// The entropy is `RandomState`, seeded per instance by the standard
/// library from the operating system. Enough to decorrelate two processes
/// on two runners, and no dependency to take on for a number whose only job
/// is to differ from the next one.
fn jittered(base: Duration) -> Duration {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher as _, Hasher as _};

    let span = base.as_nanos();
    if span == 0 {
        return base;
    }
    let noise = u128::from(RandomState::new().build_hasher().finish());
    let extra = u64::try_from(noise % span).unwrap_or(u64::MAX);
    base.saturating_add(Duration::from_nanos(extra))
}

/// Copy at most `cap` bytes from a reader to a path, reporting how many
/// landed.
///
/// Not `io::copy`, for three reasons: the count is wanted even when the copy
/// fails part-way, the file is flushed before the count is believed, and no
/// read asks for more than the cap allows — so the caller can learn that a
/// body is one byte too big without the reader being pushed past the point
/// where it would rather raise an error than answer.
fn stream(from: &mut impl Read, into: &Path, cap: u64) -> io::Result<u64> {
    if let Some(parent) = into.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(into)?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut written = 0u64;
    while written < cap {
        let room = usize::try_from(cap - written).unwrap_or(usize::MAX);
        let want = room.min(buffer.len());
        let read = from.read(&mut buffer[..want])?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        written += read as u64;
    }
    file.flush()?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wait_stays_in_its_band_and_does_not_repeat_itself() {
        let base = Duration::from_millis(80);
        let waits: std::collections::HashSet<Duration> = (0..16).map(|_| jittered(base)).collect();

        assert!(waits.len() > 1, "every wait came out identical: {waits:?}");
        assert!(
            waits.iter().all(|wait| *wait >= base && *wait < base * 2),
            "jitter left the band it was supposed to stay in: {waits:?}"
        );
    }

    #[test]
    fn a_zero_wait_is_left_alone_rather_than_divided_by() {
        assert_eq!(jittered(Duration::ZERO), Duration::ZERO);
    }
}
