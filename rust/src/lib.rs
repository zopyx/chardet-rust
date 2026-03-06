//! Universal character encoding detector - Rust implementation
//!
//! This is a port of the chardet Python library to Rust with Python bindings.

use pyo3::prelude::*;
use pyo3::types::PyList;

pub mod bigram_models;
pub mod detector;
pub mod enums;
pub mod equivalences;
pub mod models;
pub mod pipeline;
pub mod registry;

use crate::detector::{detect_all_bytes, detect_bytes};
use crate::enums::EncodingEra;

/// Detect the encoding of a byte string.
///
/// Parameters
/// ----------
/// byte_str : bytes
///     The byte sequence to detect encoding for.
/// should_rename_legacy : bool, optional
///     If True (default), remap legacy encoding names to their modern equivalents.
/// encoding_era : EncodingEra, optional
///     Restrict candidate encodings to the given era. Default is ALL.
/// max_bytes : int, optional
///     Maximum number of bytes to examine from byte_str. Default is 200000.
///
/// Returns
/// -------
/// dict
///     A dictionary with keys "encoding", "confidence", and "language".
#[pyfunction]
#[pyo3(signature = (byte_str, should_rename_legacy=true, encoding_era=EncodingEra::All, max_bytes=200_000))]
pub fn detect(
    py: Python,
    byte_str: &[u8],
    should_rename_legacy: bool,
    encoding_era: EncodingEra,
    max_bytes: usize,
) -> PyResult<PyObject> {
    let result = detect_bytes(byte_str, encoding_era, max_bytes);
    result.to_py_dict(py, should_rename_legacy)
}

/// Detect all possible encodings of the given byte string.
///
/// Parameters
/// ----------
/// byte_str : bytes
///     The byte sequence to detect encoding for.
/// ignore_threshold : bool, optional
///     If True, return all candidate encodings regardless of confidence score.
/// should_rename_legacy : bool, optional
///     If True (default), remap legacy encoding names to their modern equivalents.
/// encoding_era : EncodingEra, optional
///     Restrict candidate encodings to the given era. Default is ALL.
/// max_bytes : int, optional
///     Maximum number of bytes to examine from byte_str. Default is 200000.
///
/// Returns
/// -------
/// list
///     A list of dictionaries, each with keys "encoding", "confidence", and "language".
#[pyfunction]
#[pyo3(signature = (byte_str, ignore_threshold=false, should_rename_legacy=true, encoding_era=EncodingEra::All, max_bytes=200_000))]
pub fn detect_all(
    py: Python,
    byte_str: &[u8],
    ignore_threshold: bool,
    should_rename_legacy: bool,
    encoding_era: EncodingEra,
    max_bytes: usize,
) -> PyResult<PyObject> {
    let results = detect_all_bytes(byte_str, encoding_era, max_bytes, ignore_threshold);

    let list = PyList::empty(py);
    for result in results {
        let dict = result.to_py_dict(py, should_rename_legacy)?;
        list.append(dict)?;
    }

    Ok(list.into())
}

/// The chardet_rs Python module.
#[pymodule]
fn _chardet_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", "7.0.0")?;

    // Initialize bigram models from embedded data
    // Note: models.bin should be at src/chardet/models/models.bin relative to project root
    // For now, we'll try to load it at runtime

    // Add main functions
    m.add_wrapped(wrap_pyfunction!(detect))?;
    m.add_wrapped(wrap_pyfunction!(detect_all))?;

    // Add enums
    m.add_class::<enums::EncodingEra>()?;
    m.add_class::<enums::LanguageFilter>()?;

    // Add detector class
    m.add_class::<detector::UniversalDetector>()?;

    // Add model loading function
    #[pyfn(m)]
    fn _load_models(data: &[u8]) -> PyResult<()> {
        bigram_models::init_models(data).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
    }

    // Add function to check if models are loaded
    #[pyfn(m)]
    fn _models_loaded() -> bool {
        bigram_models::models_loaded()
    }

    Ok(())
}
