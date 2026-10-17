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
use pyo3::exceptions::PyException;
use pyo3::prelude::*;

use ligase::check::{Binding, check_step as core_check_step};
use ligase::dim::Dimension;
use ligase::load::{self, Declared, LoadError as CoreLoadError};
use ligase::types::Dataset;

/// One contig as Python sees it: name, length, checksum.
type PyContig = (String, Option<u64>, Option<String>);

/// One disagreement as Python sees it: kind, step, dimension, message.
///
/// Python turns the kind into the matching exception class. The message is
/// built in the core so that the Rust tests assert on the exact wording.
type PyCheckError = (&'static str, String, &'static str, String);

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
        .into_iter()
        .map(|error| {
            (
                error.kind().name(),
                error.step().to_owned(),
                error.dimension().name(),
                error.to_string(),
            )
        })
        .collect()
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
    m.add_function(wrap_pyfunction!(check_step, m)?)?;
    m.add_class::<PyDataset>()?;
    m.add("LoadError", m.py().get_type::<LoadError>())?;
    Ok(())
}
