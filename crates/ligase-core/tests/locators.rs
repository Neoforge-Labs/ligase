//! M10: where a remote input comes from, and the closed set of four ways to
//! say it.
//!
//! `sra:`, `ncbi:`, `ensembl:` and `https:`, and anything else is an error
//! naming the four. The set is closed on purpose: a scheme is a promise that
//! the engine knows how to turn an accession into bytes and how to recognise
//! the answer, and an open set would be a promise it could not keep.
//!
//! The ENA directory rule tested here used to live only in
//! `scripts/fetch_test_data.sh`, which M10 deletes. It is four branches on
//! the length of an accession, every one of them load-bearing, and it is
//! tested here because nothing else will be looking at it any more.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use ligase::remote::{Locator, LocatorError, Scheme};

// --------------------------------------------------------------- parsing --

#[test]
fn each_of_the_four_schemes_parses() {
    assert_eq!(
        Locator::parse("sra:SRR32217726").unwrap().scheme(),
        Scheme::Sra
    );
    assert_eq!(
        Locator::parse("ncbi:K03455.1").unwrap().scheme(),
        Scheme::Ncbi
    );
    assert_eq!(
        Locator::parse("ensembl:110/homo_sapiens/README")
            .unwrap()
            .scheme(),
        Scheme::Ensembl
    );
    assert_eq!(
        Locator::parse("https://example.org/x.fasta")
            .unwrap()
            .scheme(),
        Scheme::Https
    );
}

#[test]
fn a_locator_renders_as_what_was_written() {
    // The locator is what the user asked for, and it is what provenance and
    // the methods paragraph quote. A final URL after redirects is a
    // different fact about a different thing.
    let locator = Locator::parse("sra:SRR32217726").unwrap();
    assert_eq!(locator.to_string(), "sra:SRR32217726");
}

#[test]
fn a_fifth_scheme_is_refused_naming_the_four() {
    for attempt in ["ftp://ftp.ensembl.org/x", "s3://bucket/key", "gs://b/k"] {
        let error = Locator::parse(attempt).unwrap_err();
        let message = error.to_string();
        assert!(
            matches!(error, LocatorError::UnknownScheme { .. }),
            "{attempt} should be an unknown scheme, got {message}"
        );
        for scheme in ["sra:", "ncbi:", "ensembl:", "https:"] {
            assert!(
                message.contains(scheme),
                "the error must name {scheme} so the user can pick one: {message}"
            );
        }
    }
}

#[test]
fn a_bare_accession_with_no_scheme_is_refused_naming_the_four() {
    // The likeliest mistake, and the one where guessing would be worst:
    // `SRR32217726` looks like an SRA run and `K03455.1` looks like an NCBI
    // accession, so a guess would work often enough to be trusted and then
    // fetch the wrong thing from the wrong archive.
    let error = Locator::parse("SRR32217726").unwrap_err();
    assert!(matches!(error, LocatorError::NoScheme { .. }));
    assert!(error.to_string().contains("sra:"));
}

#[test]
fn an_empty_body_is_refused() {
    for attempt in ["sra:", "ncbi:", "ensembl:", "https://"] {
        let error = Locator::parse(attempt).unwrap_err();
        assert!(
            matches!(error, LocatorError::Empty { .. }),
            "{attempt} names no resource, got {error}"
        );
    }
}

#[test]
fn an_empty_locator_is_refused() {
    assert!(Locator::parse("").is_err());
    assert!(Locator::parse("   ").is_err());
}

#[test]
fn surrounding_space_is_forgiven() {
    assert_eq!(
        Locator::parse("  sra:SRR32217726  ").unwrap().to_string(),
        "sra:SRR32217726"
    );
}

#[test]
fn plain_http_is_not_one_of_the_four() {
    // Pinning by digest over a channel anyone can rewrite is a pin that
    // proves nothing about where the bytes came from, only that they hash
    // to what was expected -- and the expectation travelled over the same
    // channel in the pipeline file. `https:` is the scheme; `http:` is a
    // mistake worth naming.
    let error = Locator::parse("http://example.org/x.fasta").unwrap_err();
    assert!(error.to_string().contains("https:"));
}

// ------------------------------------------------------------------ URLs --

#[test]
fn an_sra_run_resolves_to_both_mates_in_mate_order() {
    let urls = Locator::parse("sra:SRR32217726").unwrap().urls().unwrap();
    assert_eq!(
        urls.len(),
        2,
        "a paired run is two files, and order matters"
    );
    assert!(urls[0].ends_with("SRR32217726_1.fastq.gz"), "{:?}", urls[0]);
    assert!(urls[1].ends_with("SRR32217726_2.fastq.gz"), "{:?}", urls[1]);
}

#[test]
fn the_ena_subdirectory_rule_holds_at_every_accession_length() {
    // ENA lays FASTQs out at vol1/fastq/<first 6>/<subdir>/<run>/, where the
    // subdir is the accession's trailing digits zero-padded to three and is
    // absent for 9-character accessions. Four branches, all four used by
    // real accessions, and the demo depends on two of them.
    let cases = [
        ("SRR123456", "/vol1/fastq/SRR123/SRR123456/"),
        ("SRR1234567", "/vol1/fastq/SRR123/007/SRR1234567/"),
        ("SRR12345678", "/vol1/fastq/SRR123/078/SRR12345678/"),
        ("SRR123456789", "/vol1/fastq/SRR123/789/SRR123456789/"),
    ];
    for (accession, expected) in cases {
        let urls = Locator::parse(&format!("sra:{accession}"))
            .unwrap()
            .urls()
            .unwrap();
        assert!(
            urls[0].contains(expected),
            "{accession} should sit at {expected}, got {}",
            urls[0]
        );
    }
}

#[test]
fn an_sra_accession_of_an_unknown_length_is_refused_rather_than_guessed() {
    // Shorter or longer than ENA's layout admits. A guess here would build
    // a URL that 404s at best and fetches another run's reads at worst.
    for attempt in ["sra:SRR12", "sra:SRR1234567890123"] {
        assert!(
            Locator::parse(attempt).unwrap().urls().is_err(),
            "{attempt} should not resolve"
        );
    }
}

#[test]
fn an_sra_accession_must_look_like_one() {
    for attempt in ["sra:not an accession", "sra:SRR1234/../etc"] {
        assert!(
            Locator::parse(attempt).unwrap().urls().is_err(),
            "{attempt} should not resolve"
        );
    }
}

#[test]
fn an_ncbi_accession_resolves_to_one_efetch_url() {
    let urls = Locator::parse("ncbi:K03455.1").unwrap().urls().unwrap();
    assert_eq!(urls.len(), 1);
    let url = &urls[0];
    assert!(url.starts_with("https://eutils.ncbi.nlm.nih.gov/"), "{url}");
    assert!(url.contains("id=K03455.1"), "{url}");
    assert!(
        url.contains("rettype=fasta"),
        "the engine asks for FASTA, so what arrives can be read as a \
         reference rather than guessed at: {url}"
    );
}

#[test]
fn an_ensembl_path_resolves_under_the_ensembl_ftp_host() {
    let urls = Locator::parse("ensembl:release-110/fasta/homo_sapiens/README")
        .unwrap()
        .urls()
        .unwrap();
    assert_eq!(urls.len(), 1);
    assert!(
        urls[0].starts_with("https://ftp.ensembl.org/pub/"),
        "{}",
        urls[0]
    );
    assert!(urls[0].ends_with("release-110/fasta/homo_sapiens/README"));
}

#[test]
fn an_https_locator_resolves_to_itself() {
    let urls = Locator::parse("https://example.org/refs/x.fasta?v=2")
        .unwrap()
        .urls()
        .unwrap();
    assert_eq!(
        urls,
        vec!["https://example.org/refs/x.fasta?v=2".to_owned()]
    );
}

// ---------------------------------------------------------- path safety --

#[test]
fn a_locator_cannot_climb_out_of_the_host_it_names() {
    // `ensembl:../../etc/passwd` must not become a URL on another host, and
    // must not reach a path the scheme does not own. The scheme is a
    // promise about where the bytes come from, and a traversal would break
    // it without the user seeing anything.
    for attempt in [
        "ensembl:../../etc/passwd",
        "ensembl:/absolute",
        "ncbi:../x",
        "ncbi:K03455.1&db=evil",
    ] {
        assert!(
            Locator::parse(attempt).unwrap().urls().is_err(),
            "{attempt} should not resolve"
        );
    }
}

#[test]
fn the_suffix_a_locator_implies_is_the_one_its_bytes_need() {
    // The store keys on a hash and a suffix, and the suffix has to come from
    // somewhere before anything is fetched.
    assert_eq!(
        Locator::parse("sra:SRR32217726").unwrap().suffixes(),
        vec![".fastq.gz".to_owned(), ".fastq.gz".to_owned()]
    );
    assert_eq!(
        Locator::parse("ncbi:K03455.1").unwrap().suffixes(),
        vec![".fasta".to_owned()]
    );
    assert_eq!(
        Locator::parse("https://example.org/x.vcf.gz")
            .unwrap()
            .suffixes(),
        vec![".vcf.gz".to_owned()]
    );
}
