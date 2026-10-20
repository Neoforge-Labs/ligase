//! Who to credit for a tool.
//!
//! A methods section names the software it used and cites the paper that
//! describes it, and a reviewer checks both. So a citation is not decoration
//! here: it is part of what [`super::methods`] exists to produce.
//!
//! # Why a table in the engine, and why a short one
//!
//! Two things a citation must never be: guessed, or quietly missing. A
//! reference invented from a tool's name would be a fabrication in a paper,
//! and a tool silently left out of a reference list would let an incomplete
//! methods section look complete. So this is a table of tools the engine
//! actually knows about — the ones the reference pipeline uses — plus
//! whatever a step declares with `cite=`; and a tool that neither covers is
//! *named as uncited* rather than omitted (D24).
//!
//! The table is deliberately small. Every entry in it is a promise to keep
//! a reference correct as versions move, and a long list of half-checked
//! citations would be worth less than a short list of checked ones plus an
//! honest gap.

/// A tool, and the reference a methods section should give for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Citation {
    /// The tool's name, as a step's `tool=` names it.
    pub tool: &'static str,
    /// The reference, formatted for a reference list.
    pub text: &'static str,
    /// The DOI, so a reader can resolve it without retyping the title.
    pub doi: &'static str,
}

impl Citation {
    /// The reference as a list prints it, DOI included.
    #[must_use]
    pub fn render(&self) -> String {
        format!("{} doi:{}", self.text, self.doi)
    }
}

/// Every tool the engine has a checked reference for.
///
/// These are the tools of the reference pipeline (D7). Anything else is
/// cited by the step that uses it, or reported as uncited.
pub const KNOWN: &[Citation] = &[
    Citation {
        tool: "minimap2",
        text: "Li H (2018) Minimap2: pairwise alignment for nucleotide sequences. \
               Bioinformatics 34(18):3094-3100.",
        doi: "10.1093/bioinformatics/bty191",
    },
    // One paper covers both: they are one project, released together, and
    // the 2021 paper is what each of them asks to be cited as.
    Citation {
        tool: "samtools",
        text: "Danecek P, Bonfield JK, Liddle J, Marshall J, Ohan V, Pollard MO, \
               Whitwham A, Keane T, McCarthy SA, Davies RM, Li H (2021) Twelve years \
               of SAMtools and BCFtools. GigaScience 10(2):giab008.",
        doi: "10.1093/gigascience/giab008",
    },
    // The resistance algorithm is not a program, but it is software that
    // produced a result and a reviewer will want the reference. Stanford
    // asks for this one by name on its own terms-of-use page.
    Citation {
        tool: "hivdb",
        text: "Rhee SY, Gonzales MJ, Kantor R, Betts BJ, Ravela J, Shafer RW (2003) \
               Human immunodeficiency virus reverse transcriptase and protease \
               sequence database. Nucleic Acids Research 31(1):298-303.",
        doi: "10.1093/nar/gkg100",
    },
    Citation {
        tool: "bcftools",
        text: "Danecek P, Bonfield JK, Liddle J, Marshall J, Ohan V, Pollard MO, \
               Whitwham A, Keane T, McCarthy SA, Davies RM, Li H (2021) Twelve years \
               of SAMtools and BCFtools. GigaScience 10(2):giab008.",
        doi: "10.1093/gigascience/giab008",
    },
];

/// The reference for a tool, if the engine knows one.
#[must_use]
pub fn of(tool: &str) -> Option<&'static Citation> {
    KNOWN.iter().find(|citation| citation.tool == tool)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    #[test]
    fn a_known_tool_has_a_reference_with_a_doi() {
        let citation = of("minimap2").expect("minimap2 is in the table");
        assert!(citation.text.contains("Li H"));
        assert!(
            citation
                .render()
                .contains("doi:10.1093/bioinformatics/bty191")
        );
    }

    #[test]
    fn samtools_and_bcftools_share_their_paper() {
        let samtools = of("samtools").expect("in the table");
        let bcftools = of("bcftools").expect("in the table");
        assert_eq!(samtools.doi, bcftools.doi);
    }

    #[test]
    fn an_unknown_tool_has_no_reference() {
        assert!(
            of("minimap3").is_none(),
            "the engine does not invent a reference from a name"
        );
    }
}
