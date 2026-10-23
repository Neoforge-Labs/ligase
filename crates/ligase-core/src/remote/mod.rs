//! Remote inputs: the fetch that starts every pipeline, brought inside the
//! engine's guarantees.
//!
//! Today a pipeline's first step usually sits outside everything Ligase
//! promises — a shell script downloads some reads, and the engine takes its
//! word for what arrived. This module is where that stops.
//!
//! # Two kinds of remote, and why they are not one
//!
//! A **pinned remote** — an SRA run, a PDB entry, a numbered ChEMBL release,
//! HXB2 by accession — promises the same bytes forever *from the source*. It
//! is pinned by digest, it is reproducible by anyone, anywhere, at any time,
//! and a change at the source is a loud failure: the remote moved.
//!
//! A **recorded query** — a live Open Targets query, a REST call to Monarch —
//! promises the same bytes forever *from the record*, because the source
//! promises nothing. It is reproducible by anyone holding the bundle.
//!
//! Conflating the two would be the usual mistake. There are two different
//! reproducibility claims here: *source* reproducibility, that a third party
//! will serve the same answer next Tuesday, cannot be promised for a live
//! query; *record* reproducibility, that anyone with the bundle gets the same
//! result, can be, and needs only that the response body itself is stored
//! rather than merely its digest (D39).
//!
//! # What is here
//!
//! [`locator`] reads where a remote comes from, out of a closed set of four
//! schemes. [`fetch`] gets the bytes, through a [`fetch::Transport`] so that
//! the tests can take the network away. [`pinned`] resolves a pinned
//! remote: a store hit whose digest satisfies the declaration needs no
//! network at all, and a digest that does not match is refused before
//! anything downstream has run.
//!
//! [`request`] says which record a query is — the canonical request, so that
//! paginated calls are keyed by what was asked rather than by the order it
//! was asked in — and [`response`] says what a recorded answer means, so
//! that two answers meaning the same thing are one answer. [`recorded`]
//! puts the two together: it replays a record, writes one, or refreshes one
//! and reports the drift.

pub mod fetch;
pub mod locator;
pub mod pinned;
pub mod recorded;
pub mod request;
pub mod response;

pub use locator::{Locator, LocatorError, SCHEMES, Scheme};
pub use request::{Auth, CanonicalRequest, Method, Request};
