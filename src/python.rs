//! Python bindings, built with [maturin]/[PyO3] behind the `python` feature and
//! published to PyPI as `c2pa-zip`.
//!
//! Byte payloads map to and from Python `bytes`. A malformed container raises
//! `ValueError`; a container that simply carries no manifest returns `None`
//! rather than raising, because absence of provenance is not an error.
//!
//! [maturin]: https://www.maturin.rs/
//! [PyO3]: https://pyo3.rs/

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

fn map_err(e: crate::Error) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// Embed a C2PA Manifest Store into a ZIP-based document.
#[pyfunction]
fn embed_manifest<'py>(
    py: Python<'py>,
    zip: &[u8],
    manifest_store: &[u8],
) -> PyResult<Bound<'py, PyBytes>> {
    let out = crate::embed_manifest(zip, manifest_store).map_err(map_err)?;
    Ok(PyBytes::new(py, &out))
}

/// Read the embedded C2PA Manifest Store, or `None` when the document carries
/// no provenance.
#[pyfunction]
fn read_manifest<'py>(py: Python<'py>, zip: &[u8]) -> PyResult<Option<Bound<'py, PyBytes>>> {
    Ok(crate::read_manifest(zip)
        .map_err(map_err)?
        .map(|store| PyBytes::new(py, &store)))
}

/// Remove the C2PA Manifest Store from a ZIP-based document.
#[pyfunction]
fn remove_manifest<'py>(py: Python<'py>, zip: &[u8]) -> PyResult<Bound<'py, PyBytes>> {
    let out = crate::remove_manifest(zip).map_err(map_err)?;
    Ok(PyBytes::new(py, &out))
}

/// The byte ranges of the members covered by the collection hash, as a list of
/// `(name, start, length)` tuples.
#[pyfunction]
fn collection_members(zip: &[u8]) -> PyResult<Vec<(String, usize, usize)>> {
    Ok(crate::collection_members(zip)
        .map_err(map_err)?
        .into_iter()
        .map(|m| (m.name, m.content.start, m.content.len()))
        .collect())
}

/// The `(start, length)` byte range of the ZIP central directory.
#[pyfunction]
fn central_directory_range(zip: &[u8]) -> PyResult<(usize, usize)> {
    let r = crate::central_directory_range(zip).map_err(map_err)?;
    Ok((r.start, r.len()))
}

/// Structurally verify the embedding, as a dict with `has_manifest`,
/// `manifest_len`, and `is_valid_zip`.
///
/// This is a transport-level report only: it does not validate the manifest's
/// signature or its collection hash.
#[pyfunction]
fn verify<'py>(py: Python<'py>, zip: &[u8]) -> PyResult<Bound<'py, PyDict>> {
    let c = crate::verify(zip).map_err(map_err)?;
    let out = PyDict::new(py);
    out.set_item("has_manifest", c.has_manifest)?;
    out.set_item("manifest_len", c.manifest_len)?;
    out.set_item("is_valid_zip", c.is_valid_zip)?;
    Ok(out)
}

/// The path the Manifest Store is stored at inside the container.
#[pyfunction]
fn manifest_path() -> &'static str {
    crate::ZIP_MANIFEST_PATH
}

#[pymodule]
fn c2pa_zip(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(embed_manifest, m)?)?;
    m.add_function(wrap_pyfunction!(read_manifest, m)?)?;
    m.add_function(wrap_pyfunction!(remove_manifest, m)?)?;
    m.add_function(wrap_pyfunction!(collection_members, m)?)?;
    m.add_function(wrap_pyfunction!(central_directory_range, m)?)?;
    m.add_function(wrap_pyfunction!(verify, m)?)?;
    m.add_function(wrap_pyfunction!(manifest_path, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
