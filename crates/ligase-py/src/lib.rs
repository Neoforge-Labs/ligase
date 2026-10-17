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

use std::path::PathBuf;

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;

use ligase::check::{Binding, CheckError, check_step as core_check_step, render, summarise};
use ligase::dim::{CoordinateSystem, Dimension};
use ligase::graph::{Argument, Graph, NodeId, StepSpec};
use ligase::load::{self, Declared, IntervalFormat, LoadError as CoreLoadError};
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

#[pyfunction]
#[pyo3(signature = (path, organism=None, build=None))]
fn load_reference(
    path: PathBuf,
    organism: Option<String>,
    build: Option<String>,
) -> PyResult<PyDataset> {
    let declared = Declared {
        organism,
        build,
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

    fn __len__(&self) -> usize {
        self.inner.len()
    }
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
    m.add_class::<PyDataset>()?;
    m.add_class::<PyGraph>()?;
    m.add("LoadError", m.py().get_type::<LoadError>())?;
    Ok(())
}
