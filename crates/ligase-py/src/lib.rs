//! PyO3 bindings exposing the Ligase core engine as `ligase._core`.
//!
//! This module is deliberately private and thin: it translates between
//! Python and the core engine and holds no logic of its own. The public
//! Python API lives in `python/ligase/__init__.py`.
//!
//! In particular, error *wording* is generated in the core so that Rust
//! tests own it, and this layer hands Python plain tuples rather than
//! building a class hierarchy — the exception classes are defined in Python,
//! where subclassing and type stubs are straightforward.

use std::path::{Path, PathBuf};

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use ligase::audit;
use ligase::cache::Cache;
use ligase::check::{Binding, CheckError, check_step as core_check_step, render, summarise};
use ligase::convert::{self, Conversion};
use ligase::dim::{CoordinateSystem, Dimension, Representation};
use ligase::exec::{self, Host, Options, Recipe, Stores, Task};
use ligase::graph::{Argument, Converted, Graph, NodeId, StepSpec};
use ligase::hash::ContentHash;
use ligase::hivdb;
use ligase::inspect;
use ligase::load::{self, Declared, IntervalFormat, LoadError as CoreLoadError};
use ligase::note::Note;
use ligase::prov::{self, Prov};
use ligase::remote::Locator;
use ligase::remote::fetch::{Network, Offline, Policy, Transport};
use ligase::remote::pinned;
use ligase::remote::recorded::{self, MAX_RECORDED_BYTES, Mode};
use ligase::remote::request::Auth;
use ligase::types::{DataKind, Dataset};

/// One contig as Python sees it: name, length, checksum.
type PyContig = (String, Option<u64>, Option<String>);

/// One disagreement as Python sees it: kind, step, dimension, scope, message.
///
/// Python turns the kind into the matching exception class. The message is
/// built in the core so that the Rust tests assert on the exact wording. The
/// dimension is absent for a step-type error, which blames no dimension; the
/// scope is absent for a step that does not stand for one sample.
type PyCheckError = (
    &'static str,
    String,
    Option<&'static str>,
    Option<(&'static str, String)>,
    String,
);

/// One step argument as Python sees it: parameter, declared kind, node.
type PyArgument = (String, String, usize);

/// One task as Python gives it: node, code, command, callback, tools,
/// parameters, citations, the seed parameter's name.
///
/// Exactly one of the command and the callback is set. A command is run by
/// the core; a callback goes back to Python, which is the only thing in a
/// run that takes the GIL.
///
/// The seed is a name and not a value: the value is already in the
/// parameters, and what the declaration adds is which of them it is.
type PyTask = (
    usize,
    String,
    Option<String>,
    Option<u64>,
    Vec<String>,
    Vec<(String, String)>,
    Vec<String>,
    Option<String>,
);

/// One finished step: node, step, outcome, key, output hash, canonical
/// hash, output, milliseconds, and whatever canonicalising the output
/// noticed (D26).
type PyRunResult = (
    usize,
    String,
    String,
    String,
    String,
    String,
    String,
    u64,
    Vec<String>,
);

/// One failed step: node, step, message.
type PyRunFailure = (usize, String, String);

/// A whole run as Python sees it: the steps that finished, the ones that
/// failed, the summary line, the report as text, and how many input files
/// were hashed, how many the prefilter already knew, and how many had to be
/// parsed to find what they mean.
type PyRunReport = (
    Vec<PyRunResult>,
    Vec<PyRunFailure>,
    String,
    String,
    (u64, u64, u64),
);

/// What an audit is about, as Python gives it: the step's name, and the one
/// sample to narrow to.
type PySubject = (String, Option<String>);

/// One audited step: node, step, scope, verdict, the two output hashes, the
/// two canonical hashes, the two durations in milliseconds, and the seed it
/// was held at.
///
/// The seed is the parameter's name and its value together, because the
/// name alone is not what a reader of a verdict needs.
type PyStepAudit = (
    usize,
    String,
    Option<String>,
    String,
    String,
    String,
    String,
    String,
    u64,
    u64,
    Option<(String, String)>,
);

/// A whole audit as Python sees it: the steps audited, the ones that failed,
/// the overall verdict, the summary line and the report as text.
type PyAuditReport = (Vec<PyStepAudit>, Vec<PyRunFailure>, String, String, String);

/// One reference a step was given: parameter, path, build, accession, how
/// many sequences, how many bases, checksum, fingerprint.
type PyReference = (
    String,
    String,
    Option<String>,
    Option<String>,
    usize,
    u64,
    Option<String>,
    String,
);

/// One way a step was run: step, how many nodes, what tells them apart,
/// what kind of thing that is, tools, parameters, the command as written.
type PyMethodsGroup = (
    String,
    usize,
    Vec<String>,
    Option<String>,
    Vec<(String, String)>,
    Vec<(String, String)>,
    Option<String>,
);

/// A step nothing is known about: step, what tells its nodes apart, how
/// many of them.
type PyMethodsGap = (String, Vec<String>, usize);

/// One claim as Python gives it: the sentence, the step, the sample, and the
/// value it restates — a parameter name, a tool name, and the value, of which
/// at most one of the two names is set.
///
/// Flat here and a sum type in the core, deliberately. A user writes
/// `parameter=` or `tool=` beside `value=`, which is the readable surface;
/// the core cannot hold both at once, and `Claim.__post_init__` is where an
/// incoherent pair is refused.
type PyClaim = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// One computation a claim rests on: the sample, the key, the output's
/// canonical hash, when that computation happened and where.
type PyBoundTo = (Option<String>, String, String, i64, String);

/// One claim checked: the claim as declared, the verdict, the sentence the
/// write-up prints for it, the value it states and the value recorded where
/// those differ, and the computations it rests on.
type PyClaimCheck = (
    PyClaim,
    String,
    Option<String>,
    Option<String>,
    Vec<PyBoundTo>,
);

/// A methods section as Python sees it: the ways steps were run, the gaps,
/// the references, the reference list, the tools with nothing to cite, the
/// claims, the engine, the platform, the summary line and the whole thing as
/// text.
type PyMethods = (
    Vec<PyMethodsGroup>,
    Vec<PyMethodsGap>,
    Vec<PyReference>,
    Vec<String>,
    Vec<String>,
    Vec<PyClaimCheck>,
    String,
    String,
    String,
    String,
);

/// How to write the run up, as Python gives it: a directory to write a
/// bundle into, a bundle to check against, files to carry into the bundle as
/// they are, whether the bundle is thin, and the claims to bind.
///
/// The claims ride here rather than as an argument of their own because they
/// change only the write-up — not the run, not a key, not a record — and
/// because every one of these is read at the same moment, after the run and
/// before the bundle.
type PyWriteUp = (
    Option<PathBuf>,
    Option<PathBuf>,
    Vec<PathBuf>,
    bool,
    Vec<PyClaim>,
);

/// A bundle that was written: where it is, what is in it, how many steps it
/// accounts for, how many it could not.
type PyBundle = (String, Vec<String>, usize, usize);

/// One result checked against a bundle: step, scope, verdict, the hash
/// expected, the hash found, whether the key matched too.
type PyCheck = (String, Option<String>, String, String, Option<String>, bool);

/// One input checked against a bundle: step, parameter, path, what was found.
type PyInputCheck = (String, String, String, String);

/// A bundle checked as Python sees it: the results, the inputs, whether
/// everything was reproduced, the worst verdict, the summary and the text.
type PyVerification = (
    Vec<PyCheck>,
    Vec<PyInputCheck>,
    bool,
    String,
    String,
    String,
);

/// Python's step bodies, for the steps that are not shell commands.
struct PyHost {
    callbacks: Vec<Py<PyAny>>,
}

impl Host for PyHost {
    fn call(
        &self,
        id: u64,
        out: &Path,
        inputs: &[(String, Vec<PathBuf>)],
    ) -> Result<Vec<Note>, String> {
        let index = usize::try_from(id).map_err(|_| format!("no callback {id}"))?;
        let callback = self
            .callbacks
            .get(index)
            .ok_or_else(|| format!("no callback {id} was registered for this run"))?;

        let bound: Vec<(String, Vec<String>)> = inputs
            .iter()
            .map(|(param, files)| {
                (
                    param.clone(),
                    files
                        .iter()
                        .map(|file| file.to_string_lossy().into_owned())
                        .collect(),
                )
            })
            .collect();

        // The only place a run takes the GIL.
        let said: Vec<(String, String)> = Python::attach(|py| {
            callback
                .call1(py, (out.to_string_lossy().into_owned(), bound))
                .map_err(|error| error.to_string())?
                .extract(py)
                .map_err(|error| {
                    format!("this step's body did not say what it noticed as pairs of a note's code and its value: {error}")
                })
        })?;

        // Loud rather than silent. The codes come from this engine's own
        // notes, so one it does not know is a mistake in a body -- and a
        // note quietly dropped here is exactly the silence this door
        // exists to end (D32).
        said.iter()
            .map(|(code, value)| {
                Note::parse(code, value).ok_or_else(|| {
                    format!("this step's body said it noticed `{code}`, which is not a note this engine has")
                })
            })
            .collect()
    }
}

fn translate(error: &CheckError) -> PyCheckError {
    (
        error.kind().name(),
        error.step().to_owned(),
        error.dimension().map(Dimension::name),
        error.scope().map(|scope| (scope.kind, scope.value.clone())),
        error.to_string(),
    )
}

/// Turn a type annotation's name into a kind, or say what the names are.
fn kind(name: &str) -> PyResult<DataKind> {
    DataKind::from_name(name).ok_or_else(|| {
        let known: Vec<&str> = DataKind::ALL.iter().map(|kind| kind.name()).collect();
        PyValueError::new_err(format!(
            "`{name}` is not a Ligase data type; expected one of {}",
            known.join(", ")
        ))
    })
}

create_exception!(
    _core,
    LoadError,
    PyException,
    "A file could not be read or did not parse as the expected format."
);

create_exception!(
    _core,
    RemoteError,
    PyException,
    "A remote input could not be fetched, or is not what its digest says."
);

create_exception!(
    _core,
    FrameError,
    PyValueError,
    "A reference is not numbered along the frame a step's coordinates mean."
);

fn to_py(error: CoreLoadError) -> PyErr {
    LoadError::new_err(error.to_string())
}

/// A loaded dataset and the dimensions inferred from it.
#[pyclass(name = "Dataset", frozen, module = "ligase._core")]
struct PyDataset {
    inner: Dataset,
}

#[pymethods]
impl PyDataset {
    /// The name of the kind of data this holds.
    #[getter]
    fn kind(&self) -> &'static str {
        self.inner.kind().name()
    }

    /// The file it was loaded from.
    #[getter]
    fn path(&self) -> Option<String> {
        self.inner.path().map(|path| path.display().to_string())
    }

    /// The dimensions that mean anything for this kind of data.
    fn applicable(&self) -> Vec<&'static str> {
        Dimension::ALL
            .into_iter()
            .filter(|dimension| self.inner.kind().applies(*dimension))
            .map(Dimension::name)
            .collect()
    }

    /// A dimension's rendered value and provenance, if it is known.
    fn dimension(&self, name: &str) -> Option<(String, String)> {
        let dimension = Dimension::from_name(name)?;
        if !self.inner.kind().applies(dimension) {
            return None;
        }
        let dimensions = self.inner.dimensions();
        Some((
            dimensions.render(dimension)?,
            dimensions.source(dimension)?.describe(),
        ))
    }

    /// Why an applicable dimension has no value.
    fn unknown_reason(&self, name: &str) -> Option<&'static str> {
        let dimension = Dimension::from_name(name)?;
        if !self.inner.kind().applies(dimension) {
            return None;
        }
        self.inner.dimensions().reason(dimension)
    }

    /// The contigs of the reference this data is on, if it has one.
    fn contigs(&self) -> Option<Vec<PyContig>> {
        Some(
            self.inner
                .dimensions()
                .reference
                .value()?
                .contigs()
                .iter()
                .map(|contig| (contig.name.clone(), contig.length, contig.md5.clone()))
                .collect(),
        )
    }

    /// Describe this dataset in words, under the name a message would use.
    ///
    /// For `ligase inspect`, which only looks: the kind, every dimension
    /// that applies with its value and source or the reason it has none,
    /// the dimensions that do not apply, and whatever loading noticed.
    /// Rendered by the engine for the same reason `notes` is -- a
    /// description and a diagnostic about one file should be the same
    /// sentences, not two accounts of it.
    fn describe(&self, name: &str) -> String {
        inspect::describe(&Binding {
            name: name.to_owned(),
            dataset: self.inner.clone(),
        })
    }

    /// What loading this noticed, as one sentence each.
    ///
    /// Rendered here rather than handed over as codes, the way
    /// `StepRun.notes` already is: the wording belongs to the engine, so
    /// that every surface says the same thing about the same file.
    fn notes(&self) -> Vec<String> {
        self.inner.notes().iter().map(ToString::to_string).collect()
    }

    /// Every file this data is, in order.
    ///
    /// One for most kinds and two for a pair of FASTQ mates, in mate
    /// order. `path` names the primary file, which is what a message
    /// quotes; this is what code that has to open all of them wants.
    fn files(&self) -> Vec<String> {
        self.inner
            .files()
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect()
    }

    /// Where this was fetched from, if it was fetched.
    ///
    /// A tuple rather than a class, because the Python layer owns the
    /// wording and the shape a user reads: `ligase.types.RemoteOrigin`.
    fn origin(&self) -> Option<(String, String, String, i64, String)> {
        let origin = self.inner.origin()?;
        Some((
            origin.locator.clone(),
            origin.digest.to_hex(),
            origin.bytes.to_hex(),
            origin.fetched_at,
            origin.verdict.to_string(),
        ))
    }

    /// The samples this data covers, in order, if they are known.
    fn samples(&self) -> Option<Vec<String>> {
        Some(
            self.inner
                .dimensions()
                .sample_set
                .value()?
                .as_slice()
                .to_vec(),
        )
    }
}

/// Interpret variant calls as drug resistance, writing the report.
///
/// The reference is required and not a convenience: a VCF holds only the
/// positions that differ, and two of the three bases of a codon are
/// usually not among them, so nothing can be translated without the
/// sequence those positions sit in.
///
/// Returns what the reading noticed about itself, as pairs of a note's
/// code and its value: what `lg.write` expects a body to hand back, so a
/// step can return this straight to the engine and have it said in the run
/// report, provenance, a bundle and the methods section (D32). Empty only
/// for a reading with an alignment, full coverage and every codon phased.
#[pyfunction]
#[pyo3(signature = (reference, variants, out, alignment=None, algorithm=None, mixture=None, depth=None, gene_coverage=None))]
#[expect(
    clippy::too_many_arguments,
    reason = "each is a separately documented judgement that belongs in the cache key; bundling them would hide them from the Python signature this mirrors"
)]
fn interpret_resistance(
    reference: PathBuf,
    variants: PathBuf,
    out: PathBuf,
    alignment: Option<PathBuf>,
    algorithm: Option<String>,
    mixture: Option<f64>,
    depth: Option<u32>,
    gene_coverage: Option<f64>,
) -> PyResult<Vec<(String, String)>> {
    hivdb::write_report(
        &reference,
        &variants,
        alignment.as_deref(),
        algorithm.as_deref().unwrap_or(hivdb::ruleset::DEFAULT),
        mixture.unwrap_or(hivdb::codon::MIXTURE),
        depth.unwrap_or(hivdb::codon::DEPTH),
        gene_coverage.unwrap_or(hivdb::codon::GENE_COVERAGE),
        &out,
    )
    .map(|notes| {
        notes
            .iter()
            .map(|note| (note.code().to_owned(), note.value()))
            .collect()
    })
    .map_err(|error| match error {
        // Distinct because the fix is distinct: not a different file, but
        // the same sequence put back where its numbers mean what they say.
        hivdb::ReportError::Frame(detail) => FrameError::new_err(detail),
        hivdb::ReportError::Other(detail) => PyValueError::new_err(detail),
    })
}

/// Why a name is not a registered conversion, naming the ones that are.
///
/// Built here rather than inline, because the decorator and the graph both
/// refuse an unknown conversion and a user should get the same sentence
/// whichever of them catches it.
fn unregistered(name: &str) -> String {
    let known: Vec<&str> = Conversion::ALL.iter().map(|entry| entry.name).collect();
    format!(
        "`{name}` is not a registered conversion; this engine ships {}",
        known.join(" and ")
    )
}

/// The axis a registered conversion crosses, or an error naming the ones
/// that exist.
///
/// Asked by the step decorator, so that a step claiming a conversion the
/// engine does not have is refused where it is written.
#[pyfunction]
fn conversion_axis(name: &str) -> PyResult<&'static str> {
    Conversion::by_name(name)
        .map(|conversion| conversion.axis.name())
        .ok_or_else(|| PyValueError::new_err(unregistered(name)))
}

/// The parameter every conversion reads its target from.
#[pyfunction]
fn conversion_target_parameter() -> &'static str {
    Conversion::TARGET
}

/// Rewrite a gene list's identifiers through a mapping table.
///
/// Returns what the conversion noticed, as pairs of a note's code and its
/// value: what `lg.write` expects a body to hand back, so the engine says
/// it in the run report, provenance, a bundle and the methods section.
/// Every identifier that did not cross is named there.
#[pyfunction]
fn map_identifiers(
    entities: PathBuf,
    id_column: &str,
    table: PathBuf,
    table_id_column: &str,
    column: &str,
    out: PathBuf,
) -> PyResult<Vec<(String, String)>> {
    convert::map_identifiers(&entities, id_column, &table, table_id_column, column, &out)
        .map(|notes| {
            notes
                .iter()
                .map(|note| (note.code().to_owned(), note.value()))
                .collect()
        })
        .map_err(PyValueError::new_err)
}

/// The reference this engine has for a tool, if it has a checked one.
///
/// `None` for a tool the citation table does not cover, which a methods
/// section then reports as uncited (D24) rather than omitting.
///
/// This is here for `ligase.steps`. A library step is admitted only when
/// its tool's citation resolves (D42), and the test that holds a future
/// contributor to that has to be able to run on a machine with none of the
/// tools installed — otherwise the criterion is checked on one machine in
/// the world and is not a criterion.
#[pyfunction]
fn citation(tool: &str) -> Option<String> {
    prov::cite::of(tool).map(prov::cite::Citation::render)
}

/// How a file of this kind would be hashed: `canonical`, or `raw`.
///
/// `raw` means the file did not parse as the kind it was read as, so its
/// bytes stood in for its meaning. That is the conservative fallback and
/// never a wrong answer in itself — but for a step's *output* it is a
/// quiet loss, because every volatile field the canonical form would have
/// stripped is then hashed, and early cutoff stops working below that step.
///
/// Here for the same reason as [`citation`]: a library step declares how
/// its output canonicalises (D42), and a declaration that nothing checks
/// is a comment. It is what catches a step annotated `-> Alignment` whose
/// output is in fact a BAM *index*, which parses as neither and so would
/// be hashed as an alignment's meaning with nothing saying so.
#[pyfunction]
fn canonical_form(kind_name: &str, path: PathBuf) -> PyResult<&'static str> {
    let data = kind(kind_name)?;
    let raw = ContentHash::of_file(&path).map_err(|error| {
        LoadError::new_err(format!("{} could not be read: {error}", path.display()))
    })?;
    ligase::canon::hash(data, &path, raw)
        .map(|canonical| match canonical.form {
            ligase::canon::Form::Canonical => "canonical",
            ligase::canon::Form::Raw => "raw",
        })
        .map_err(|error| {
            LoadError::new_err(format!("{} could not be read: {error}", path.display()))
        })
}

/// What version a tool reports, as the engine would hash it.
///
/// The third of the three accessors `ligase.steps` needs. D42 asks a
/// library step to declare its tool's version capture, and the engine's
/// probe table is a claim about what these programs actually print -- so
/// the test that holds it is a test against the installed programs, and it
/// needs to be able to ask.
///
/// # Errors
///
/// If the tool is not installed, prints nothing, or fails a probe that did
/// not expect to fail. The message says what to do about it.
#[pyfunction]
fn tool_version(tool: &str) -> PyResult<String> {
    exec::tool_version(tool).map_err(PyValueError::new_err)
}

/// Every resistance algorithm this engine ships, newest first.
#[pyfunction]
fn resistance_algorithms() -> Vec<String> {
    hivdb::ruleset::VENDORED
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect()
}

/// The default algorithm, used when a pipeline does not name one.
#[pyfunction]
fn default_resistance_algorithm() -> &'static str {
    hivdb::ruleset::DEFAULT
}

/// What the vendored algorithms are, by version, date and content.
///
/// This is what a step declaring `tool="hivdb"` records as its version,
/// so it is what puts the rules into the cache key.
#[pyfunction]
fn resistance_fingerprint() -> PyResult<String> {
    hivdb::ruleset::fingerprint().map_err(PyValueError::new_err)
}

/// `organism=` as Python may spell it: one name, or several.
///
/// The `String` variant stays first, and `str` is named as its own case,
/// because a Python `str` is also a sequence of one-character strings:
/// read as a list of names, `"HIV-1"` would be `["H", "I", "V", "-", "1"]`,
/// and every organism comparison after it would succeed or fail for the
/// wrong reason without anything crashing. PyO3 tries an untagged enum's
/// variants in declaration order, and (as the version in use does) also
/// refuses to read a `str` as a `Vec`; this does not lean on the second of
/// those.
#[derive(FromPyObject)]
enum Organisms {
    /// A single organism, as `organism="HIV-1"`.
    One(String),
    /// Several, as `organism=["HIV-1", "Homo sapiens"]`.
    Many(Vec<String>),
}

impl Organisms {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::One(name) => vec![name],
            Self::Many(names) => names,
        }
    }
}

/// What the loaders declare as the organisms, refusing a declaration that
/// names none.
///
/// The core also reads an empty or all-blank declaration as unknown, but a
/// user who wrote `organism=[]`, or `organism=config.get("organism", "")`,
/// has made a mistake worth naming here, where the message is read, rather
/// than finding a dimension unexpectedly unknown.
fn organisms(organism: Option<Organisms>) -> PyResult<Option<Vec<String>>> {
    let Some(names) = organism.map(Organisms::into_vec) else {
        return Ok(None);
    };
    if names.is_empty() {
        return Err(PyValueError::new_err(
            "organism was given an empty list; name at least one organism, or leave it out",
        ));
    }
    if names.iter().all(|name| name.trim().is_empty()) {
        return Err(PyValueError::new_err(
            "organism was given only blank names; name at least one organism, or leave it out",
        ));
    }
    Ok(Some(names))
}

/// What the loaders declare as the sources, refusing a declaration that
/// names none.
///
/// [`organisms`]'s rule, for the other set-valued axis and for the same
/// reason: a user who wrote `source_set=[]` has made a mistake worth naming
/// where the message is read. The core treats an empty declaration as
/// unknown too, but for a graph that would silently fall back to reading the
/// sources off the file, which is not what somebody passing a list meant.
fn sources(source_set: Option<Vec<String>>) -> PyResult<Option<Vec<String>>> {
    let Some(names) = source_set else {
        return Ok(None);
    };
    if names.is_empty() {
        return Err(PyValueError::new_err(
            "source_set was given an empty list; name at least one source, or leave it out",
        ));
    }
    if names.iter().all(|name| name.trim().is_empty()) {
        return Err(PyValueError::new_err(
            "source_set was given only blank names; name at least one source, or leave it out",
        ));
    }
    Ok(Some(names))
}

/// A declared label, refusing a blank one as [`organisms`] refuses blank names.
///
/// One helper for every scalar declaration — `release`, `subject`,
/// `identifier_namespace` — so there is one message template rather than
/// three that could drift apart. The core reads a blank as unknown too, but
/// a user who wrote `release=""`, or `subject=config.get("subject", "")`,
/// has made a mistake worth naming here, where the message is read, rather
/// than finding the dimension unexpectedly unknown later.
fn declared_label(name: &str, value: Option<String>) -> PyResult<Option<String>> {
    match value {
        Some(label) if label.trim().is_empty() => Err(PyValueError::new_err(format!(
            "{name} was given a blank value; name the {name}, or leave it out"
        ))),
        label => Ok(label),
    }
}

/// Resolve a recorded query, and load its body as `kind`.
///
/// A query's source promises nothing, so what makes this reproducible is
/// that the canonicalised response body is in the content-addressed store.
/// `mode` is the per-run setting -- `auto`, `replay` or `refresh` -- and a
/// `replay` with no record is an error here, while the pipeline is still
/// being read, which is what makes "it fails before any step runs" true
/// rather than merely intended.
///
/// `auth_env` names an environment variable holding a bearer token. The
/// name rather than the value, because a bundle carries the pipeline file
/// and a token written in the pipeline would travel in every bundle.
#[pyfunction]
#[pyo3(signature = (locator, kind, mode, cache=None, auth_env=None, organism=None, build=None, frame=None, release=None, samples=None, namespace=None, subject=None, source_set=None, representation=None))]
#[expect(
    clippy::too_many_arguments,
    reason = "every declarable dimension, plus the three a query needs; \
              the Python surface names them and nothing calls this twice"
)]
fn resolve_query(
    locator: &str,
    kind: &str,
    mode: &str,
    cache: Option<PathBuf>,
    auth_env: Option<&str>,
    organism: Option<Organisms>,
    build: Option<String>,
    frame: Option<(String, u64)>,
    release: Option<String>,
    samples: Option<Vec<String>>,
    namespace: Option<String>,
    subject: Option<String>,
    source_set: Option<Vec<String>>,
    representation: Option<String>,
) -> PyResult<PyDataset> {
    let parsed =
        Locator::parse(locator).map_err(|error| RemoteError::new_err(error.to_string()))?;
    let wanted = kind_named(kind)?;
    let mode = match mode {
        "auto" => Mode::Auto,
        "replay" => Mode::Replay,
        "refresh" => Mode::Refresh,
        other => {
            return Err(PyValueError::new_err(format!(
                "`{other}` is not a mode. The modes are auto, replay and refresh"
            )));
        }
    };

    // Read here rather than carried from Python, so the token is in one
    // process for one call and never in a default argument, a repr or a
    // traceback.
    let auth = match auth_env {
        None => None,
        Some(name) => {
            let token = std::env::var(name).map_err(|_| {
                RemoteError::new_err(format!(
                    "`{locator}` is to be authenticated from the environment \
                     variable `{name}`, and that variable is not set. Set it, \
                     or drop `auth_env=` if the query needs no token -- an \
                     unauthenticated request would be answered with a 401, or \
                     worse with a smaller public subset of the data recorded \
                     as though it were the answer"
                ))
            })?;
            Some(Auth::bearer(&token))
        }
    };

    let root = cache.unwrap_or_else(|| PathBuf::from(".ligase"));
    let cache = Cache::open(&root).map_err(|error| RemoteError::new_err(error.to_string()))?;

    // Capped at the recorded limit, so an answer too large to record is
    // refused without being read to its end.
    let network = Network::new(Policy {
        max_bytes: MAX_RECORDED_BYTES,
        ..Policy::default()
    });
    // `replay` must not be able to reach the network at all: a mode that
    // merely declined to use it would still deadlock a mock, and would rest
    // the guarantee on a branch rather than on there being nothing to
    // branch to.
    let transport: &dyn Transport = if mode == Mode::Replay {
        &Offline
    } else {
        &network
    };
    let resolved = Python::attach(|py| {
        py.detach(|| recorded::resolve(&parsed, wanted, auth, mode, &cache, transport))
    })
    .map_err(|error| RemoteError::new_err(error.to_string()))?;

    let declared = Declared {
        organism: organisms(organism)?,
        build,
        frame,
        samples,
        release: declared_label("release", release)?,
        identifier_namespace: declared_label("namespace", namespace)?,
        subject: declared_label("subject", subject)?,
        source_set: sources(source_set)?,
        representation: declared_representation(representation)?,
    };

    // Loaded from the store exactly as a local file is, so a recorded
    // query's dimensions are inferred by the same code and cannot disagree
    // with it.
    let primary = resolved.path.as_path();
    let loaded = match wanted {
        DataKind::Reference => load::reference(primary, &declared),
        DataKind::Alignment => load::alignment(primary, &declared),
        DataKind::Variants => load::variants(primary, &declared),
        DataKind::Intervals => load::intervals(primary, None, &declared),
        DataKind::EntitySet => load::entity_set(primary, None, &declared),
        DataKind::RankedList => load::ranked_list(primary, None, &declared),
        DataKind::Reads => load::reads(primary, None, &declared),
        DataKind::KnowledgeGraph => load::knowledge_graph(primary, &declared),
        DataKind::MoleculeSet => load::molecule_set(primary, &declared),
        DataKind::MoleculeRanking => load::molecule_ranking(primary, &declared),
        DataKind::ResistanceReport => {
            return Err(PyValueError::new_err(
                "a resistance report is something Ligase produces, not \
                 something to query for",
            ));
        }
    }
    .map_err(to_py)?;

    let mut inner = loaded.with_origin(resolved.origin());
    if !resolved.notes.is_empty() {
        let mut notes = inner.notes().to_vec();
        notes.extend(resolved.notes.iter().cloned());
        inner = inner.with_notes(notes);
    }
    Ok(PyDataset { inner })
}

/// Put a bundle's recorded responses into a cache, so the run replays.
///
/// Called before the pipeline is imported, because that is when a query
/// resolves. Returns how many records were added.
#[pyfunction]
#[pyo3(signature = (bundle, cache=None))]
fn seed_records(bundle: PathBuf, cache: Option<PathBuf>) -> PyResult<usize> {
    let root = cache.unwrap_or_else(|| PathBuf::from(".ligase"));
    let cache = Cache::open(&root).map_err(|error| RemoteError::new_err(error.to_string()))?;
    prov::bundle::seed(&bundle, &cache).map_err(|error| RemoteError::new_err(error.to_string()))
}

/// The kind this name stands for, or an error naming the kinds there are.
///
/// One place rather than two, so `lg.remote` and `lg.query` cannot come to
/// disagree about what a kind is called.
fn kind_named(kind: &str) -> PyResult<DataKind> {
    DataKind::ALL
        .iter()
        .copied()
        .find(|known| known.name() == kind)
        .ok_or_else(|| {
            PyValueError::new_err(format!(
                "`{kind}` is not a kind of data Ligase knows. The kinds are {}",
                DataKind::ALL
                    .iter()
                    .map(|known| known.name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
}

/// Resolve a pinned remote, and load what arrives as `kind`.
///
/// The digest is over what the file *means*, so a source that reflows a
/// FASTA does not break the pin. A digest that does not match is an error
/// here, while the pipeline is still being read, which is what makes "it
/// fails before any step runs" true rather than merely intended.
#[pyfunction]
#[pyo3(signature = (locator, kind, digest=None, cache=None, offline=false, organism=None, build=None, frame=None, release=None, samples=None, namespace=None, subject=None, source_set=None, representation=None))]
#[expect(
    clippy::too_many_arguments,
    reason = "every declarable dimension, plus the three a remote needs; \
              the Python surface names them and nothing calls this twice"
)]
fn resolve_remote(
    locator: &str,
    kind: &str,
    digest: Option<&str>,
    cache: Option<PathBuf>,
    offline: bool,
    organism: Option<Organisms>,
    build: Option<String>,
    frame: Option<(String, u64)>,
    release: Option<String>,
    samples: Option<Vec<String>>,
    namespace: Option<String>,
    subject: Option<String>,
    source_set: Option<Vec<String>>,
    representation: Option<String>,
) -> PyResult<PyDataset> {
    let parsed =
        Locator::parse(locator).map_err(|error| RemoteError::new_err(error.to_string()))?;
    let wanted = kind_named(kind)?;
    let declared_digest = match digest {
        Some(text) => Some(ContentHash::parse(text.trim()).ok_or_else(|| {
            PyValueError::new_err(format!(
                "`{text}` is not a digest: a digest is the 64 hexadecimal \
                 characters `ligase run` reports for a remote it fetched"
            ))
        })?),
        None => None,
    };

    let root = cache.unwrap_or_else(|| PathBuf::from(".ligase"));
    let cache = Cache::open(&root).map_err(|error| RemoteError::new_err(error.to_string()))?;

    // The GIL is released for the fetch, which can take minutes on an SRA
    // run. Holding it would stop every other Python thread for the
    // duration -- including a progress display, a signal handler, or a
    // local server a test is fetching from, which is a deadlock rather
    // than a slowdown.
    let network = Network::new(Policy::default());
    let transport: &dyn Transport = if offline { &Offline } else { &network };
    let resolved = Python::attach(|py| {
        py.detach(|| pinned::resolve(&parsed, wanted, declared_digest, &cache, transport))
    })
    .map_err(|error| RemoteError::new_err(error.to_string()))?;

    // A local FASTQ's sample is taken from its filename, and a store object's
    // filename is the hash of its bytes -- so reads fetched from a remote
    // have no filename to read a sample from, and the engine must be told
    // rather than take a hash for a sample identifier. Not defaulted to the
    // run accession either: one sample can have several runs, so a run is
    // not a sample and quietly treating it as one is the kind of wrong
    // answer this engine exists to refuse.
    if wanted == DataKind::Reads && samples.as_ref().is_none_or(Vec::is_empty) {
        return Err(PyValueError::new_err(format!(
            "`{locator}` is reads, so it needs `samples=` naming the sample \
             they are from. A local FASTQ's sample comes from its filename, \
             and a fetched one is stored under the hash of its bytes, which \
             is no sample identifier at all. The run accession is usually \
             what you want -- `samples=[\"{}\"]` -- but say so, because one \
             sample can have several runs",
            parsed.body()
        )));
    }

    let declared = Declared {
        organism: organisms(organism)?,
        build,
        frame,
        samples,
        release: declared_label("release", release)?,
        identifier_namespace: declared_label("namespace", namespace)?,
        subject: declared_label("subject", subject)?,
        source_set: sources(source_set)?,
        representation: declared_representation(representation)?,
    };

    // Loaded from the store exactly as a local file is, so a remote's
    // dimensions are inferred by the same code and cannot disagree with it.
    let mates: Vec<PathBuf> = resolved.paths.iter().skip(1).cloned().collect();
    let primary = resolved
        .paths
        .first()
        .ok_or_else(|| RemoteError::new_err(format!("`{locator}` resolved to no files")))?;
    let loaded = match wanted {
        DataKind::Reference => load::reference(primary, &declared),
        DataKind::Alignment => load::alignment(primary, &declared),
        DataKind::Variants => load::variants(primary, &declared),
        // The format is read off the file's own suffix, which the locator
        // settled before anything was fetched.
        DataKind::Intervals => load::intervals(primary, None, &declared),
        DataKind::EntitySet => load::entity_set(primary, None, &declared),
        DataKind::RankedList => load::ranked_list(primary, None, &declared),
        DataKind::Reads => load::reads(primary, mates.first().map(PathBuf::as_path), &declared),
        DataKind::KnowledgeGraph => load::knowledge_graph(primary, &declared),
        DataKind::MoleculeSet => load::molecule_set(primary, &declared),
        DataKind::MoleculeRanking => load::molecule_ranking(primary, &declared),
        DataKind::ResistanceReport => {
            return Err(PyValueError::new_err(
                "a resistance report is something Ligase produces, not \
                 something to fetch",
            ));
        }
    }
    .map_err(to_py)?;

    let mut inner = loaded.with_origin(resolved.origin());
    if !resolved.notes.is_empty() {
        let mut notes = inner.notes().to_vec();
        notes.extend(resolved.notes.iter().cloned());
        inner = inner.with_notes(notes);
    }
    Ok(PyDataset { inner })
}

#[pyfunction]
#[pyo3(signature = (path, organism=None, build=None, frame=None, release=None))]
fn load_reference(
    path: PathBuf,
    organism: Option<Organisms>,
    build: Option<String>,
    frame: Option<(String, u64)>,
    release: Option<String>,
) -> PyResult<PyDataset> {
    let declared = Declared {
        organism: organisms(organism)?,
        build,
        frame,
        samples: None,
        release: declared_label("release", release)?,
        identifier_namespace: None,
        subject: None,
        source_set: None,
        representation: None,
    };
    load::reference(&path, &declared)
        .map(|inner| PyDataset { inner })
        .map_err(to_py)
}

#[pyfunction]
#[pyo3(signature = (path, organism=None, build=None, samples=None))]
fn load_alignment(
    path: PathBuf,
    organism: Option<Organisms>,
    build: Option<String>,
    samples: Option<Vec<String>>,
) -> PyResult<PyDataset> {
    let declared = Declared {
        organism: organisms(organism)?,
        build,
        frame: None,
        samples,
        release: None,
        identifier_namespace: None,
        subject: None,
        source_set: None,
        representation: None,
    };
    load::alignment(&path, &declared)
        .map(|inner| PyDataset { inner })
        .map_err(to_py)
}

#[pyfunction]
#[pyo3(signature = (path, organism=None, build=None, samples=None))]
fn load_variants(
    path: PathBuf,
    organism: Option<Organisms>,
    build: Option<String>,
    samples: Option<Vec<String>>,
) -> PyResult<PyDataset> {
    let declared = Declared {
        organism: organisms(organism)?,
        build,
        frame: None,
        samples,
        release: None,
        identifier_namespace: None,
        subject: None,
        source_set: None,
        representation: None,
    };
    load::variants(&path, &declared)
        .map(|inner| PyDataset { inner })
        .map_err(to_py)
}

#[pyfunction]
#[pyo3(signature = (r1, r2=None, organism=None, samples=None))]
fn load_reads(
    r1: PathBuf,
    r2: Option<PathBuf>,
    organism: Option<Organisms>,
    samples: Option<Vec<String>>,
) -> PyResult<PyDataset> {
    let declared = Declared {
        organism: organisms(organism)?,
        build: None,
        samples,
        frame: None,
        release: None,
        identifier_namespace: None,
        subject: None,
        source_set: None,
        representation: None,
    };
    load::reads(&r1, r2.as_deref(), &declared)
        .map(|inner| PyDataset { inner })
        .map_err(to_py)
}

#[pyfunction]
#[pyo3(signature = (path, format=None, organism=None, build=None))]
fn load_intervals(
    path: PathBuf,
    format: Option<&str>,
    organism: Option<Organisms>,
    build: Option<String>,
) -> PyResult<PyDataset> {
    let format = match format {
        Some(name) => Some(IntervalFormat::from_name(name).ok_or_else(|| {
            PyValueError::new_err(format!(
                "`{name}` is not an interval format; expected bed, gff, gff3 or gtf"
            ))
        })?),
        None => None,
    };
    let declared = Declared {
        organism: organisms(organism)?,
        build,
        frame: None,
        samples: None,
        release: None,
        identifier_namespace: None,
        subject: None,
        source_set: None,
        representation: None,
    };
    load::intervals(&path, format, &declared)
        .map(|inner| PyDataset { inner })
        .map_err(to_py)
}

/// Load a gene or protein list from a TSV with a header.
#[pyfunction]
#[pyo3(signature = (path, id_column=None, organism=None, identifier_namespace=None, subject=None, release=None))]
fn load_entity_set(
    path: PathBuf,
    id_column: Option<&str>,
    organism: Option<Organisms>,
    identifier_namespace: Option<String>,
    subject: Option<String>,
    release: Option<String>,
) -> PyResult<PyDataset> {
    let declared = Declared {
        organism: organisms(organism)?,
        build: None,
        frame: None,
        samples: None,
        release: declared_label("release", release)?,
        identifier_namespace: declared_label("identifier_namespace", identifier_namespace)?,
        subject: declared_label("subject", subject)?,
        source_set: None,
        representation: None,
    };
    load::entity_set(&path, id_column, &declared)
        .map(|inner| PyDataset { inner })
        .map_err(to_py)
}

/// Load a ranked list from a TSV with a header, highest rank first.
#[pyfunction]
#[pyo3(signature = (path, id_column=None, organism=None, identifier_namespace=None, subject=None, release=None))]
fn load_ranked_list(
    path: PathBuf,
    id_column: Option<&str>,
    organism: Option<Organisms>,
    identifier_namespace: Option<String>,
    subject: Option<String>,
    release: Option<String>,
) -> PyResult<PyDataset> {
    let declared = Declared {
        organism: organisms(organism)?,
        build: None,
        frame: None,
        samples: None,
        release: declared_label("release", release)?,
        identifier_namespace: declared_label("identifier_namespace", identifier_namespace)?,
        subject: declared_label("subject", subject)?,
        source_set: None,
        representation: None,
    };
    load::ranked_list(&path, id_column, &declared)
        .map(|inner| PyDataset { inner })
        .map_err(to_py)
}

/// Load a knowledge graph from JSON.
#[pyfunction]
#[pyo3(signature = (path, organism=None, identifier_namespace=None, subject=None, release=None, source_set=None))]
fn load_knowledge_graph(
    path: PathBuf,
    organism: Option<Organisms>,
    identifier_namespace: Option<String>,
    subject: Option<String>,
    release: Option<String>,
    source_set: Option<Vec<String>>,
) -> PyResult<PyDataset> {
    let declared = Declared {
        organism: organisms(organism)?,
        build: None,
        frame: None,
        samples: None,
        release: declared_label("release", release)?,
        identifier_namespace: declared_label("identifier_namespace", identifier_namespace)?,
        subject: declared_label("subject", subject)?,
        source_set: sources(source_set)?,
        representation: None,
    };
    load::knowledge_graph(&path, &declared)
        .map(|inner| PyDataset { inner })
        .map_err(to_py)
}

/// Load a set of molecules from a tab-separated table.
#[pyfunction]
#[pyo3(signature = (path, organism=None, representation=None, subject=None, release=None))]
fn load_molecule_set(
    path: PathBuf,
    organism: Option<Organisms>,
    representation: Option<String>,
    subject: Option<String>,
    release: Option<String>,
) -> PyResult<PyDataset> {
    load::molecule_set(
        &path,
        &molecules_declared(organism, representation, subject, release)?,
    )
    .map(|inner| PyDataset { inner })
    .map_err(to_py)
}

/// Load a ranking of molecules, whose row order is the answer.
#[pyfunction]
#[pyo3(signature = (path, organism=None, representation=None, subject=None, release=None))]
fn load_molecule_ranking(
    path: PathBuf,
    organism: Option<Organisms>,
    representation: Option<String>,
    subject: Option<String>,
    release: Option<String>,
) -> PyResult<PyDataset> {
    load::molecule_ranking(
        &path,
        &molecules_declared(organism, representation, subject, release)?,
    )
    .map(|inner| PyDataset { inner })
    .map_err(to_py)
}

/// Read a representation, and give back the one spelling for it.
///
/// Exported so that `ligase.chem` has the *same* validator the loaders and
/// the conversion registry use, rather than a second reading of the same
/// string in Python. It also canonicalises: the operations come back
/// deduplicated and in one order, so the line a standardiser writes is the
/// line its own loader reads back to the same value.
#[pyfunction]
fn parse_representation(spelled: &str) -> PyResult<String> {
    Representation::parse(spelled)
        .map(|representation| representation.spell())
        .map_err(PyValueError::new_err)
}

/// A declared representation, checked before anything is loaded.
///
/// Parsed here rather than only in the loader so that a spelling the engine
/// cannot read is a `ValueError` naming what it does know, instead of a
/// declaration silently ignored and a dimension mysteriously unknown. The
/// loader is deliberately lenient about the same thing, because a *file*
/// Ligase did not write is not the user's mistake to be shouted at for; a
/// declaration is.
fn declared_representation(declared: Option<String>) -> PyResult<Option<String>> {
    let declared = declared_label("representation", declared)?;
    if let Some(spelled) = declared.as_deref() {
        Representation::parse(spelled).map_err(PyValueError::new_err)?;
    }
    Ok(declared)
}

/// What a molecule loader was told, checked before it is handed over.
fn molecules_declared(
    organism: Option<Organisms>,
    representation: Option<String>,
    subject: Option<String>,
    release: Option<String>,
) -> PyResult<Declared> {
    let representation = declared_representation(representation)?;
    Ok(Declared {
        organism: organisms(organism)?,
        build: None,
        frame: None,
        samples: None,
        release: declared_label("release", release)?,
        identifier_namespace: None,
        subject: declared_label("subject", subject)?,
        source_set: None,
        representation,
    })
}

#[pyfunction]
#[pyo3(signature = (paths, paired, organism=None))]
fn load_samples(
    paths: Vec<PathBuf>,
    paired: bool,
    organism: Option<Organisms>,
) -> PyResult<Vec<PyDataset>> {
    let declared = Declared {
        organism: organisms(organism)?,
        build: None,
        samples: None,
        frame: None,
        release: None,
        identifier_namespace: None,
        subject: None,
        source_set: None,
        representation: None,
    };
    load::samples(&paths, paired, &declared)
        .map(|datasets| {
            datasets
                .into_iter()
                .map(|inner| PyDataset { inner })
                .collect()
        })
        .map_err(to_py)
}

/// Check a step's inputs, returning one [`PyCheckError`] per disagreement.
#[pyfunction]
fn check_step(step: &str, bindings: Vec<(String, PyRef<'_, PyDataset>)>) -> Vec<PyCheckError> {
    let bindings: Vec<Binding> = bindings
        .into_iter()
        .map(|(name, dataset)| Binding {
            name,
            dataset: dataset.inner.clone(),
        })
        .collect();

    core_check_step(step, &bindings)
        .iter()
        .map(translate)
        .collect()
}

/// The dimensions two or more of these datasets state a value for.
///
/// What a verdict over several files covers, for `ligase inspect`. Ordered
/// by the engine so that a report reads in the catalogue's own order rather
/// than in the order files happened to be given.
#[pyfunction]
fn comparable(datasets: Vec<PyRef<'_, PyDataset>>) -> Vec<&'static str> {
    let datasets: Vec<Dataset> = datasets
        .into_iter()
        .map(|dataset| dataset.inner.clone())
        .collect();
    inspect::comparable(&datasets)
}

/// A dependency graph, built from signatures and never executed.
#[pyclass(name = "Graph", module = "ligase._core")]
struct PyGraph {
    inner: Graph,
}

#[pymethods]
impl PyGraph {
    #[new]
    fn new() -> Self {
        Self {
            inner: Graph::new(),
        }
    }

    /// Add a dataset that was loaded rather than computed.
    fn add_input(&mut self, label: &str, dataset: PyRef<'_, PyDataset>) -> usize {
        self.inner.add_input(label, dataset.inner.clone()).index()
    }

    /// Add a step over nodes that already exist, propagating its output.
    ///
    /// `conversion` is the registered conversion this step performs and the
    /// target its call declared, as `(name, to)`. It is resolved against
    /// the registry here rather than on the Python side so that the two
    /// surfaces cannot disagree about which conversions exist.
    #[pyo3(signature = (name, output, arguments, coordinates=None, conversion=None))]
    fn add_step(
        &mut self,
        name: &str,
        output: &str,
        arguments: Vec<PyArgument>,
        coordinates: Option<&str>,
        conversion: Option<(String, String)>,
    ) -> PyResult<usize> {
        let output = kind(output)?;
        let coordinates = match coordinates {
            Some(given) => Some(CoordinateSystem::from_name(given).ok_or_else(|| {
                let known: Vec<&str> = CoordinateSystem::ALL
                    .iter()
                    .map(|system| system.describe())
                    .collect();
                PyValueError::new_err(format!(
                    "`{given}` is not a coordinate system; expected {}",
                    known.join(" or ")
                ))
            })?),
            None => None,
        };

        let conversion = match conversion {
            Some((registered, to)) => {
                let found = Conversion::by_name(&registered)
                    .ok_or_else(|| PyValueError::new_err(unregistered(&registered)))?;
                Some(Converted {
                    conversion: found,
                    target: found.target(&to).map_err(PyValueError::new_err)?,
                })
            }
            None => None,
        };

        let arguments = arguments
            .into_iter()
            .map(|(param, declared, node)| {
                Ok(Argument {
                    param,
                    declared: kind(&declared)?,
                    node: NodeId::from_index(node),
                })
            })
            .collect::<PyResult<Vec<Argument>>>()?;

        self.inner
            .add_step(StepSpec {
                name: name.to_owned(),
                output,
                coordinates,
                conversion,
                arguments,
            })
            .map(NodeId::index)
            .map_err(|error| PyValueError::new_err(error.to_string()))
    }

    /// The dataset a node holds or produces, with its propagated dimensions.
    fn dataset(&self, node: usize) -> Option<PyDataset> {
        self.inner
            .dataset(NodeId::from_index(node))
            .map(|inner| PyDataset {
                inner: inner.clone(),
            })
    }

    /// The steps these outputs depend on, in dependency order.
    fn step_names(&self, roots: Vec<usize>) -> Vec<String> {
        self.inner
            .step_names(&nodes(&roots))
            .into_iter()
            .map(ToOwned::to_owned)
            .collect()
    }

    /// Check every step these outputs depend on.
    ///
    /// Returns every error, the one-line summary and the whole report as
    /// text, all three worded in the core so the Rust tests own them. The
    /// report states each distinct problem once and names every sample it
    /// affects; the error list keeps one entry per failing node.
    fn check(&self, roots: Vec<usize>) -> (Vec<PyCheckError>, String, String) {
        let errors = self.inner.check(&nodes(&roots));
        let summary = summarise(&errors);
        let report = render(&errors);
        (errors.iter().map(translate).collect(), summary, report)
    }

    /// Run every step these outputs depend on.
    ///
    /// The GIL is released for the whole run and taken again only for the
    /// steps whose bodies are Python callbacks, so a cohort of shell steps
    /// runs at full width.
    ///
    /// Returns every finished step, every failure, the summary line, the
    /// whole report as text, and how many input files had to be hashed
    /// against how many the prefilter already knew.
    #[pyo3(signature = (roots, tasks, cache_dir, callbacks, jobs=None, force=false))]
    fn run(
        &self,
        roots: Vec<usize>,
        tasks: Vec<PyTask>,
        cache_dir: PathBuf,
        callbacks: Vec<Py<PyAny>>,
        jobs: Option<usize>,
        force: bool,
    ) -> PyResult<PyRunReport> {
        let roots = nodes(&roots);
        let tasks = recipes(tasks)?;

        let cache =
            Cache::open(&cache_dir).map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let prov =
            Prov::open(&cache_dir).map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let options = Options {
            jobs: jobs.unwrap_or_else(|| Options::default().jobs),
            force,
        };
        let host = PyHost { callbacks };

        let report = Python::attach(|py| {
            py.detach(|| {
                exec::run(
                    &self.inner,
                    &roots,
                    &tasks,
                    &Stores {
                        cache: &cache,
                        prov: &prov,
                    },
                    &options,
                    &host,
                )
            })
        });

        let summary = exec::summarise(&report);
        let text = exec::render(&self.inner, &roots, &report);
        let stats = cache.stats();

        Ok((
            report
                .results
                .iter()
                .map(|result| {
                    (
                        result.node.index(),
                        result.step.clone(),
                        result.outcome.to_string(),
                        result.key.to_hex(),
                        result.hash.to_hex(),
                        result.canonical.to_hex(),
                        result.output.to_string_lossy().into_owned(),
                        u64::try_from(result.duration.as_millis()).unwrap_or(u64::MAX),
                        result.notes.iter().map(ToString::to_string).collect(),
                    )
                })
                .collect(),
            report.failures.iter().map(failed).collect(),
            summary,
            text,
            (stats.hashed, stats.prefiltered, stats.canonicalised),
        ))
    }

    /// Run one step twice and report whether it is reproducible.
    ///
    /// The pipeline is run first so that the step has its inputs; the two
    /// audit runs write into scratch files and are recorded nowhere.
    #[pyo3(signature = (roots, tasks, cache_dir, callbacks, subject, jobs=None))]
    fn audit(
        &self,
        roots: Vec<usize>,
        tasks: Vec<PyTask>,
        cache_dir: PathBuf,
        callbacks: Vec<Py<PyAny>>,
        subject: PySubject,
        jobs: Option<usize>,
    ) -> PyResult<PyAuditReport> {
        let (step, sample) = &subject;
        let roots = nodes(&roots);
        let tasks = recipes(tasks)?;

        let cache =
            Cache::open(&cache_dir).map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let prov =
            Prov::open(&cache_dir).map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let options = Options {
            jobs: jobs.unwrap_or_else(|| Options::default().jobs),
            force: false,
        };
        let host = PyHost { callbacks };

        let report = Python::attach(|py| {
            py.detach(|| {
                audit::audit(
                    &self.inner,
                    &roots,
                    &tasks,
                    &Stores {
                        cache: &cache,
                        prov: &prov,
                    },
                    &options,
                    &host,
                    &audit::Subject {
                        step,
                        sample: sample.as_deref(),
                    },
                )
            })
        })
        .map_err(PyValueError::new_err)?;

        Ok((
            report
                .audited
                .iter()
                .map(|audited| {
                    (
                        audited.node.index(),
                        audited.step.clone(),
                        audited.scope.clone(),
                        audited.verdict.to_string(),
                        audited.first.to_hex(),
                        audited.second.to_hex(),
                        audited.first_canonical.to_hex(),
                        audited.second_canonical.to_hex(),
                        u64::try_from(audited.first_duration.as_millis()).unwrap_or(u64::MAX),
                        u64::try_from(audited.second_duration.as_millis()).unwrap_or(u64::MAX),
                        audited.seed.clone(),
                    )
                })
                .collect(),
            report.failures.iter().map(failed).collect(),
            report.verdict().to_string(),
            report.summary(),
            report.text(),
        ))
    }

    /// Run the pipeline, then say how its results were produced.
    ///
    /// The run comes first because a provenance record is filed under a
    /// step's key, and computing keys is what a run does. It is an ordinary
    /// run, so a pipeline already computed costs a cache lookup per step.
    ///
    /// `bundle` writes a reproducibility bundle; `verify` checks one against
    /// what this machine just produced. Both are optional and independent.
    #[pyo3(signature = (roots, tasks, cache_dir, callbacks, write_up, jobs=None))]
    fn methods(
        &self,
        roots: Vec<usize>,
        tasks: Vec<PyTask>,
        cache_dir: PathBuf,
        callbacks: Vec<Py<PyAny>>,
        write_up: PyWriteUp,
        jobs: Option<usize>,
    ) -> PyResult<(PyMethods, Option<PyBundle>, Option<PyVerification>)> {
        let (bundle, verify, include, thin, claims) = &write_up;
        let roots = nodes(&roots);
        let tasks = recipes(tasks)?;

        let cache =
            Cache::open(&cache_dir).map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let prov =
            Prov::open(&cache_dir).map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        let options = Options {
            jobs: jobs.unwrap_or_else(|| Options::default().jobs),
            force: false,
        };
        let host = PyHost { callbacks };

        let report = Python::attach(|py| {
            py.detach(|| {
                exec::run(
                    &self.inner,
                    &roots,
                    &tasks,
                    &Stores {
                        cache: &cache,
                        prov: &prov,
                    },
                    &options,
                    &host,
                )
            })
        });

        // A failed step has produced nothing to account for, and saying how
        // the rest were produced would read as a complete methods section
        // for a pipeline that did not finish.
        if let Some(failure) = report.failures.first() {
            return Err(PyRuntimeError::new_err(format!(
                "`{}` failed, so there is nothing to write up: {}",
                failure.step, failure.message
            )));
        }

        let mut methods = prov::methods(&self.inner, &roots, &tasks, &report, &prov)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
        // Before the bundle is written, so the paper's claims travel in it.
        methods.bind(&declared(claims)?);

        let written = match &bundle {
            None => None,
            Some(into) => {
                let summary = prov::bundle::write(
                    into,
                    &prov::bundle::Request {
                        graph: &self.inner,
                        roots: &roots,
                        report: &report,
                        prov: &prov,
                        methods: &methods,
                        include,
                        // A recorded query's body travels in the bundle, so
                        // a reviewer with no network and no API key can
                        // replay the run (D39).
                        queries: Some(&cache),
                        thin: *thin,
                    },
                )
                .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
                Some((
                    summary.root.to_string_lossy().into_owned(),
                    summary.files,
                    summary.steps,
                    summary.gaps,
                ))
            }
        };

        let checked = match &verify {
            None => None,
            Some(against) => {
                let verification = prov::bundle::verify(against, &self.inner, &roots, &report)
                    .map_err(|error| PyValueError::new_err(error.to_string()))?;
                Some((
                    verification
                        .checks
                        .iter()
                        .map(|check| {
                            (
                                check.step.clone(),
                                check.scope.clone(),
                                check.verdict.to_string(),
                                check.expected.to_hex(),
                                check.found.map(|hash| hash.to_hex()),
                                check.same_key,
                            )
                        })
                        .collect(),
                    verification
                        .inputs
                        .iter()
                        .map(|input| {
                            (
                                input.step.clone(),
                                input.param.clone(),
                                input.path.to_string_lossy().into_owned(),
                                input.presence.to_string(),
                            )
                        })
                        .collect(),
                    verification.reproduced(),
                    verification.verdict().to_string(),
                    verification.summary(),
                    verification.text(),
                ))
            }
        };

        let (cited, uncited) = methods.citations();
        Ok((
            (
                methods
                    .groups
                    .iter()
                    .map(|group| {
                        (
                            group.step.clone(),
                            group.nodes.len(),
                            group.scopes.clone(),
                            group.scope_kind.map(str::to_owned),
                            group.record.tools.clone(),
                            group.record.parameters.clone(),
                            group.template.clone(),
                        )
                    })
                    .collect(),
                methods
                    .gaps
                    .iter()
                    .map(|gap| (gap.step.clone(), gap.scopes.clone(), gap.nodes))
                    .collect(),
                methods.references().iter().map(reference).collect(),
                cited,
                uncited,
                methods.claims.iter().map(claim_check).collect(),
                methods.engine.clone(),
                methods.platform.clone(),
                methods.summary(),
                methods.text(),
            ),
            written,
            checked,
        ))
    }

    fn __len__(&self) -> usize {
        self.inner.len()
    }
}

/// Every task's recipe, as the core wants them.
fn recipes(tasks: Vec<PyTask>) -> PyResult<Vec<Task>> {
    tasks
        .into_iter()
        .map(
            |(node, code, command, callback, tools, parameters, citations, seed)| {
                let recipe = match (command, callback) {
                    (Some(command), _) => Recipe::Shell(command),
                    (None, Some(id)) => Recipe::Callback(id),
                    (None, None) => {
                        return Err(PyValueError::new_err(
                            "a step needs either a command or a callback to produce anything",
                        ));
                    }
                };
                Ok(Task {
                    node: NodeId::from_index(node),
                    code,
                    recipe,
                    tools,
                    parameters,
                    citations,
                    seed,
                })
            },
        )
        .collect()
}

/// One step that did not produce anything.
/// One pinned reference, as Python sees it.
fn reference(pinned: &prov::ReferenceRecord) -> PyReference {
    (
        pinned.param.clone(),
        pinned.path.to_string_lossy().into_owned(),
        pinned.build.clone(),
        pinned.accession.clone(),
        pinned.contigs,
        pinned.length,
        pinned.checksum.clone(),
        pinned.fingerprint.clone(),
    )
}

/// The claims as the core wants them.
///
/// The flattening Python does is undone here: `parameter=` and `tool=` arrive
/// as two optional names and become one [`prov::Restated`], which is a type
/// that cannot hold both. A pair that named both would already have been
/// refused by `Claim.__post_init__`, so reaching this with both set means the
/// surface let something through — and that is an error rather than a guess
/// about which the user meant.
fn declared(claims: &[PyClaim]) -> PyResult<Vec<prov::Claim>> {
    claims
        .iter()
        .cloned()
        .map(|(text, step, scope, parameter, tool, value)| {
            let restates = match (parameter, tool, value) {
                (None, None, None) => None,
                (Some(name), None, Some(value)) => Some(prov::Restated::Parameter { name, value }),
                (None, Some(name), Some(version)) => Some(prov::Restated::Tool { name, version }),
                _ => {
                    return Err(PyValueError::new_err(format!(
                        "the claim {text:?} restates a value the engine cannot \
                         read: name exactly one of `parameter=` and `tool=`, \
                         and give `value=` beside it"
                    )));
                }
            };
            Ok(prov::Claim {
                text,
                step,
                scope,
                restates,
            })
        })
        .collect()
}

/// One checked claim, as Python sees it.
///
/// Named for the type rather than the verb, because `checked` is what the
/// verification half of `methods` already calls its own result.
fn claim_check(check: &prov::ClaimCheck) -> PyClaimCheck {
    let (parameter, tool, value) = match &check.claim.restates {
        None => (None, None, None),
        Some(prov::Restated::Parameter { name, value }) => {
            (Some(name.clone()), None, Some(value.clone()))
        }
        Some(prov::Restated::Tool { name, version }) => {
            (None, Some(name.clone()), Some(version.clone()))
        }
    };
    let recorded = match &check.verdict {
        prov::ClaimVerdict::Contradicted { recorded, .. } => Some(recorded.clone()),
        _ => None,
    };

    (
        (
            check.claim.text.clone(),
            check.claim.step.clone(),
            check.claim.scope.clone(),
            parameter,
            tool,
            value,
        ),
        check.verdict.name().to_owned(),
        // The sentence the write-up prints, so a script reporting one claim
        // says what `ligase methods` says about it.
        Some(check.sentence()),
        recorded,
        check
            .bound
            .iter()
            .map(|bound| {
                (
                    bound.scope.clone(),
                    bound.key.to_hex(),
                    bound.canonical.to_hex(),
                    bound.recorded_at,
                    bound.platform.clone(),
                )
            })
            .collect(),
    )
}

fn failed(failure: &exec::StepFailure) -> PyRunFailure {
    (
        failure.node.index(),
        failure.step.clone(),
        failure.message.clone(),
    )
}

fn nodes(indices: &[usize]) -> Vec<NodeId> {
    indices.iter().copied().map(NodeId::from_index).collect()
}

/// How many objects `ligase store status` will ask the remote about.
///
/// Above this it samples and says so. One `has` is one round trip, so an
/// exact answer on a lab-sized store costs minutes, and a number a user
/// reads as exact when it is not is its own kind of wrong answer.
const SURVEY_BOUND: usize = 1000;

/// What `ligase store status` reports, in order:
/// whether this build can open a bucket, the store URL, why the store did
/// not answer, local objects, local index rows, how many were asked about,
/// how many of those are local only, and whether that last number is
/// exact.
type StatusReport = (
    bool,
    Option<String>,
    Option<String>,
    usize,
    usize,
    usize,
    usize,
    bool,
);

/// What is configured, what answers, and how much is here but not there.
///
/// Returns `(supported, url, unreachable, objects, rows, checked,
/// local_only, exact)`. A tuple rather than a class because the CLI is its
/// only caller and formats every field itself.
#[pyfunction]
#[pyo3(signature = (cache=None, url=None))]
fn store_status(cache: Option<PathBuf>, url: Option<String>) -> PyResult<StatusReport> {
    let root = cache.unwrap_or_else(|| PathBuf::from(".ligase"));
    let store = ligase::store::TieredStore::open(&root.join("store"), url.as_deref())
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

    let unreachable = match store.reachable() {
        Some(Err(why)) => Some(why),
        Some(Ok(())) | None => None,
    };
    let survey = store
        .survey(SURVEY_BOUND)
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

    Ok((
        ligase::store::supports_remote(),
        store.remote_url().map(ToOwned::to_owned),
        unreachable,
        survey.objects,
        survey.rows,
        survey.checked,
        survey.local_only,
        survey.exact,
    ))
}

/// Push everything in the local tier that the remote has not got.
///
/// Returns `(pushed, failed)`. A single object that will not upload is
/// counted rather than fatal: stopping half way through a lab's cache
/// helps nobody, and the count is what tells a user to look.
#[pyfunction]
#[pyo3(signature = (cache=None, url=None))]
fn store_push(cache: Option<PathBuf>, url: Option<String>) -> PyResult<(usize, usize)> {
    let root = cache.unwrap_or_else(|| PathBuf::from(".ligase"));
    let store = ligase::store::TieredStore::open(&root.join("store"), url.as_deref())
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
    store
        .push_all()
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))
}

/// Fetch every object this index refers to that the local tier lacks.
///
/// Returns `(fetched, missing)`. `missing` is an object the remote does
/// not have either, which is not an error: it means a step will run.
#[pyfunction]
#[pyo3(signature = (cache=None, url=None))]
fn store_pull(cache: Option<PathBuf>, url: Option<String>) -> PyResult<(usize, usize)> {
    use ligase::store::BlobStore as _;

    let root = cache.unwrap_or_else(|| PathBuf::from(".ligase"));
    let cache = Cache::open_with(&root, url.as_deref())
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;
    let wanted = cache
        .referenced_objects()
        .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

    let mut missing = 0;
    for key in &wanted {
        match cache.objects().local_path(key) {
            Ok(Some(_)) => {}
            // Gone from both tiers, or refused for a hash mismatch. Both
            // mean a step runs, and the mismatch is counted and named by
            // the store.
            Ok(None) | Err(_) => missing += 1,
        }
    }
    Ok((cache.objects().stats().fetched as usize, missing))
}

/// Version of the compiled core engine.
#[pyfunction]
fn version() -> &'static str {
    ligase::version()
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(load_reference, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_remote, m)?)?;
    m.add_function(wrap_pyfunction!(resolve_query, m)?)?;
    m.add_function(wrap_pyfunction!(seed_records, m)?)?;
    m.add_function(wrap_pyfunction!(load_entity_set, m)?)?;
    m.add_function(wrap_pyfunction!(load_ranked_list, m)?)?;
    m.add_function(wrap_pyfunction!(load_knowledge_graph, m)?)?;
    m.add_function(wrap_pyfunction!(parse_representation, m)?)?;
    m.add_function(wrap_pyfunction!(load_molecule_set, m)?)?;
    m.add_function(wrap_pyfunction!(load_molecule_ranking, m)?)?;
    m.add_function(wrap_pyfunction!(conversion_axis, m)?)?;
    m.add_function(wrap_pyfunction!(conversion_target_parameter, m)?)?;
    m.add_function(wrap_pyfunction!(map_identifiers, m)?)?;
    m.add_function(wrap_pyfunction!(load_alignment, m)?)?;
    m.add_function(wrap_pyfunction!(load_variants, m)?)?;
    m.add_function(wrap_pyfunction!(load_reads, m)?)?;
    m.add_function(wrap_pyfunction!(load_intervals, m)?)?;
    m.add_function(wrap_pyfunction!(load_samples, m)?)?;
    m.add_function(wrap_pyfunction!(check_step, m)?)?;
    m.add_function(wrap_pyfunction!(comparable, m)?)?;
    m.add_function(wrap_pyfunction!(interpret_resistance, m)?)?;
    m.add_function(wrap_pyfunction!(resistance_algorithms, m)?)?;
    m.add_function(wrap_pyfunction!(default_resistance_algorithm, m)?)?;
    m.add_function(wrap_pyfunction!(resistance_fingerprint, m)?)?;
    m.add_function(wrap_pyfunction!(citation, m)?)?;
    m.add_function(wrap_pyfunction!(canonical_form, m)?)?;
    m.add_function(wrap_pyfunction!(tool_version, m)?)?;
    m.add_function(wrap_pyfunction!(store_status, m)?)?;
    m.add_function(wrap_pyfunction!(store_push, m)?)?;
    m.add_function(wrap_pyfunction!(store_pull, m)?)?;
    m.add_class::<PyDataset>()?;
    m.add_class::<PyGraph>()?;
    m.add("LoadError", m.py().get_type::<LoadError>())?;
    m.add("RemoteError", m.py().get_type::<RemoteError>())?;
    m.add("FrameError", m.py().get_type::<FrameError>())?;
    Ok(())
}
