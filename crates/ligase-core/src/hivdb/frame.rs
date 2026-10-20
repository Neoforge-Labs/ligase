//! Which coordinate frame the gene windows are stated in.
//!
//! [`super::codon::GENES`] names protease, reverse transcriptase, integrase
//! and capsid as nucleotide ranges — `PR` is 2253..2549 — and those numbers
//! mean nothing on their own. They are positions in HXB2 K03455, counted
//! from the first base of the genome, and a reference numbered any other way
//! puts a different gene under each window.
//!
//! Nothing used to check that. A pol-relative reference, the obvious thing
//! to have when the amplicon is pol, is 2084 bases shorter at the front:
//! K03455 2085..5096. Handed one, the engine read what is really integrase
//! as protease and reported protease-inhibitor resistance at a clinical
//! level, with no error and no note. That is the bug this module exists to
//! turn into a refusal, and it is exactly the class of bug Ligase exists to
//! prevent — a silent coordinate mixup — found in Ligase itself.
//!
//! # Why the frame is declared and not inferred
//!
//! A reference whose single contig is 9,719 bases *is* whole-genome K03455,
//! or something deliberately numbered along it, and needs no declaration:
//! length is the one fact the windows actually depend on, and checksum
//! equality would refuse an HXB2-coordinate consensus of another subtype,
//! which is a legitimate thing to interpret against.
//!
//! Anything else cannot be settled by reading the file. A 3,012-base slice
//! of K03455 and a 3,012-base genome of something else look identical from
//! the outside, so the engine asks rather than guesses
//! ([`crate::dim::Frame`]).

use crate::dim::{Frame, ReferenceId};

use super::codon::GENES;

/// The frame the gene windows are stated in: HXB2, by its accession.
pub const FRAME: &str = "K03455";

/// How many bases whole-genome K03455 has.
pub const BASES: u64 = 9719;

/// Where pol starts in K03455, 1-based: the front a pol-relative reference
/// is missing, and the padding that puts it back.
pub const POL: u64 = 2085;

/// The last base any gene window reaches.
pub const LAST: u64 = {
    let mut last = 0;
    let mut index = 0;
    while index < GENES.len() {
        if GENES[index].end as u64 > last {
            last = GENES[index].end as u64;
        }
        index += 1;
    }
    last
};

/// Why a reference is not one these gene windows can be read against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mismatch {
    /// Too short to hold the windows at all, whatever it is numbered along.
    TooShort {
        /// How long it is.
        bases: u64,
    },
    /// Long enough, but not the whole frame and not declared to be in it.
    Undeclared {
        /// How long it is, when the file records a length.
        bases: Option<u64>,
    },
    /// Declared as a slice of the right frame rather than the whole of it.
    Sliced {
        /// The frame as declared.
        frame: Frame,
    },
    /// Declared as being numbered along something else entirely.
    Elsewhere {
        /// The frame as declared.
        frame: Frame,
    },
    /// Several sequences, where a resistance reference is one genome.
    Several {
        /// How many there are.
        contigs: usize,
    },
}

/// How the frame this engine reads in describes itself, for a message.
#[must_use]
pub fn expected() -> String {
    format!("{FRAME} from base 1 ({BASES} bases)")
}

impl Mismatch {
    /// What is wrong, in one sentence that names both sides.
    #[must_use]
    pub fn detail(&self) -> String {
        match self {
            Self::TooShort { bases } => format!(
                "The reference is {bases} bases long and the gene windows reach base {LAST}, \
                 so they cannot all land on the genes they name; protease is read from \
                 wherever base {} of this file happens to be.",
                first_of("PR"),
            ),
            Self::Undeclared { bases: Some(bases) } => format!(
                "The reference is {bases} bases where whole-genome {FRAME} is {BASES}, so \
                 this engine cannot tell from the file whether its gene windows land on the \
                 genes they name."
            ),
            Self::Undeclared { bases: None } => format!(
                "The reference does not record a length, so this engine cannot tell whether \
                 it is numbered along {FRAME} from base 1."
            ),
            Self::Sliced { frame } => format!(
                "Base 1 of this reference is base {} of {FRAME}, so every position in it is \
                 {} lower than the position this engine means by the same number.",
                frame.first(),
                frame.offset(),
            ),
            Self::Elsewhere { frame } => format!(
                "This reference is numbered along {}, and these gene windows are positions \
                 in {FRAME}.",
                frame.describe(),
            ),
            Self::Several { contigs } => format!(
                "The reference holds {contigs} sequences, and a resistance reference is one \
                 viral genome numbered along {FRAME}."
            ),
        }
    }

    /// What to do about it.
    #[must_use]
    pub fn conclusion(&self) -> String {
        match self {
            Self::TooShort { .. } | Self::Sliced { .. } => format!(
                "This engine reads whole-genome {FRAME} and does not shift its windows. If \
                 this is a pol-relative reference — {FRAME} {POL}..5096 is the usual slice — \
                 pad it back into the frame with `\"N\" * {} + pol` and declare \
                 `frame=(\"{FRAME}\", 1)`.",
                POL - 1,
            ),
            Self::Undeclared { .. } => format!(
                "Which frame a reference is numbered along cannot be read from a FASTA. \
                 Declare it where you load the reference: `frame=(\"{FRAME}\", 1)` for a \
                 whole genome, or the base it starts at for a slice."
            ),
            Self::Elsewhere { .. } => format!(
                "Convert the reference to {FRAME} coordinates, or declare the frame it is \
                 really numbered along."
            ),
            Self::Several { .. } => {
                "Pass the single genome the calls were made against.".to_owned()
            }
        }
    }

    /// Both halves, for a caller with no error shape of its own to fill in.
    #[must_use]
    pub fn describe(&self) -> String {
        format!("{} {}", self.detail(), self.conclusion())
    }
}

/// Whether these gene windows can be read against this reference.
///
/// `None` when they can. See the module documentation for why a 9,719-base
/// reference is accepted without a declaration and nothing else is.
#[must_use]
pub fn mismatch(reference: &ReferenceId) -> Option<Mismatch> {
    if let Some(frame) = reference.frame() {
        if !is_k03455(frame.name()) {
            return Some(Mismatch::Elsewhere {
                frame: frame.clone(),
            });
        }
        return (!frame.is_whole()).then(|| Mismatch::Sliced {
            frame: frame.clone(),
        });
    }

    match reference.contigs() {
        [only] => match only.length {
            Some(BASES) => None,
            Some(bases) if bases < LAST => Some(Mismatch::TooShort { bases }),
            bases => Some(Mismatch::Undeclared { bases }),
        },
        contigs => Some(Mismatch::Several {
            contigs: contigs.len(),
        }),
    }
}

/// Whether a declared frame name is this one.
///
/// Accession versions and the strain's own name both count: a user who
/// declares `K03455.1` or `HXB2` has said the same thing as one who declares
/// `K03455`, and refusing them would be pedantry rather than safety. This is
/// a *declaration* being read, not identity being established — identity is
/// the reference dimension's job and rests on contigs, never on a name.
fn is_k03455(name: &str) -> bool {
    let bare = name.split('.').next().unwrap_or(name).trim();
    bare.eq_ignore_ascii_case(FRAME) || bare.eq_ignore_ascii_case("HXB2")
}

/// The first base of a gene's window, for a message. Zero if it has none.
fn first_of(name: &str) -> u32 {
    let mut index = 0;
    while index < GENES.len() {
        if GENES[index].name.as_bytes() == name.as_bytes() {
            return GENES[index].start;
        }
        index += 1;
    }
    0
}
