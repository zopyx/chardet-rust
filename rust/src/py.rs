//! Python bindings for chardet-rs.
//!
//! This module contains PyO3-specific code for Python interoperability.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3::types::PyList;

use crate::bigram_models::{init_models, models_loaded};
use crate::detector::{
    detect_all_bytes, detect_bytes, validate_max_bytes, StreamingDetector, StreamingDetectorError,
};
use crate::enums::{EncodingEra, LanguageFilter};
use crate::pipeline::DEFAULT_MAX_BYTES;

fn map_streaming_error(err: StreamingDetectorError) -> PyErr {
    match err {
        StreamingDetectorError::MaxFeedCallsExceeded { .. } => {
            pyo3::exceptions::PyRuntimeError::new_err(err.to_string())
        }
        StreamingDetectorError::ZeroMaxBytes
        | StreamingDetectorError::MaxBytesLimitExceeded { .. }
        | StreamingDetectorError::FeedAfterClose
        | StreamingDetectorError::FeedTooLarge { .. } => {
            pyo3::exceptions::PyValueError::new_err(err.to_string())
        }
    }
}

/// Validate byte input data.
///
/// # Security
/// This validation prevents:
/// - Empty input edge cases
/// - Excessively large allocations
fn validate_byte_input(_data: &[u8]) -> PyResult<()> {
    // Empty input is valid but may produce low-confidence results
    // No upper limit here - pipeline will truncate to max_bytes
    Ok(())
}

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
#[pyo3(signature = (
    byte_str,
    should_rename_legacy = true,
    encoding_era = EncodingEra::All,
    max_bytes = DEFAULT_MAX_BYTES
))]
pub fn detect(
    py: Python,
    byte_str: &[u8],
    should_rename_legacy: bool,
    encoding_era: EncodingEra,
    max_bytes: usize,
) -> PyResult<PyObject> {
    // Security: Validate inputs before processing
    validate_max_bytes(max_bytes).map_err(map_streaming_error)?;
    validate_byte_input(byte_str)?;

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
#[pyo3(signature = (
    byte_str,
    ignore_threshold = false,
    should_rename_legacy = true,
    encoding_era = EncodingEra::All,
    max_bytes = DEFAULT_MAX_BYTES
))]
pub fn detect_all(
    py: Python,
    byte_str: &[u8],
    ignore_threshold: bool,
    should_rename_legacy: bool,
    encoding_era: EncodingEra,
    max_bytes: usize,
) -> PyResult<PyObject> {
    // Security: Validate inputs before processing
    validate_max_bytes(max_bytes).map_err(map_streaming_error)?;
    validate_byte_input(byte_str)?;

    let results = detect_all_bytes(byte_str, encoding_era, max_bytes, ignore_threshold);

    let list = PyList::empty(py);
    for result in results {
        let dict = result.to_py_dict(py, should_rename_legacy)?;
        list.append(dict)?;
    }

    Ok(list.into())
}

/// Load bigram models from bytes.
#[pyfunction]
fn _load_models(data: &[u8]) -> PyResult<()> {
    init_models(data).map_err(|e| {
        pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to load models: {}", e))
    })
}

/// Check if models are loaded.
#[pyfunction]
fn _models_loaded() -> bool {
    models_loaded()
}

fn empty_result_dict(py: Python) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    dict.set_item("encoding", py.None())?;
    dict.set_item("confidence", 0.0_f64)?;
    dict.set_item("language", py.None())?;
    Ok(dict.into())
}

/// UniversalDetector for streaming detection.
#[pyclass]
struct UniversalDetector {
    should_rename_legacy: bool,
    inner: StreamingDetector,
}

#[pymethods]
impl UniversalDetector {
    /// Create a new UniversalDetector.
    #[new]
    #[pyo3(signature = (
        should_rename_legacy = true,
        encoding_era = None,
        max_bytes = DEFAULT_MAX_BYTES
    ))]
    fn new(
        should_rename_legacy: bool,
        encoding_era: Option<EncodingEra>,
        max_bytes: usize,
    ) -> PyResult<Self> {
        let inner = StreamingDetector::new(encoding_era.unwrap_or(EncodingEra::All), max_bytes)
            .map_err(map_streaming_error)?;

        Ok(UniversalDetector {
            should_rename_legacy,
            inner,
        })
    }

    /// Feed a chunk of bytes to the detector.
    fn feed(&mut self, byte_str: &[u8]) -> PyResult<()> {
        self.inner.feed(byte_str).map_err(map_streaming_error)
    }

    /// Finalize detection and return the best result.
    fn close<'py>(&mut self, py: Python<'py>) -> PyResult<PyObject> {
        let result = self.inner.close().clone();
        result.to_py_dict(py, self.should_rename_legacy)
    }

    /// Reset the detector to its initial state.
    fn reset(&mut self) {
        self.inner.reset();
    }

    /// Whether detection is complete.
    #[getter]
    fn done(&self) -> bool {
        self.inner.done()
    }

    /// Get the current detection result.
    #[getter]
    fn result<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        match self.inner.result() {
            Some(r) => r.to_py_dict(py, self.should_rename_legacy),
            None => empty_result_dict(py),
        }
    }
}

/// The chardet_rs Python module.
#[pymodule]
fn _chardet_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(detect, m)?)?;
    m.add_function(wrap_pyfunction!(detect_all, m)?)?;
    m.add_function(wrap_pyfunction!(_load_models, m)?)?;
    m.add_function(wrap_pyfunction!(_models_loaded, m)?)?;

    // Add classes
    m.add_class::<UniversalDetector>()?;
    m.add_class::<LanguageFilter>()?;
    m.add_class::<EncodingEra>()?;

    Ok(())
}
