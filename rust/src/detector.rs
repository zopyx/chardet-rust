//! UniversalDetector - streaming encoding detection.

use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::enums::{EncodingEra, LanguageFilter};
use crate::pipeline::orchestrator::run_pipeline;
use crate::pipeline::{DetectionResult, DEFAULT_MAX_BYTES};

/// Streaming character encoding detector.
#[pyclass]
pub struct UniversalDetector {
    /// Whether to rename legacy encodings.
    #[pyo3(get, set)]
    should_rename_legacy: bool,
    /// Encoding era filter.
    encoding_era: EncodingEra,
    /// Maximum bytes to buffer.
    max_bytes: usize,
    /// Internal buffer.
    buffer: Vec<u8>,
    /// Whether detection is complete.
    #[pyo3(get)]
    done: bool,
    /// Whether detector is closed.
    closed: bool,
    /// Detection result (cached after close).
    result: Option<DetectionResult>,
}

#[pymethods]
impl UniversalDetector {
    /// Create a new UniversalDetector.
    #[new]
    #[pyo3(signature = (
        lang_filter = LanguageFilter::ALL,
        should_rename_legacy = true,
        encoding_era = EncodingEra::All,
        max_bytes = DEFAULT_MAX_BYTES
    ))]
    fn new(
        lang_filter: LanguageFilter,
        should_rename_legacy: bool,
        encoding_era: EncodingEra,
        max_bytes: usize,
    ) -> PyResult<Self> {
        // Note: lang_filter is deprecated and ignored
        if lang_filter != LanguageFilter::ALL {
            // Would emit deprecation warning in Python
        }

        if max_bytes < 1 {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "max_bytes must be a positive integer",
            ));
        }

        Ok(Self {
            should_rename_legacy,
            encoding_era,
            max_bytes,
            buffer: Vec::new(),
            done: false,
            closed: false,
            result: None,
        })
    }

    /// Feed a chunk of bytes to the detector.
    fn feed(&mut self, byte_str: &[u8]) -> PyResult<()> {
        if self.closed {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "feed() called after close() without reset()",
            ));
        }

        if self.done {
            return Ok(());
        }

        let remaining = self.max_bytes.saturating_sub(self.buffer.len());
        if remaining > 0 {
            self.buffer
                .extend_from_slice(&byte_str[..byte_str.len().min(remaining)]);
        }

        if self.buffer.len() >= self.max_bytes {
            self.done = true;
        }

        Ok(())
    }

    /// Finalize detection and return the best result.
    fn close(&mut self, py: Python) -> PyResult<PyObject> {
        if !self.closed {
            self.closed = true;
            let results = run_pipeline(&self.buffer, self.encoding_era, self.max_bytes);
            self.result = Some(results[0].clone());
            self.done = true;
        }

        self.get_result(py)
    }

    /// Reset the detector to its initial state for reuse.
    fn reset(&mut self) {
        self.buffer.clear();
        self.done = false;
        self.closed = false;
        self.result = None;
    }

    /// Get the current best detection result.
    #[getter]
    fn get_result(&self, py: Python) -> PyResult<PyObject> {
        match &self.result {
            Some(result) => result.to_py_dict(py, self.should_rename_legacy),
            None => {
                // Return empty result
                let dict = PyDict::new(py);
                dict.set_item("encoding", py.None())?;
                dict.set_item("confidence", 0.0)?;
                dict.set_item("language", py.None())?;
                Ok(dict.into())
            }
        }
    }
}

/// Detect the encoding of a byte string.
pub fn detect_bytes(data: &[u8], encoding_era: EncodingEra, max_bytes: usize) -> DetectionResult {
    let results = run_pipeline(data, encoding_era, max_bytes);
    // Results are already sorted by the pipeline; confusion resolution handles ties
    results[0].clone()
}

/// Detect all possible encodings of the given byte string.
pub fn detect_all_bytes(
    data: &[u8],
    encoding_era: EncodingEra,
    max_bytes: usize,
    ignore_threshold: bool,
) -> Vec<DetectionResult> {
    let results = run_pipeline(data, encoding_era, max_bytes);

    // Filter by threshold if requested
    if !ignore_threshold {
        let threshold: f64 = 0.20;
        let filtered: Vec<_> = results
            .iter()
            .filter(|r| r.confidence > threshold)
            .cloned()
            .collect();

        if !filtered.is_empty() {
            return filtered;
        }
    }

    // Results are already sorted by the pipeline; don't re-sort to preserve confusion resolution
    results
}
