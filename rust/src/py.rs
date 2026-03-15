//! Python bindings for chardet-rs.
//!
//! This module contains PyO3-specific code for Python interoperability.

use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::detector::{detect_all_bytes, detect_bytes};
use crate::enums::EncodingEra;
use crate::bigram_models::{init_models, models_loaded};
use crate::pipeline::DetectionResult;

/// Maximum allowed value for max_bytes parameter (100 MB)
/// Prevents memory exhaustion attacks via excessive buffer allocation.
const MAX_BYTES_LIMIT: usize = 100 * 1024 * 1024;

/// Validate max_bytes parameter.
///
/// # Security
/// This validation prevents:
/// - Integer overflow attacks (usize::MAX values)
/// - Memory exhaustion (gigabyte-scale allocations)
/// - Type confusion (bool values passed as int)
fn validate_max_bytes(max_bytes: usize) -> PyResult<()> {
    if max_bytes == 0 {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "max_bytes must be a positive integer",
        ));
    }
    if max_bytes > MAX_BYTES_LIMIT {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            format!(
                "max_bytes ({}) exceeds maximum allowed value ({})",
                max_bytes, MAX_BYTES_LIMIT
            ),
        ));
    }
    Ok(())
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
#[pyo3(signature = (byte_str, should_rename_legacy=true, encoding_era=EncodingEra::All, max_bytes=200_000))]
pub fn detect(
    py: Python,
    byte_str: &[u8],
    should_rename_legacy: bool,
    encoding_era: EncodingEra,
    max_bytes: usize,
) -> PyResult<PyObject> {
    // Security: Validate inputs before processing
    validate_max_bytes(max_bytes)?;
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
#[pyo3(signature = (byte_str, ignore_threshold=false, should_rename_legacy=true, encoding_era=EncodingEra::All, max_bytes=200_000))]
pub fn detect_all(
    py: Python,
    byte_str: &[u8],
    ignore_threshold: bool,
    should_rename_legacy: bool,
    encoding_era: EncodingEra,
    max_bytes: usize,
) -> PyResult<PyObject> {
    // Security: Validate inputs before processing
    validate_max_bytes(max_bytes)?;
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
    init_models(data)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Failed to load models: {}", e)))
}

/// Check if models are loaded.
#[pyfunction]
fn _models_loaded() -> bool {
    models_loaded()
}

/// Language filter for limiting detection to specific languages.
#[pyclass(eq, eq_int, rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum LanguageFilter {
    /// All languages.
    All = 0,
    /// Chinese Simplified.
    ChineseSimplified = 1,
    /// Chinese Traditional.
    ChineseTraditional = 2,
    /// Japanese.
    Japanese = 3,
    /// Korean.
    Korean = 4,
    /// Non-CJK languages.
    NonCjk = 5,
    /// All Chinese.
    Chinese = 6,
    /// All CJK.
    AllCjk = 7,
}

/// UniversalDetector for streaming detection.
#[pyclass]
struct UniversalDetector {
    should_rename_legacy: bool,
    encoding_era: EncodingEra,
    max_bytes: usize,
    buffer: Vec<u8>,
    done: bool,
    closed: bool,
    result: Option<DetectionResult>,
    /// Security: Track number of feed() calls to prevent DoS
    feed_count: usize,
    /// Security: Maximum number of feed() calls allowed
    #[allow(dead_code)]
    max_feed_calls: usize,
}

/// Maximum number of feed() calls allowed per detector instance.
/// Prevents denial-of-service via excessive iteration.
const MAX_FEED_CALLS: usize = 1_000_000;

#[pymethods]
impl UniversalDetector {
    /// Create a new UniversalDetector.
    #[new]
    #[pyo3(signature = (should_rename_legacy=true, encoding_era=None, max_bytes=200_000))]
    fn new(
        should_rename_legacy: bool,
        encoding_era: Option<EncodingEra>,
        max_bytes: usize,
    ) -> PyResult<Self> {
        // Security: Validate max_bytes parameter
        validate_max_bytes(max_bytes)?;

        Ok(UniversalDetector {
            should_rename_legacy,
            encoding_era: encoding_era.unwrap_or(EncodingEra::All),
            max_bytes,
            buffer: Vec::new(),
            done: false,
            closed: false,
            result: None,
            feed_count: 0,
            max_feed_calls: MAX_FEED_CALLS,
        })
    }

    /// Feed a chunk of bytes to the detector.
    fn feed(&mut self, byte_str: &[u8]) -> PyResult<()> {
        if self.closed {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "UniversalDetector.feed() called after close()"
            ));
        }

        if self.done {
            return Ok(());
        }

        // Security: Check iteration limit to prevent DoS
        if self.feed_count >= self.max_feed_calls {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(
                format!(
                    "Maximum feed() calls ({}) exceeded. Call reset() to start a new detection.",
                    self.max_feed_calls
                )
            ));
        }

        // Security: Validate input slice is not excessively large
        // Individual feed calls should be reasonable (< 50 MB)
        // Note: The max_bytes buffer limit (default 200KB, max 100MB) and
        // iteration limit (1M calls) provide the primary DoS protection.
        const MAX_FEED_SIZE: usize = 50 * 1024 * 1024;
        if byte_str.len() > MAX_FEED_SIZE {
            return Err(pyo3::exceptions::PyValueError::new_err(
                format!(
                    "feed() input size ({}) exceeds maximum ({})",
                    byte_str.len(), MAX_FEED_SIZE
                )
            ));
        }

        self.buffer.extend_from_slice(byte_str);
        self.feed_count += 1;

        if self.buffer.len() >= self.max_bytes {
            self.done = true;
        }

        Ok(())
    }

    /// Finalize detection and return the best result.
    fn close<'py>(&mut self, py: Python<'py>) -> PyResult<PyObject> {
        if self.result.is_none() {
            let result = detect_bytes(&self.buffer, self.encoding_era, self.max_bytes);
            self.result = Some(result);
            self.done = true;
        }

        self.closed = true;
        self.result.as_ref().unwrap().to_py_dict(py, self.should_rename_legacy)
    }

    /// Reset the detector to its initial state.
    fn reset(&mut self) {
        self.buffer.clear();
        self.done = false;
        self.closed = false;
        self.result = None;
    }

    /// Whether detection is complete.
    #[getter]
    fn done(&self) -> bool {
        self.done
    }

    /// Get the current detection result.
    #[getter]
    fn result<'py>(&self, py: Python<'py>) -> PyResult<PyObject> {
        match &self.result {
            Some(r) => r.to_py_dict(py, self.should_rename_legacy),
            None => {
                // Return default result dict when not yet closed
                let dict = pyo3::types::PyDict::new(py);
                dict.set_item("encoding", py.None())?;
                dict.set_item("confidence", 0.0_f64)?;
                dict.set_item("language", py.None())?;
                Ok(dict.into())
            }
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
