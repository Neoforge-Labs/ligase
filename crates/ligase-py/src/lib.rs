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
use ligase::dim::{CoordinateSystem, Dimension};
use ligase::exec::{self, Host, Options, Recipe, Stores, Task};
use ligase::graph::{Argument, Graph, NodeId, StepSpec};
use ligase::hivdb;
use ligase::load::{self, Declared, IntervalFormat, LoadError as CoreLoadError};
use ligase::note::Note;
use ligase::prov::{self, Prov};
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
/// parameters, citations.
///
/// Exactly one of the command and the callback is set. A command is run by
/// the core; a callback goes back to Python, which is the only thing in a
/// run that takes the GIL.
type PyTask = (
    usize,
    String,
    Option<String>,
    Option<u64>,
    Vec<String>,
    Vec<(String, String)>,
    Vec<String>,
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
/// two canonical hashes, and the two durations in milliseconds.
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

/// A methods section as Python sees it: the ways steps were run, the gaps,
/// the references, the reference list, the tools with nothing to cite, the
/// engine, the platform, the summary line and the whole thing as text.
type PyMethods = (
    Vec<PyMethodsGroup>,
    Vec<PyMethodsGap>,
    Vec<PyReference>,
    Vec<String>,
    Vec<String>,
    String,
    String,
    String,
    String,
);

/// What to do besides describing the run, as Python gives it: a directory
/// to write a bundle into, a bundle to check against, and files to carry
/// into the bundle as they are.
type PyWriteUp = (Option<PathBuf>, Option<PathBuf>, Vec<PathBuf>);

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

#[pyfunction]
#[pyo3(signature = (path, organism=None, build=None, frame=None))]
fn load_reference(
    path: PathBuf,
    organism: Option<String>,
    build: Option<String>,
    frame: Option<(String, u64)>,
) -> PyResult<PyDataset> {
    let declared = Declared {
        organism,
        build,
        frame,
        samples: None,
    };
    load::reference(&path, &declared)
        .map(|inner| PyDataset { inner })
        .map_err(to_py)
}

#[pyfunction]
#[pyo3(signature = (path, organism=None, build=None, samples=None))]
fn load_alignment(
    path: PathBuf,
    organism: Option<String>,
    build: Option<String>,
    samples: Option<Vec<String>>,
) -> PyResult<PyDataset> {
    let declared = Declared {
        organism,
        build,
        frame: None,
        samples,
    };
    load::alignment(&path, &declared)
        .map(|inner| PyDataset { inner })
        .map_err(to_py)
}

#[pyfunction]
#[pyo3(signature = (path, organism=None, build=None, samples=None))]
fn load_variants(
    path: PathBuf,
    organism: Option<String>,
    build: Option<String>,
    samples: Option<Vec<String>>,
) -> PyResult<PyDataset> {
    let declared = Declared {
        organism,
        build,
        frame: None,
        samples,
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
    organism: Option<String>,
    samples: Option<Vec<String>>,
) -> PyResult<PyDataset> {
    let declared = Declared {
        organism,
        build: None,
        samples,
        frame: None,
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
    organism: Option<String>,
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
        organism,
        build,
        frame: None,
        samples: None,
    };
    load::intervals(&path, format, &declared)
        .map(|inner| PyDataset { inner })
        .map_err(to_py)
}

#[pyfunction]
#[pyo3(signature = (paths, paired, organism=None))]
fn load_samples(
    paths: Vec<PathBuf>,
    paired: bool,
    organism: Option<String>,
) -> PyResult<Vec<PyDataset>> {
    let declared = Declared {
        organism,
        build: None,
        samples: None,
        frame: None,
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
    #[pyo3(signature = (name, output, arguments, coordinates=None))]
    fn add_step(
        &mut self,
        name: &str,
        output: &str,
        arguments: Vec<PyArgument>,
        coordinates: Option<&str>,
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
        let (bundle, verify, include) = &write_up;
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

        let methods = prov::methods(&self.inner, &roots, &tasks, &report, &prov)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))?;

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
            |(node, code, command, callback, tools, parameters, citations)| {
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

/// Version of the compiled core engine.
#[pyfunction]
fn version() -> &'static str {
    ligase::version()
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(load_reference, m)?)?;
    m.add_function(wrap_pyfunction!(load_alignment, m)?)?;
    m.add_function(wrap_pyfunction!(load_variants, m)?)?;
    m.add_function(wrap_pyfunction!(load_reads, m)?)?;
    m.add_function(wrap_pyfunction!(load_intervals, m)?)?;
    m.add_function(wrap_pyfunction!(load_samples, m)?)?;
    m.add_function(wrap_pyfunction!(check_step, m)?)?;
    m.add_function(wrap_pyfunction!(interpret_resistance, m)?)?;
    m.add_function(wrap_pyfunction!(resistance_algorithms, m)?)?;
    m.add_function(wrap_pyfunction!(default_resistance_algorithm, m)?)?;
    m.add_function(wrap_pyfunction!(resistance_fingerprint, m)?)?;
    m.add_class::<PyDataset>()?;
    m.add_class::<PyGraph>()?;
    m.add("LoadError", m.py().get_type::<LoadError>())?;
    m.add("FrameError", m.py().get_type::<FrameError>())?;
    Ok(())
}
