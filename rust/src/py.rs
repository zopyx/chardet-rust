//! Python bindings for chardet-rs.
//!
//! This module contains PyO3-specific code for Python interoperability.

use pyo3::prelude::*;
use pyo3::types::PyList;

use crate::detector::{detect_all_bytes, detect_bytes};
use crate::enums::EncodingEra;
use crate::bigram_models::{init_models, models_loaded};
use crate::pipeline::DetectionResult;

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
    #[allow(dead_code)]
    lang_filter: LanguageFilter,
    should_rename_legacy: bool,
    encoding_era: EncodingEra,
    max_bytes: usize,
    buffer: Vec<u8>,
    done: bool,
    closed: bool,
    result: Option<DetectionResult>,
}

#[pymethods]
impl UniversalDetector {
    /// Create a new UniversalDetector.
    #[new]
    #[pyo3(signature = (lang_filter=None, should_rename_legacy=true, encoding_era=None, max_bytes=200_000))]
    fn new(
        lang_filter: Option<LanguageFilter>,
        should_rename_legacy: bool,
        encoding_era: Option<EncodingEra>,
        max_bytes: usize,
    ) -> Self {
        UniversalDetector {
            lang_filter: lang_filter.unwrap_or(LanguageFilter::All),
            should_rename_legacy,
            encoding_era: encoding_era.unwrap_or(EncodingEra::All),
            max_bytes,
            buffer: Vec::new(),
            done: false,
            closed: false,
            result: None,
        }
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
        
        self.buffer.extend_from_slice(byte_str);
        
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
