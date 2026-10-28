//! Advisory leases: which worker is computing which step key.
//!
//! # A lease is not a lock, and that is the design
//!
//! The store affords write-once blobs, no listing and no compare-and-swap.
//! A lock built on that is a lock that fails silently under exactly the
//! conditions it exists for, so nothing here pretends to be one.
//!
//! It does not need to be. Two workers that agree on a step key agreed on
//! the code, the parameters, the inputs' canonical hashes and the tool
//! versions — there is no sense in which they are doing different work — so
//! a race between them wastes CPU and cannot produce a wrong answer (D52).
//! The lease divides work. Correctness is the key's job.
//!
//! That is what makes the rest of this module simple. **Expiry does not
//! have to be right.** A worker that dies holding a lease needs no recovery
//! protocol, no fencing and no coordinator to notice: its deadline passes
//! and somebody else computes the step. Stealing is safe, so the mechanism
//! that decides when to steal is allowed to be approximate.
//!
//! # Generations, and the three things they buy
//!
//! A lease row cannot be deleted, rewritten or refreshed. One row per step
//! key would therefore be a lease that can be taken exactly once in the
//! lifetime of the store — so a stealing worker could not claim it, a
//! worker giving up could not release it, and a long-running holder could
//! not extend it.
//!
//! A generation counter gives all three on one mechanism. Rows live at
//! `<step key>/<generation>`, contiguous from zero, and the **highest
//! occupied** generation is the current state of the key:
//!
//! - **taking** a free key publishes the next generation;
//! - **giving it back** publishes the next generation with a deadline
//!   already in the past, because a release is just a lease nobody holds —
//!   which is why a worker whose step failed needs no second object class
//!   to say so;
//! - **extending** would publish the next generation with a deadline
//!   further out. That is not built (v0.3 has one `--lease` and says what
//!   getting it wrong costs), and the format carries it, so adding it later
//!   is not a format change.
//!
//! Exhausting [`MAX_GENERATIONS`] means a key has been claimed and given up
//! fifteen times. It is then free to everyone, which degrades to the
//! lock-free correctness above rather than to a stall.

use crate::key::StepKey;
use crate::worker::Worker;

use super::published::{self, Publication};
use super::{BlobStore, StoreError};

/// Advisory leases: which worker is computing which step key.
pub const LEASES: &str = "leases";

/// How many times one key may be claimed and given back.
///
/// Small on purpose. A key that has been through this many holders is not
/// going to be fixed by a sixteenth, and the probe that reads the
/// generations costs a store read each — so the bound is what keeps a
/// pathological key from turning every worker into a load generator.
pub const MAX_GENERATIONS: u32 = 16;

/// What probing a step key's leases found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lease {
    /// Nobody holds this key: either it has never been claimed, or the
    /// holder's deadline has passed. `generation` is the one to claim.
    Free {
        /// The generation a worker taking this key should publish at.
        generation: u32,
    },
    /// A worker holds this key until `deadline`.
    Held {
        /// What that worker calls itself.
        worker: String,
        /// When its claim runs out, in seconds since the Unix epoch.
        deadline: i64,
        /// The generation it holds.
        generation: u32,
    },
}

/// What trying to take a lease came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Claimed {
    /// This worker holds the key, at this generation.
    Acquired {
        /// The generation it holds, which is what it gives back.
        generation: u32,
    },
    /// Another worker got there first.
    Held {
        /// What that worker calls itself.
        worker: String,
        /// When its claim runs out.
        deadline: i64,
    },
}

/// Where one generation of one key's lease lives.
fn row_key(key: StepKey, generation: u32) -> String {
    format!("{}/{generation}", key.to_hex())
}

/// What the store says about this key, as of `now`.
///
/// # Errors
///
/// If the store cannot be read.
pub fn probe(store: &dyn BlobStore, key: StepKey, now: i64) -> Result<Lease, StoreError> {
    let mut highest: Option<(u32, String, i64)> = None;

    for generation in 0..MAX_GENERATIONS {
        match published::fetch(store, LEASES, &row_key(key, generation))? {
            Some(fields) => {
                if let Some((worker, deadline)) = read(&fields) {
                    highest = Some((generation, worker, deadline));
                }
                // A row this build cannot read still *occupies* its
                // generation, so probing has to keep going: stopping here
                // would read a newer format as an empty key and let every
                // worker claim it at once.
            }
            // Generations are contiguous, so the first gap is the end.
            None => {
                return Ok(match highest {
                    Some((generation, worker, deadline)) if deadline > now => Lease::Held {
                        worker,
                        deadline,
                        generation,
                    },
                    Some((generation, ..)) => Lease::Free {
                        generation: generation + 1,
                    },
                    None => Lease::Free { generation },
                });
            }
        }
    }

    // Every generation is taken. Free to anyone: see the module note.
    Ok(Lease::Free {
        generation: MAX_GENERATIONS,
    })
}

/// Take this key for `worker` until `deadline`, if it is free.
///
/// # Errors
///
/// If the store cannot be read or written.
pub fn acquire(
    store: &dyn BlobStore,
    key: StepKey,
    worker: &Worker,
    deadline: i64,
    now: i64,
) -> Result<Claimed, StoreError> {
    let generation = match probe(store, key, now)? {
        Lease::Held {
            worker: holder,
            deadline,
            ..
        } => {
            return Ok(Claimed::Held {
                worker: holder,
                deadline,
            });
        }
        Lease::Free { generation } => generation,
    };

    // Beyond the bound there is nothing to publish into, and the key is
    // free to everyone rather than claimable by anyone.
    if generation >= MAX_GENERATIONS {
        return Ok(Claimed::Acquired { generation });
    }

    // Publish, then believe the store rather than the write. Two workers
    // that both probed and both saw nothing both get here; write-once means
    // one row wins, and the read-back is what tells the loser it lost.
    // `instance` and not `name` is what is compared, so two processes a
    // user has given one name to still cannot both believe they hold it.
    let fields = vec![
        worker.name().to_owned(),
        worker.instance().to_owned(),
        now.to_string(),
        deadline.to_string(),
    ];
    match published::publish(store, LEASES, &row_key(key, generation), &fields)? {
        Publication::Ours => Ok(Claimed::Acquired { generation }),
        Publication::Theirs(theirs) => match read_full(&theirs) {
            Some((holder, instance, deadline)) if instance != worker.instance() => {
                Ok(Claimed::Held {
                    worker: holder,
                    deadline,
                })
            }
            // Ours after all, by a different spelling of the same claim.
            Some(..) => Ok(Claimed::Acquired { generation }),
            // Somebody wrote a lease row this build cannot read. Take the
            // key anyway.
            //
            // Waiting looks like the careful answer and is the wrong one:
            // there is no deadline to read, so a worker that waited would
            // have nothing to wait *for* and would never stop. Computing
            // is safe — that is the whole of D52 — so an unreadable lease
            // costs duplicated work, which is the one currency this
            // mechanism is allowed to spend.
            None => Ok(Claimed::Acquired { generation }),
        },
    }
}

/// Give this key back now, rather than holding it to its deadline.
///
/// For a worker whose step failed: another worker retries it at once
/// instead of idling until the deadline, which for a node-local failure —
/// a full scratch disk, a missing module — is what the user wants.
///
/// # Errors
///
/// If the store cannot be written.
pub fn relinquish(
    store: &dyn BlobStore,
    key: StepKey,
    generation: u32,
    now: i64,
) -> Result<(), StoreError> {
    let next = generation.saturating_add(1);
    if next >= MAX_GENERATIONS {
        // Nothing to write into, and nothing that needs writing: the key is
        // already free to everyone.
        return Ok(());
    }
    // A release is a lease that has already expired. `now` and not `now -
    // 1`: `probe` asks whether the deadline is *after* the moment it is
    // reading at, so a deadline equal to now is already past.
    let fields = vec![
        "(given back)".to_owned(),
        String::new(),
        now.to_string(),
        now.to_string(),
    ];
    published::put(store, LEASES, &row_key(key, next), &fields)?;
    Ok(())
}

/// A lease row's worker and deadline.
fn read(fields: &[String]) -> Option<(String, i64)> {
    read_full(fields).map(|(worker, _, deadline)| (worker, deadline))
}

/// A lease row's worker, instance and deadline.
fn read_full(fields: &[String]) -> Option<(String, String, i64)> {
    let [worker, instance, _started, deadline] = fields else {
        return None;
    };
    Some((worker.clone(), instance.clone(), deadline.parse().ok()?))
}
