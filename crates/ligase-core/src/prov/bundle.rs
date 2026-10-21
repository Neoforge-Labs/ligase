//! A bundle: everything needed to reproduce a result, and to check that it was.
//!
//! `web/content/project/milestones.mdx` asks for "a bundle that reproduces
//! the results on a clean machine". The phrase needs pinning down before it
//! can be built, because taken literally it is impossible: the reference
//! demo's inputs are sequencing runs from the SRA, and a bundle that carried
//! them would be gigabytes. Fetching them is out of scope for v0.1 anyway
//! (D8).
//!
//! So what a bundle carries is everything *except* the data, and the data is
//! named firmly enough that it cannot be substituted: every input by content
//! hash, every reference by accession and checksum. A clean machine is then,
//! to this engine, an empty cache and whatever tools are installed — and
//! what "reproduces the results" means concretely is that a run there
//! produces outputs whose canonical hashes are the ones the bundle recorded.
//!
//! That is a claim, so it can be checked, and [`verify`] checks it. Three
//! outcomes matter and the third is the interesting one:
//!
//! - **identical** — the same key and the same result. Nothing that feeds
//!   the computation moved, so the cache would have served it.
//! - **reproduced** — a *different* key and the same result anyway. A tool
//!   version or a platform moved and the science did not. This is the
//!   outcome a reviewer on another machine actually gets, and the one a
//!   bundle exists to be able to report.
//! - **differs** — a different result. A finding, and the reason the inputs
//!   are checked too: a changed input explains a changed output, and
//!   without that column a verdict here would be uninterpretable.
//!
//! # Why TSV and not JSON
//!
//! No dependency, and a reviewer can open any of these files in a
//! spreadsheet, `grep` it, or read it in a terminal. The engine has no JSON
//! library and does not need one for eight flat tables; the one structured
//! store it has is SQLite, for the same reason (D6). Tabs, newlines and
//! backslashes inside a field are escaped, so a command containing a tab
//! cannot shift a column.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{Methods, Noticed, Prov, ProvError, Record};
use crate::exec::RunReport;
use crate::graph::{Graph, NodeId};
use crate::hash::ContentHash;

/// The hash the manifest uses, named in the manifest itself.
const MANIFEST_HASH: &str = "blake3";

/// What a bundle is built from.
pub struct Request<'a> {
    /// The graph the results came from.
    pub graph: &'a Graph,
    /// The outputs that were asked for.
    pub roots: &'a [NodeId],
    /// The run that produced them.
    pub report: &'a RunReport,
    /// Where to read how each step ran.
    pub prov: &'a Prov,
    /// The methods section, already built.
    pub methods: &'a Methods,
    /// Files to copy in as they are, such as the pipeline itself.
    pub include: &'a [PathBuf],
}

/// What a bundle turned out to hold.
#[derive(Clone, Debug, Default)]
pub struct Summary {
    /// Where it was written.
    pub root: PathBuf,
    /// The files in it, relative to the root, in manifest order.
    pub files: Vec<String>,
    /// How many steps it accounts for.
    pub steps: usize,
    /// Steps it could not account for, because nothing recorded them.
    pub gaps: usize,
}

/// Why a bundle could not be written or read.
#[derive(Debug)]
pub enum BundleError {
    /// A file could not be read or written.
    Io {
        /// Which file.
        path: PathBuf,
        /// The underlying failure.
        source: io::Error,
    },
    /// Provenance could not be read.
    Prov(ProvError),
    /// The bundle is not one, or is damaged.
    Malformed {
        /// Which file.
        path: PathBuf,
        /// What was wrong with it.
        detail: String,
    },
}

impl fmt::Display for BundleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Prov(error) => write!(f, "{error}"),
            Self::Malformed { path, detail } => write!(f, "{}: {detail}", path.display()),
        }
    }
}

impl std::error::Error for BundleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Prov(error) => Some(error),
            Self::Malformed { .. } => None,
        }
    }
}

impl From<ProvError> for BundleError {
    fn from(error: ProvError) -> Self {
        Self::Prov(error)
    }
}

/// How a step's result compared with what the bundle expected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Match {
    /// The same key and the same result: nothing that feeds it moved.
    Identical,
    /// A different key and the same result: something in the environment
    /// moved and the science did not.
    Reproduced,
    /// A different result.
    Differs,
    /// The bundle expected a step this pipeline did not produce.
    Missing,
}

impl fmt::Display for Match {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Identical => "identical",
            Self::Reproduced => "reproduced",
            Self::Differs => "differs",
            Self::Missing => "missing",
        })
    }
}

/// One step, as the bundle recorded it and as this machine produced it.
#[derive(Clone, Debug)]
pub struct Check {
    /// The step's name.
    pub step: String,
    /// What tells this node apart, where anything does.
    pub scope: Option<String>,
    /// The canonical hash the bundle recorded.
    pub expected: ContentHash,
    /// The canonical hash produced here, if the step produced anything.
    pub found: Option<ContentHash>,
    /// Whether the key matched too.
    pub same_key: bool,
    /// The verdict.
    pub verdict: Match,
}

/// Whether an input is the one the bundle was built from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    /// Present, and the same content.
    Same,
    /// Present, and different content.
    Changed,
    /// Not where the bundle said it was.
    Absent,
}

impl fmt::Display for Presence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Same => "same",
            Self::Changed => "changed",
            Self::Absent => "absent",
        })
    }
}

/// One input file, as the bundle recorded it and as it is here.
#[derive(Clone, Debug)]
pub struct InputCheck {
    /// The step it fed.
    pub step: String,
    /// The parameter it filled.
    pub param: String,
    /// Where the bundle said it was.
    pub path: PathBuf,
    /// Whether it is that file.
    pub presence: Presence,
}

/// What checking a bundle against this machine found.
#[derive(Clone, Debug, Default)]
pub struct Verification {
    /// Every step the bundle recorded.
    pub checks: Vec<Check>,
    /// Every input it recorded.
    pub inputs: Vec<InputCheck>,
    /// What canonicalising the outputs produced *here* noticed (D26).
    ///
    /// Read beside a `differs`: a canonical hash that depends on a file
    /// path does not travel, so a result that differs for that reason
    /// differs for a reason that has nothing to do with the science.
    pub noticed: Vec<Noticed>,
}

impl Verification {
    /// Whether every recorded result was reproduced.
    #[must_use]
    pub fn reproduced(&self) -> bool {
        !self.checks.is_empty()
            && self
                .checks
                .iter()
                .all(|check| matches!(check.verdict, Match::Identical | Match::Reproduced))
    }

    /// The worst thing found.
    #[must_use]
    pub fn verdict(&self) -> Match {
        self.checks
            .iter()
            .map(|check| check.verdict)
            .max()
            .unwrap_or(Match::Missing)
    }

    /// One line summarising the check.
    #[must_use]
    pub fn summary(&self) -> String {
        let count = |wanted: Match| {
            self.checks
                .iter()
                .filter(|check| check.verdict == wanted)
                .count()
        };
        let mut parts = Vec::new();
        for verdict in [
            Match::Identical,
            Match::Reproduced,
            Match::Differs,
            Match::Missing,
        ] {
            let found = count(verdict);
            if found > 0 {
                parts.push(format!("{found} {verdict}"));
            }
        }

        let places = self.checks.len();
        let noun = if places == 1 { "result" } else { "results" };
        if parts.is_empty() {
            return "this bundle records no results.".to_owned();
        }
        format!("{places} {noun}: {}.", parts.join(", "))
    }

    /// The whole report, as the command line prints it.
    #[must_use]
    pub fn text(&self) -> String {
        let mut lines = Vec::new();

        for check in &self.checks {
            let named = match &check.scope {
                Some(scope) => format!("`{}` ({scope})", check.step),
                None => format!("`{}`", check.step),
            };
            lines.push(format!("{:<16} {named}", check.verdict.to_string()));
            match check.verdict {
                Match::Identical => lines.push(format!(
                    "    the same key, the same result   {}",
                    short(check.expected)
                )),
                Match::Reproduced => {
                    lines.push(format!(
                        "    the result is the same          {}",
                        short(check.expected)
                    ));
                    lines.push("    the key is not: something in the environment moved".to_owned());
                }
                Match::Differs => lines.push(format!(
                    "    expected {} and got {}",
                    short(check.expected),
                    check.found.map_or_else(|| "nothing".to_owned(), short),
                )),
                Match::Missing => {
                    lines.push("    this pipeline did not produce it".to_owned());
                }
            }
        }

        // Only the inputs that are not as recorded: a run of four hundred
        // samples should not print four hundred lines to say nothing moved.
        let moved: Vec<&InputCheck> = self
            .inputs
            .iter()
            .filter(|input| input.presence != Presence::Same)
            .collect();
        if !moved.is_empty() {
            lines.push(String::new());
            lines.push(format!(
                "{} of {} inputs are not the ones this bundle was built from:",
                moved.len(),
                self.inputs.len()
            ));
            for input in moved {
                lines.push(format!(
                    "    {:<10} `{}` of `{}`: {}",
                    input.presence.to_string(),
                    input.param,
                    input.step,
                    input.path.display()
                ));
            }
            lines
                .push("A changed input explains a changed output, so read these first.".to_owned());
        }

        // Outputs named after a file, which is the other thing that
        // explains a `differs` without any science behind it.
        if !self.noticed.is_empty() {
            lines.push(String::new());
            // Places, not notes: one output with two things wrong with it
            // is one result that will not travel (D15).
            let mut places: Vec<(&str, Option<&str>)> = self
                .noticed
                .iter()
                .map(|item| (item.step.as_str(), item.scope.as_deref()))
                .collect();
            places.dedup();
            lines.push(format!(
                "{} of {} results name a file where a sample identifier belongs:",
                places.len(),
                self.checks.len()
            ));
            for item in &self.noticed {
                let named = match &item.scope {
                    Some(scope) => format!("`{}` ({scope})", item.step),
                    None => format!("`{}`", item.step),
                };
                lines.push(format!("    {named}: {}", item.note));
            }
            lines.push(
                "Such a hash depends on the file it was named after, so a `differs` here may \
                 be that and nothing else."
                    .to_owned(),
            );
            // Said once per kind rather than once per result (D15). The
            // whole of why it matters is in the bundle's own README; here
            // there is room for the instruction and not the essay.
            let mut given: Vec<&str> = Vec::new();
            for item in &self.noticed {
                if !given.contains(&item.note.code()) {
                    given.push(item.note.code());
                    lines.push(format!("    {}", item.note.advice()));
                }
            }
        }

        if !lines.is_empty() {
            lines.push(String::new());
        }
        lines.push(self.summary());
        lines.join("\n")
    }
}

/// Write a bundle into `into`, creating it.
///
/// # Errors
///
/// If the directory cannot be written, or provenance cannot be read.
pub fn write(into: &Path, request: &Request<'_>) -> Result<Summary, BundleError> {
    create_dir(into)?;

    let scopes = request.graph.scopes(request.roots);
    let mut rows = Vec::new();
    for result in &request.report.results {
        let record = request.prov.lookup(result.key)?;
        rows.push((
            result,
            scopes.get(&result.node).map(|scope| scope.value.clone()),
            record,
        ));
    }

    let gaps = rows.iter().filter(|(_, _, held)| held.is_none()).count();

    // ---- the prose
    let mut methods = String::from("# Methods\n\n");
    methods.push_str(&request.methods.text());
    methods.push('\n');
    put(into, "METHODS.md", &methods)?;
    put(into, "README.md", &readme(request.methods))?;

    // ---- the claim
    let mut results = Table::new(&["step", "scope", "key", "bytes", "canonical"]);
    for (result, scope, _) in &rows {
        results.row(&[
            &result.step,
            scope.as_deref().unwrap_or(""),
            &result.key.to_hex(),
            &result.hash.to_hex(),
            &result.canonical.to_hex(),
        ]);
    }
    put(into, "results.tsv", &results.finish())?;

    // ---- how each one ran, verbatim
    let mut commands = Table::new(&["step", "scope", "key", "command"]);
    for (result, scope, record) in &rows {
        let command = record
            .as_ref()
            .and_then(|record| record.command.clone())
            .unwrap_or_default();
        commands.row(&[
            &result.step,
            scope.as_deref().unwrap_or(""),
            &result.key.to_hex(),
            &command,
        ]);
    }
    put(into, "commands.tsv", &commands.finish())?;

    // ---- what went in
    let mut inputs = Table::new(&[
        "step",
        "scope",
        "param",
        "kind",
        "path",
        "bytes",
        "canonical",
    ]);
    for (result, scope, record) in &rows {
        let Some(record) = record else { continue };
        for input in &record.inputs {
            inputs.row(&[
                &result.step,
                scope.as_deref().unwrap_or(""),
                &input.param,
                input.kind.name(),
                &input.path.to_string_lossy(),
                &input.bytes.to_hex(),
                &input.canonical.to_hex(),
            ]);
        }
    }
    put(into, "inputs.tsv", &inputs.finish())?;

    // ---- what canonicalising the outputs noticed (D26)
    //
    // Its own file rather than a column on `results.tsv`, because that
    // table is the bundle's claim and a reader — or a later version of this
    // engine — parses it by position. Almost always just a header line.
    let mut notes = Table::new(&["step", "scope", "key", "code", "value"]);
    for (result, scope, record) in &rows {
        let Some(record) = record else { continue };
        for note in &record.output_notes {
            notes.row(&[
                &result.step,
                scope.as_deref().unwrap_or(""),
                &result.key.to_hex(),
                note.code(),
                &note.value(),
            ]);
        }
    }
    put(into, "notes.tsv", &notes.finish())?;

    // ---- the references, pinned
    let mut references = Table::new(&[
        "param",
        "path",
        "build",
        "accession",
        "contigs",
        "length",
        "checksum",
        "fingerprint",
    ]);
    for reference in request.methods.references() {
        references.row(&[
            &reference.param,
            &reference.path.to_string_lossy(),
            reference.build.as_deref().unwrap_or(""),
            reference.accession.as_deref().unwrap_or(""),
            &reference.contigs.to_string(),
            &reference.length.to_string(),
            reference.checksum.as_deref().unwrap_or(""),
            &reference.fingerprint,
        ]);
    }
    put(into, "references.tsv", &references.finish())?;

    // ---- what it was run with
    let mut environment = Table::new(&["name", "value"]);
    environment.row(&["ligase", &request.methods.engine]);
    environment.row(&["platform", &request.methods.platform]);
    for (tool, version) in request.methods.tools() {
        environment.row(&[&tool, &version]);
    }
    put(into, "environment.tsv", &environment.finish())?;

    // ---- the code of every step, so the bundle holds the pipeline's logic
    // even when the file that held it is lost
    create_dir(&into.join("steps"))?;
    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    for (_, _, record) in &rows {
        let Some(record) = record else { continue };
        sources.insert(step_file(record, &sources), record.code.clone());
    }
    for (name, code) in &sources {
        put(into, &format!("steps/{name}"), code)?;
    }

    // ---- whatever the caller wanted carried along
    if !request.include.is_empty() {
        create_dir(&into.join("pipeline"))?;
        for source in request.include {
            let Some(name) = source.file_name() else {
                continue;
            };
            let destination = into.join("pipeline").join(name);
            fs::copy(source, &destination).map_err(|error| BundleError::Io {
                path: source.clone(),
                source: error,
            })?;
        }
    }

    // ---- and finally the bundle's own contents, hashed
    let files = manifest(into)?;

    Ok(Summary {
        root: into.to_path_buf(),
        files,
        steps: rows.len() - gaps,
        gaps,
    })
}

/// Check a bundle's recorded results against a run on this machine.
///
/// # Errors
///
/// If the bundle cannot be read, or does not hold the files it must.
pub fn verify(
    bundle: &Path,
    graph: &Graph,
    roots: &[NodeId],
    report: &RunReport,
) -> Result<Verification, BundleError> {
    let scopes = graph.scopes(roots);
    let here: Vec<(String, Option<String>, &crate::exec::StepResult)> = report
        .results
        .iter()
        .map(|result| {
            (
                result.step.clone(),
                scopes.get(&result.node).map(|scope| scope.value.clone()),
                result,
            )
        })
        .collect();

    let mut verification = Verification::default();

    for result in &report.results {
        // Only the notes that can account for a hash that moved. The rest
        // say what a result rested on, which belongs in the run report and
        // the methods section and is no reason for two machines to
        // disagree: offered here, it would send a reader after the wrong
        // thing.
        for note in result
            .notes
            .iter()
            .filter(|note| note.explains_a_difference())
        {
            verification.noticed.push(Noticed {
                step: result.step.clone(),
                scope: scopes.get(&result.node).map(|scope| scope.value.clone()),
                scope_kind: scopes.get(&result.node).map(|scope| scope.kind),
                note: note.clone(),
            });
        }
    }

    for row in read_table(bundle, "results.tsv", 5)? {
        let (step, scope, key, _bytes, canonical) = (&row[0], &row[1], &row[2], &row[3], &row[4]);
        let scope = (!scope.is_empty()).then(|| scope.clone());
        let expected = parse_hash(bundle, "results.tsv", canonical)?;

        let found = here
            .iter()
            .find(|(name, at, _)| name == step && at == &scope);

        let check = match found {
            None => Check {
                step: step.clone(),
                scope,
                expected,
                found: None,
                same_key: false,
                verdict: Match::Missing,
            },
            Some((_, _, result)) => {
                let same_key = result.key.to_hex() == *key;
                let verdict = if result.canonical == expected {
                    if same_key {
                        Match::Identical
                    } else {
                        Match::Reproduced
                    }
                } else {
                    Match::Differs
                };
                Check {
                    step: step.clone(),
                    scope,
                    expected,
                    found: Some(result.canonical),
                    same_key,
                    verdict,
                }
            }
        };
        verification.checks.push(check);
    }

    // The inputs are checked by content, because that is the only way to
    // tell the file the bundle was built from apart from a file with the
    // same name. A bundle cannot fetch them (D8), so it says what it found.
    for row in read_table(bundle, "inputs.tsv", 7)? {
        let (step, param, path, bytes) = (&row[0], &row[2], &row[4], &row[5]);
        let path = PathBuf::from(path);
        let recorded = parse_hash(bundle, "inputs.tsv", bytes)?;

        let presence = if path.is_file() {
            match ContentHash::of_file(&path) {
                Ok(hash) if hash == recorded => Presence::Same,
                Ok(_) => Presence::Changed,
                Err(_) => Presence::Absent,
            }
        } else {
            Presence::Absent
        };

        verification.inputs.push(InputCheck {
            step: step.clone(),
            param: param.clone(),
            path,
            presence,
        });
    }

    Ok(verification)
}

/// What a bundle says about itself.
fn readme(methods: &Methods) -> String {
    let mut out = String::from(
        "# Reproducibility bundle\n\n\
         Written by Ligase. Everything here describes how a set of results was\n\
         produced, and is enough to check whether another machine produces the\n\
         same ones.\n\n\
         ## What is here\n\n\
         | File | What it holds |\n\
         | --- | --- |\n\
         | `METHODS.md` | The methods paragraph, with tool versions and references. |\n\
         | `results.tsv` | Every step's output, by content hash. **This is the claim.** |\n\
         | `commands.tsv` | The command that produced each one, as it ran. |\n\
         | `inputs.tsv` | Every input file, by content hash. |\n\
         | `notes.tsv` | What canonicalising an output noticed. Usually empty. |\n\
         | `references.tsv` | Every reference, by accession and checksum. |\n\
         | `environment.tsv` | The engine, the platform and every tool's version. |\n\
         | `steps/` | The source of every step. |\n\
         | `pipeline/` | The pipeline file itself, where one was given. |\n\
         | `MANIFEST` | Every file above, hashed, so damage is visible. |\n\n\
         ## What is not here\n\n\
         The data. Sequencing runs are gigabytes and a bundle is meant to be\n\
         readable, so inputs are *named* rather than carried: every one by\n\
         content hash, and every reference by accession and checksum. Fetch\n\
         them by those identifiers; nothing else will satisfy the check.\n\n\
         ## How to check it\n\n\
         With the data in place and the pipeline to hand:\n\n\
         ```\n\
         ligase methods --verify <this directory>\n\
         ```\n\n\
         It runs the pipeline and compares every output's canonical hash with\n\
         the one recorded here. Three answers are good news and one is not:\n\
         `identical` (nothing moved), `reproduced` (a tool version or a\n\
         platform moved and the result did not), `differs` (it did), and\n\
         `missing` (this pipeline no longer produces that step).\n\n\
         A `differs` is worth reading alongside the input lines: an input\n\
         reported as `changed` or `absent` explains an output that moved.\n\n",
    );

    // Only when there is one: a standing paragraph about a problem this
    // bundle does not have would be noise in every bundle that is fine.
    let notices = methods.notices();
    if !notices.is_empty() {
        // One heading for both kinds of note: a sample identifier that will
        // not travel, and an assumption a result rests on. Each says which
        // it is in its own words, and a reader of a bundle wants the whole
        // list in one place either way.
        out.push_str("## What was noticed\n\n");
        out.push_str(&notices.join("\n\n"));
        out.push_str("\n\n");
    }

    out.push_str("## What produced it\n\n");
    out.push_str(&format!(
        "Ligase {} on {}.\n",
        methods.engine, methods.platform
    ));
    out
}

/// A name for a step's source file, distinct from the ones already used.
///
/// Two groups of one step can have different code — the pipeline was edited
/// between runs — and a bundle that let the second overwrite the first would
/// be describing a run that never happened.
fn step_file(record: &Record, used: &BTreeMap<String, String>) -> String {
    let base = record
        .step
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();

    let first = format!("{base}.py");
    if used.get(&first).is_none_or(|held| held == &record.code) {
        return first;
    }
    for attempt in 2.. {
        let next = format!("{base}.{attempt}.py");
        if used.get(&next).is_none_or(|held| held == &record.code) {
            return next;
        }
    }
    first
}

/// Hash every file in the bundle, and write the manifest.
fn manifest(root: &Path) -> Result<Vec<String>, BundleError> {
    let mut files = Vec::new();
    collect(root, root, &mut files)?;
    files.sort();

    let mut out = format!("# {MANIFEST_HASH}\n");
    for name in &files {
        let hash = ContentHash::of_file(&root.join(name)).map_err(|error| BundleError::Io {
            path: root.join(name),
            source: error,
        })?;
        out.push_str(&format!("{}  {name}\n", hash.to_hex()));
    }
    put(root, "MANIFEST", &out)?;

    Ok(files)
}

fn collect(root: &Path, at: &Path, into: &mut Vec<String>) -> Result<(), BundleError> {
    let entries = fs::read_dir(at).map_err(|error| BundleError::Io {
        path: at.to_path_buf(),
        source: error,
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| BundleError::Io {
            path: at.to_path_buf(),
            source: error,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect(root, &path, into)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            into.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

// ------------------------------------------------------------ the files --

/// A TSV being built, with its header.
struct Table {
    text: String,
}

impl Table {
    fn new(columns: &[&str]) -> Self {
        Self {
            text: format!("{}\n", columns.join("\t")),
        }
    }

    fn row(&mut self, fields: &[&str]) {
        let escaped: Vec<String> = fields.iter().map(|field| escape(field)).collect();
        self.text.push_str(&escaped.join("\t"));
        self.text.push('\n');
    }

    fn finish(self) -> String {
        self.text
    }
}

/// Escape a field so it cannot shift a column.
fn escape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    for character in field.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

/// Read back what [`escape`] wrote.
fn unescape(field: &str) -> String {
    let mut out = String::with_capacity(field.len());
    let mut characters = field.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('\\') => out.push('\\'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Every row of one of the bundle's tables, unescaped.
fn read_table(bundle: &Path, name: &str, columns: usize) -> Result<Vec<Vec<String>>, BundleError> {
    let path = bundle.join(name);
    let text = fs::read_to_string(&path).map_err(|error| BundleError::Io {
        path: path.clone(),
        source: error,
    })?;

    let mut lines = text.lines();
    let header = lines.next().ok_or_else(|| BundleError::Malformed {
        path: path.clone(),
        detail: "the file is empty; it should begin with a header row".to_owned(),
    })?;
    if header.split('\t').count() != columns {
        return Err(BundleError::Malformed {
            path: path.clone(),
            detail: format!(
                "the header has {} columns and should have {columns}",
                header.split('\t').count()
            ),
        });
    }

    let mut rows = Vec::new();
    for (number, line) in lines.enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<String> = line.split('\t').map(unescape).collect();
        if fields.len() != columns {
            return Err(BundleError::Malformed {
                path: path.clone(),
                detail: format!(
                    "row {} has {} columns and should have {columns}",
                    number + 2,
                    fields.len()
                ),
            });
        }
        rows.push(fields);
    }
    Ok(rows)
}

fn parse_hash(bundle: &Path, name: &str, text: &str) -> Result<ContentHash, BundleError> {
    ContentHash::parse(text).ok_or_else(|| BundleError::Malformed {
        path: bundle.join(name),
        detail: format!("`{text}` is not a content hash"),
    })
}

fn put(root: &Path, name: &str, contents: &str) -> Result<(), BundleError> {
    let path = root.join(name);
    fs::write(&path, contents).map_err(|error| BundleError::Io {
        path,
        source: error,
    })
}

fn create_dir(path: &Path) -> Result<(), BundleError> {
    fs::create_dir_all(path).map_err(|error| BundleError::Io {
        path: path.to_path_buf(),
        source: error,
    })
}

/// The first twelve characters of a hash, which is plenty to compare by eye.
fn short(hash: ContentHash) -> String {
    hash.to_hex().chars().take(12).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_survives_a_tab_and_a_newline() {
        let awkward = "a\tb\nc\\d";
        assert_eq!(escape(awkward), "a\\tb\\nc\\\\d");
        assert_eq!(unescape(&escape(awkward)), awkward);
        assert!(
            !escape(awkward).contains('\t'),
            "an escaped field cannot shift a column"
        );
    }

    #[test]
    fn an_ordinary_field_is_left_alone() {
        assert_eq!(escape("minimap2 2.31-r1287"), "minimap2 2.31-r1287");
    }
}
