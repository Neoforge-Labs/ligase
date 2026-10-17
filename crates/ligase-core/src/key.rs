//! What a step's cache key is made of.
//!
//! `CLAUDE.md` fixes the recipe: hash(function code, parameters, input
//! content hashes, external tool versions). Everything that can change a
//! step's result is in there, and nothing else is — a key that moves when it
//! should not is a cache that never hits, and a key that stays when it
//! should not is a wrong answer served quickly.
//!
//! # Why the ingredients are length-prefixed
//!
//! Concatenating them would make a parameter `x` with value `yz`
//! indistinguishable from a parameter `xy` with value `z`. Each field is
//! written with its length first, so no two different ingredient lists can
//! produce the same byte stream.
//!
//! # Why a seed needs nothing special
//!
//! `CLAUDE.md` requires random seeds to be explicit, typed, hashed
//! parameters. A seed is an ordinary parameter of a step, so it is hashed
//! here like any other: a step that draws on a seed it did not declare is
//! not reproducible, and no cache can make it so.
//!
//! # What an input's hash is a hash of
//!
//! Inputs enter as content hashes, tagged with the policy that produced
//! them. That is [`INPUT_POLICY`]: since M4, the hash of a file's
//! *canonical form* rather than of its bytes (D4) — records sorted,
//! volatile headers stripped, as [`crate::canon`] defines per type. The
//! bytes were too strict a test, because every real tool records its own
//! command line in its output.
//!
//! The tag is in the key, so a key made under one policy can never be
//! mistaken for a key made under another; [`KEY_VERSION`] retires the ones
//! made under the old one.

use std::fmt;

use crate::hash::{ContentHash, hex, unhex};
use crate::types::DataKind;

/// The version of the key *format*.
///
/// Bumped whenever the ingredients or their layout change, which retires
/// every key written by an older version. An entry from another format is
/// not stale, it is a claim about ingredients that were combined
/// differently — so it must never be read, only ignored.
pub const KEY_VERSION: u32 = 2;

/// How an input's content hash was arrived at.
///
/// `canonical` is the hash of what a file *means*, by its type's rules
/// ([`crate::canon`]). It replaced `raw` — the hash of the bytes — in M4,
/// and the name is here in the key so that a key made under one policy can
/// never be mistaken for a key made under the other.
///
/// The rules' own version is not in this name. It is tracked beside the
/// cache's other versions, where a change to it clears the canonical hashes
/// as well as the keys built from them.
pub const INPUT_POLICY: &str = "canonical";

/// Separates this hash's domain from every other BLAKE3 hash in the engine.
const DOMAIN: &str = "ligase.step-key";

/// A step's cache key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StepKey([u8; 32]);

impl StepKey {
    /// The key as 64 lowercase hex characters.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex(&self.0)
    }

    /// Read a key back from the hex [`Self::to_hex`] writes.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        unhex(text).map(Self)
    }
}

impl fmt::Display for StepKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for StepKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StepKey({})", self.to_hex())
    }
}

/// Everything a step's result can depend on.
///
/// Inputs, parameters and tools are bound by name, so the order they were
/// collected in says nothing about the result and is not part of the key.
/// Which name a value is bound to very much is: the same two files swapped
/// between `aln` and `ref` is a different command.
#[derive(Clone, Debug)]
pub struct Ingredients {
    /// The step's name.
    pub step: String,
    /// The source of the function, as written.
    pub code: String,
    /// The command or callback the body produced.
    pub recipe: String,
    /// The kind of data the step declares it produces.
    pub output: DataKind,
    /// Plain parameters, as `(name, value)` rendered for hashing.
    pub parameters: Vec<(String, String)>,
    /// Data inputs, as `(parameter, content hash)`.
    pub inputs: Vec<(String, ContentHash)>,
    /// External tools, as `(name, version)`.
    pub tools: Vec<(String, String)>,
}

impl Ingredients {
    /// The key these ingredients produce.
    #[must_use]
    pub fn key(&self) -> StepKey {
        let mut hasher = blake3::Hasher::new();
        let mut write = |label: &str, value: &[u8]| {
            field(&mut hasher, label.as_bytes());
            field(&mut hasher, value);
        };

        write("domain", DOMAIN.as_bytes());
        write("key-version", &KEY_VERSION.to_le_bytes());
        write("input-policy", INPUT_POLICY.as_bytes());
        write("step", self.step.as_bytes());
        write("code", self.code.as_bytes());
        write("recipe", self.recipe.as_bytes());
        write("output", self.output.name().as_bytes());

        named(&mut hasher, "parameters", &self.parameters);
        named(
            &mut hasher,
            "inputs",
            &self
                .inputs
                .iter()
                .map(|(param, hash)| (param.clone(), hash.to_hex()))
                .collect::<Vec<_>>(),
        );
        named(&mut hasher, "tools", &self.tools);

        StepKey(*hasher.finalize().as_bytes())
    }
}

/// A group of name-value pairs, in a fixed order whatever order they arrived
/// in.
fn named(hasher: &mut blake3::Hasher, label: &str, pairs: &[(String, String)]) {
    let mut sorted: Vec<&(String, String)> = pairs.iter().collect();
    sorted.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));

    field(hasher, label.as_bytes());
    field(hasher, &(sorted.len() as u64).to_le_bytes());
    for (name, value) in sorted {
        field(hasher, name.as_bytes());
        field(hasher, value.as_bytes());
    }
}

/// One field, length first, so no two different lists share a byte stream.
fn field(hasher: &mut blake3::Hasher, bytes: &[u8]) {
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}
