//! Where a remote input comes from: four schemes and nothing else.
//!
//! A locator is what the *user* asked for — `sra:SRR32217726`,
//! `ncbi:K03455.1` — and it is what provenance and the methods paragraph
//! quote, because it is the thing a reader can go and look up. The URL that
//! answered is a different fact, recorded separately.
//!
//! # Why the set of schemes is closed
//!
//! A scheme is a promise that the engine knows how to turn an accession into
//! bytes *and* how to recognise the answer: `ncbi:` asks eutils for FASTA,
//! `sra:` knows ENA's directory layout and that a run is a pair of files.
//! An open set, or one extensible by configuration, would be a promise the
//! engine could not keep, and the failure would be a fetch of the wrong
//! thing rather than an error. So there are four, and a fifth is a decision
//! rather than a setting.
//!
//! `https:` is the escape hatch for everything else, and it is deliberately
//! the only one: it says "these exact bytes, from this exact URL", which is
//! a promise the user makes rather than one the engine makes for them.
//!
//! # The one exception, and why it is not a fifth scheme
//!
//! A loopback address may be written `http://`. TLS protects a channel
//! against something sitting in it, and between two ends of a loopback
//! socket there is no channel to sit in — so requiring it there buys nothing
//! and costs the ability to run a local server, which is how both the
//! engine's own tests and anyone's mock of an archive work. The host must be
//! exactly `127.0.0.1`, `::1` or `localhost`; `http://` to anywhere else is
//! the error that names the four.

use std::fmt;

/// Which archive a locator names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scheme {
    /// An SRA run accession, served by ENA over HTTPS as a pair of gzipped
    /// FASTQs. No SRA toolkit is needed, which is why ENA rather than NCBI.
    Sra,
    /// An NCBI nucleotide accession, fetched as FASTA through eutils.
    Ncbi,
    /// A path under Ensembl's public FTP tree, served over HTTPS.
    Ensembl,
    /// An exact URL, for everything the other three do not cover.
    Https,
}

impl Scheme {
    /// The prefix a locator of this scheme is written with.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Sra => "sra:",
            Self::Ncbi => "ncbi:",
            Self::Ensembl => "ensembl:",
            Self::Https => "https:",
        }
    }
}

/// Every scheme there is, in the order the error lists them.
pub const SCHEMES: [Scheme; 4] = [Scheme::Sra, Scheme::Ncbi, Scheme::Ensembl, Scheme::Https];

/// The four schemes, as an error names them.
fn the_four() -> String {
    let names: Vec<&str> = SCHEMES.iter().map(|scheme| scheme.prefix()).collect();
    crate::dim::english_list(&names)
}

/// Why a locator could not be read, or could not be turned into a URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocatorError {
    /// The locator names a scheme the engine does not have.
    UnknownScheme {
        /// What was written.
        locator: String,
        /// The scheme part of it.
        scheme: String,
    },
    /// The locator names no scheme at all.
    NoScheme {
        /// What was written.
        locator: String,
    },
    /// The locator names a scheme and nothing after it.
    Empty {
        /// What was written.
        locator: String,
    },
    /// The locator's body is not something this scheme can resolve.
    Unresolvable {
        /// What was written.
        locator: String,
        /// Why it cannot be resolved.
        why: String,
    },
}

impl fmt::Display for LocatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownScheme { locator, scheme } => write!(
                f,
                "`{locator}` names the scheme `{scheme}`, which Ligase does \
                 not have. The schemes are {}",
                the_four()
            ),
            Self::NoScheme { locator } => write!(
                f,
                "`{locator}` names no scheme, so there is nothing to say \
                 which archive it is in. Write one of {}",
                the_four()
            ),
            Self::Empty { locator } => write!(
                f,
                "`{locator}` names a scheme and no resource, so there is \
                 nothing to fetch"
            ),
            Self::Unresolvable { locator, why } => {
                write!(f, "`{locator}` cannot be resolved: {why}")
            }
        }
    }
}

impl std::error::Error for LocatorError {}

/// Where a remote input comes from.
///
/// Parsed once, kept as what the user wrote, and resolved to URLs only when
/// something is about to be fetched — so a pipeline that type-checks against
/// a warm store never builds a URL at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locator {
    written: String,
    scheme: Scheme,
    body: String,
}

impl Locator {
    /// Read a locator, or say which four schemes there are.
    ///
    /// # Errors
    ///
    /// [`LocatorError`] for an unknown scheme, a missing one, or a scheme
    /// with nothing after it.
    pub fn parse(written: &str) -> Result<Self, LocatorError> {
        let written = written.trim();
        if written.is_empty() {
            return Err(LocatorError::NoScheme {
                locator: written.to_owned(),
            });
        }

        // `https://` keeps its slashes, because the body is a whole URL; the
        // other three are `scheme:body`.
        if let Some(body) = written.strip_prefix("https://") {
            return Self::finish(written, Scheme::Https, body);
        }
        // Plain HTTP to a loopback address, which is the same scheme by a
        // different name: see the module documentation for why that is not
        // a hole.
        if let Some(body) = written.strip_prefix("http://")
            && is_loopback(body)
        {
            return Self::finish(written, Scheme::Https, body);
        }

        let Some((scheme, body)) = written.split_once(':') else {
            return Err(LocatorError::NoScheme {
                locator: written.to_owned(),
            });
        };

        let found = SCHEMES.iter().copied().find(|known| {
            known
                .prefix()
                .trim_end_matches(':')
                .eq_ignore_ascii_case(scheme)
        });

        match found {
            // `https:x` without the slashes is not a URL; treat it as empty
            // rather than inventing an authority for it.
            Some(Scheme::Https) => Self::finish(written, Scheme::Https, ""),
            Some(scheme) => Self::finish(written, scheme, body),
            None => Err(LocatorError::UnknownScheme {
                locator: written.to_owned(),
                scheme: scheme.to_owned(),
            }),
        }
    }

    fn finish(written: &str, scheme: Scheme, body: &str) -> Result<Self, LocatorError> {
        if body.trim().is_empty() {
            return Err(LocatorError::Empty {
                locator: written.to_owned(),
            });
        }
        Ok(Self {
            written: written.to_owned(),
            scheme,
            body: body.trim().to_owned(),
        })
    }

    /// Which archive this names.
    #[must_use]
    pub const fn scheme(&self) -> Scheme {
        self.scheme
    }

    /// The accession or path, without the scheme.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// The URLs this locator resolves to, in order.
    ///
    /// More than one for an SRA run, which is a *pair* of FASTQs — and the
    /// order is mate order, because swapping the mates is a different
    /// dataset and the engine hashes the pair in order.
    ///
    /// # Errors
    ///
    /// [`LocatorError::Unresolvable`] where the body is not something this
    /// scheme can turn into a URL: an accession of a length ENA's layout
    /// does not admit, a path that tries to climb out of the tree its
    /// scheme owns, or an accession with characters no accession has.
    pub fn urls(&self) -> Result<Vec<String>, LocatorError> {
        match self.scheme {
            Scheme::Sra => self.ena_urls(),
            Scheme::Ncbi => {
                self.require_plain_accession()?;
                Ok(vec![format!(
                    "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/efetch.fcgi\
                     ?db=nuccore&id={}&rettype=fasta&retmode=text",
                    self.body
                )])
            }
            Scheme::Ensembl => {
                self.require_safe_path()?;
                Ok(vec![format!("https://ftp.ensembl.org/pub/{}", self.body)])
            }
            Scheme::Https => Ok(vec![self.written.clone()]),
        }
    }

    /// The file suffix each of this locator's objects is stored under.
    ///
    /// Known before anything is fetched, because the store keys on a hash
    /// and a suffix and a tool downstream dispatches on the extension.
    #[must_use]
    pub fn suffixes(&self) -> Vec<String> {
        match self.scheme {
            Scheme::Sra => vec![".fastq.gz".to_owned(), ".fastq.gz".to_owned()],
            Scheme::Ncbi => vec![".fasta".to_owned()],
            Scheme::Ensembl | Scheme::Https => {
                vec![suffix_of_url(&self.body)]
            }
        }
    }

    /// ENA lays FASTQs out at `vol1/fastq/<first 6>/<subdir>/<run>/`, where
    /// the subdirectory is the accession's trailing digits zero-padded to
    /// three — and is absent for a 9-character accession.
    ///
    /// Four branches, every one of them used by real accessions. Anything
    /// outside them is refused rather than guessed: a guessed URL 404s at
    /// best and serves another run's reads at worst.
    fn ena_urls(&self) -> Result<Vec<String>, LocatorError> {
        self.require_plain_accession()?;
        let accession = &self.body;
        let subdirectory = match accession.len() {
            9 => String::new(),
            10 => format!("00{}/", &accession[accession.len() - 1..]),
            11 => format!("0{}/", &accession[accession.len() - 2..]),
            12 => format!("{}/", &accession[accession.len() - 3..]),
            length => {
                return Err(LocatorError::Unresolvable {
                    locator: self.written.clone(),
                    why: format!(
                        "an SRA run accession is 9 to 12 characters and this \
                         one is {length}, so ENA's directory layout cannot \
                         place it"
                    ),
                });
            }
        };
        let prefix = &accession[..6];
        Ok((1..=2)
            .map(|mate| {
                format!(
                    "https://ftp.sra.ebi.ac.uk/vol1/fastq/{prefix}/{subdirectory}\
                     {accession}/{accession}_{mate}.fastq.gz"
                )
            })
            .collect())
    }

    /// An accession is letters, digits, dots and underscores, and nothing
    /// else.
    ///
    /// This is what keeps a locator from carrying a path segment, a query
    /// parameter or an authority into a URL the engine builds — which would
    /// turn "fetch this accession from NCBI" into "fetch whatever this says",
    /// silently.
    fn require_plain_accession(&self) -> Result<(), LocatorError> {
        let plain = self
            .body
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
        if plain {
            return Ok(());
        }
        Err(LocatorError::Unresolvable {
            locator: self.written.clone(),
            why: "an accession is letters, digits, dots, dashes and \
                  underscores. This one has something else in it, and a \
                  locator may not carry a path or a query into a URL the \
                  engine builds"
                .to_owned(),
        })
    }

    /// A path under a scheme's own tree, and not a way out of it.
    fn require_safe_path(&self) -> Result<(), LocatorError> {
        let safe = !self.body.starts_with('/')
            && !self.body.split('/').any(|segment| segment == "..")
            && !self.body.contains("//")
            && !self.body.contains(':');
        if safe {
            return Ok(());
        }
        Err(LocatorError::Unresolvable {
            locator: self.written.clone(),
            why: "a path is relative to the archive's own tree and may not \
                  climb out of it, so `..`, a leading `/` and a second \
                  scheme are all refused"
                .to_owned(),
        })
    }
}

impl fmt::Display for Locator {
    /// What the user wrote, which is what provenance quotes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.written)
    }
}

/// Whether a URL's authority is a loopback address.
///
/// Exactly the three spellings of "this machine", because a prefix match
/// would accept `localhost.example.org`, which is not this machine at all.
fn is_loopback(after_scheme: &str) -> bool {
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    let host = authority
        .rsplit_once(':')
        .map_or(authority, |(host, _)| host)
        .trim_start_matches('[')
        .trim_end_matches(']');
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

/// The suffix a URL's last path segment implies, dots and all.
///
/// `.vcf.gz` rather than `.gz`, matching [`crate::store::suffix_of`], so an
/// object fetched from a URL and one produced by a step are named the same
/// way. Empty where the URL's last segment has no dot, which is legal.
fn suffix_of_url(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.rsplit('/')
        .next()
        .and_then(|name| name.find('.').map(|at| name[at..].to_owned()))
        .unwrap_or_default()
}
