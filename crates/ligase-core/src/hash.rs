//! Content hashing.
//!
//! Everything the cache believes rests on this: a file's identity is the
//! hash of its bytes, never its name, its path or its timestamp (D2). A
//! timestamp is only ever allowed to say that a file *need not be read
//! again* — the prefilter in [`crate::cache`] — which can save work but can
//! never decide an answer.
//!
//! BLAKE3 is the hash (D3). It is fast enough to run over a multi-gigabyte
//! BAM on every run, which is the property that makes content addressing
//! affordable here at all; the `mmap` and `rayon` features are what buy
//! that.

use std::fmt;
use std::io;
use std::path::Path;

/// The BLAKE3 hash of some bytes.
///
/// Compared, stored and printed as a hash; never as a path or a time.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Hash bytes already in memory.
    #[must_use]
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    /// Hash a file's contents.
    ///
    /// Memory-mapped and hashed in parallel where the file is large enough
    /// to be worth it, which BLAKE3 decides.
    ///
    /// # Errors
    ///
    /// If the file cannot be opened or read.
    pub fn of_file(path: &Path) -> io::Result<Self> {
        let mut hasher = blake3::Hasher::new();
        hasher.update_mmap_rayon(path)?;
        Ok(Self(*hasher.finalize().as_bytes()))
    }

    /// Fold several hashes into one, in the order given.
    ///
    /// Used where one input is more than one file — a pair of FASTQ mates —
    /// so that swapping the mates is a different hash.
    #[must_use]
    pub fn of_hashes(hashes: &[Self]) -> Self {
        let mut hasher = blake3::Hasher::new();
        for hash in hashes {
            hasher.update(hash.as_bytes());
        }
        Self(*hasher.finalize().as_bytes())
    }

    /// The raw digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The digest as 64 lowercase hex characters.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex(&self.0)
    }

    /// Read a hash back from the hex [`Self::to_hex`] writes.
    ///
    /// `None` for anything that is not exactly 64 hex characters, so a
    /// truncated or corrupted cache row is rejected rather than half-read.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        unhex(text).map(Self)
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({})", self.to_hex())
    }
}

/// 32 bytes as 64 lowercase hex characters.
pub(crate) fn hex(bytes: &[u8; 32]) -> String {
    let mut text = String::with_capacity(64);
    for byte in bytes {
        // Two hex digits per byte, so the string is fixed-width and sorts.
        text.push(char::from(nibble(byte >> 4)));
        text.push(char::from(nibble(byte & 0x0f)));
    }
    text
}

/// 64 hex characters back to 32 bytes.
pub(crate) fn unhex(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 {
        return None;
    }
    let mut bytes = [0u8; 32];
    let digits = text.as_bytes();
    for (index, byte) in bytes.iter_mut().enumerate() {
        let high = value(digits[index * 2])?;
        let low = value(digits[index * 2 + 1])?;
        *byte = (high << 4) | low;
    }
    Some(bytes)
}

const fn nibble(value: u8) -> u8 {
    match value {
        0..=9 => b'0' + value,
        _ => b'a' + (value - 10),
    }
}

const fn value(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn hex_round_trips_every_byte_value() {
        let mut bytes = [0u8; 32];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::try_from(index * 8 % 256).unwrap();
        }
        assert_eq!(unhex(&hex(&bytes)), Some(bytes));
    }

    #[test]
    fn hex_is_lowercase_and_fixed_width() {
        let text = hex(&[0xab; 32]);
        assert_eq!(text.len(), 64);
        assert_eq!(text, "ab".repeat(32));
    }

    #[test]
    fn uppercase_and_short_hex_are_refused() {
        assert_eq!(unhex(&"AB".repeat(32)), None);
        assert_eq!(unhex("abcd"), None);
        assert_eq!(unhex(&"zz".repeat(32)), None);
    }

    #[test]
    fn folding_hashes_depends_on_their_order() {
        let one = ContentHash::of_bytes(b"mate one");
        let two = ContentHash::of_bytes(b"mate two");
        assert_ne!(
            ContentHash::of_hashes(&[one, two]),
            ContentHash::of_hashes(&[two, one])
        );
    }
}
