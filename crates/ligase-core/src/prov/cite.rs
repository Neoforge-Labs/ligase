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
/// Two sets of tools are in here and nothing else is. The first is the
/// reference pipeline's (D7). The second is `ligase.steps`': a library step
/// is admitted only if its citation resolves *here* rather than through its
/// own `cite=` string (D42), because `cite=` is how a user's own step says
/// what it wants cited, and a step the project ships should not be taking
/// that liberty on the project's behalf.
///
/// Anything else is cited by the step that uses it, or reported as uncited.
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
    // `ligase.steps.bwa.mem`, and the one entry here whose reference is a
    // preprint. BWA's own README asks for the 2013 arXiv note when the
    // BWA-MEM algorithm was used, and that is the algorithm the library
    // step runs; the 2009 Bioinformatics paper describes `bwa aln`, which
    // the library does not ship. So the preprint is what is cited, with
    // the DOI arXiv itself issues for it.
    //
    // A reviewer may well want a journal reference instead and there is
    // not one to give. That is a fact about this tool rather than a gap in
    // this table, and substituting the paper for a different algorithm to
    // make the entry look tidier would be the kind of plausible wrongness
    // D24 exists to refuse.
    // The second entry whose software is not a program, after `hivdb`, and
    // the reason `exec`'s tool taxonomy grew a third kind (D46). RDKit asks
    // to be cited as its website rather than as a paper -- there is no
    // RDKit paper -- so that is what is given. A journal reference
    // substituted to make the row look like its neighbours would be the
    // plausible wrongness D24 refuses.
    //
    // The DOI is Zenodo's concept DOI, which resolves to the archive of
    // every release rather than to one of them. The release actually used
    // is in the methods paragraph beside this reference, from the version
    // the host registered, so the pair says which software and which
    // version without this row having to move every quarter.
    Citation {
        tool: "rdkit",
        text: "Landrum G et al. RDKit: Open-source cheminformatics. \
               https://www.rdkit.org",
        doi: "10.5281/zenodo.591637",
    },
    Citation {
        tool: "bwa",
        text: "Li H (2013) Aligning sequence reads, clone sequences and assembly \
               contigs with BWA-MEM. arXiv:1303.3997 [q-bio.GN].",
        doi: "10.48550/arXiv.1303.3997",
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
    fn the_aligner_a_library_step_runs_is_the_one_cited() {
        let citation = of("bwa").expect("bwa is in the table");
        assert!(
            citation.text.contains("BWA-MEM"),
            "`ligase.steps.bwa.mem` runs BWA-MEM, and that is the algorithm \
             bwa asks to be cited for: {}",
            citation.text
        );
        assert!(citation.render().contains("doi:10.48550/arXiv.1303.3997"));
    }

    #[test]
    fn a_library_is_cited_for_the_software_it_is_rather_than_a_paper() {
        // The second non-program in the table, after `hivdb`. There is no
        // RDKit paper, so what it asks to be cited as is what is given --
        // substituting a journal reference to make the row look like its
        // neighbours would be the plausible wrongness D24 refuses.
        let citation = of("rdkit").expect("rdkit is in the table");
        assert!(citation.text.contains("rdkit.org"), "{}", citation.text);
        assert!(citation.render().contains("doi:10.5281/zenodo.591637"));
    }

    #[test]
    fn every_library_the_engine_knows_can_be_credited() {
        // The invariant worth having, and the whole reason D46 exists: a
        // library is admitted as a `tool=` so that a reviewer gets the
        // reference. One in the taxonomy and not in this table would be
        // software a methods section used and did not name, which is the
        // asymmetry D24 refuses.
        for tool in crate::exec::LIBRARY {
            assert!(
                of(tool).is_some(),
                "`{tool}` is a library tool with no reference to give"
            );
        }
    }

    #[test]
    fn every_reference_carries_a_doi() {
        for citation in KNOWN {
            assert!(
                citation.doi.starts_with("10."),
                "{} has no resolvable DOI",
                citation.tool
            );
        }
    }

    #[test]
    fn an_unknown_tool_has_no_reference() {
        assert!(
            of("minimap3").is_none(),
            "the engine does not invent a reference from a name"
        );
    }
}
