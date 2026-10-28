//! A ranking of molecules: the rows **in order**.
//!
//! # This module does not sort, and that is deliberate
//!
//! Every other canonicaliser here sorts its records, for the reason D14
//! gives once: the order records came off whatever produced them is not a
//! fact about the biology. This one and
//! [`ranked_list`](super::ranked_list) are the two exceptions in the engine,
//! and the exception is the whole reason the kind exists.
//!
//! A ranking of docked compounds answers *which of these to order*. Rank is
//! the row position; there is nothing else for it to be. So the same ten
//! thousand compounds in two orders are two different answers, and a
//! canonical form that sorted them would declare the two identical — after
//! which a cache keyed on that form would serve one when the pipeline asked
//! for the other, in silence. For a virtual screen the order decides the
//! length of the prefix anybody actually buys, so the stakes are a purchase
//! order rather than a figure.
//!
//! **So do not change [`Digest::ordered_record`](super::Digest::ordered_record)
//! to `record` here to make this module consistent with its neighbours.** It
//! would compile, and every test but
//! `molecules.rs::the_order_of_a_molecule_ranking_is_its_result` would still
//! pass. That test hashes the same two files as a ranking and as a set and
//! asserts the *contrast*, rather than a bare inequality, so that a reader
//! can see which way round the rule goes (D41).
//!
//! Everything above the rows is identical to the set's, including the
//! representation being hashed as a field, and is shared with it.

use std::path::Path;

use super::{Digest, molecule_set};

/// Hash a molecule ranking's canonical form, rows in order.
pub(super) fn canonicalise(path: &Path, digest: &mut Digest) -> Result<(), String> {
    let (mut table, comments) = molecule_set::read(path)?;
    molecule_set::fields(digest, &comments, &table);

    while let Some(row) = table.next_row().map_err(|error| error.to_string())? {
        // `ordered_record`, not `record`. See the module comment.
        digest.ordered_record(&[row.line.as_bytes()]);
    }
    Ok(())
}
