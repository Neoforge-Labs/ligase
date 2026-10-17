//! PyO3 bindings exposing the Ligase core engine as `ligase._core`.
//!
//! This module is deliberately private and thin: it translates between
//! Python and the core engine and holds no logic of its own. The public
//! Python API lives in `python/ligase/__init__.py`.

use pyo3::prelude::*;

/// Version of the compiled core engine.
#[pyfunction]
fn version() -> &'static str {
    ligase::version()
}

#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}
