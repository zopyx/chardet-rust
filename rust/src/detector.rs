//! UniversalDetector - streaming encoding detection.

use crate::enums::EncodingEra;
use crate::pipeline::orchestrator::run_pipeline;
use crate::pipeline::{DetectionResult, DEFAULT_MAX_BYTES};

/// Maximum allowed value for max_bytes parameter (100 MB)
/// Prevents memory exhaustion attacks via excessive buffer allocation.
const MAX_BYTES_LIMIT: usize = 100 * 1024 * 1024;

/// Maximum number of feed() calls allowed per detector instance.
/// Prevents denial-of-service via excessive iteration.
const MAX_FEED_CALLS: usize = 1_000_000;

/// Maximum size of individual feed() input (50 MB)
/// Note: The max_bytes buffer limit (default 200KB, max 100MB) and
/// iteration limit (1M calls) provide the primary DoS protection.
const MAX_FEED_SIZE: usize = 50 * 1024 * 1024;

/// Validate max_bytes parameter.
fn validate_max_bytes(max_bytes: usize) -> Result<(), String> {
    if max_bytes == 0 {
        return Err("max_bytes must be a positive integer".to_string());
    }
    if max_bytes > MAX_BYTES_LIMIT {
        return Err(format!(
            "max_bytes ({}) exceeds maximum allowed value ({})",
            max_bytes, MAX_BYTES_LIMIT
        ));
    }
    Ok(())
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

// Python bindings - only compiled when "python" feature is enabled
#[cfg(feature = "python")]
pub mod py {
    use super::*;
    use pyo3::prelude::*;
    use pyo3::types::PyDict;

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
        /// Security: Track number of feed() calls to prevent DoS
        feed_count: usize,
        /// Security: Maximum number of feed() calls allowed
        #[allow(dead_code)]
        max_feed_calls: usize,
    }

    #[pymethods]
    impl UniversalDetector {
        /// Create a new UniversalDetector.
        #[new]
        #[pyo3(signature = (
            should_rename_legacy = true,
            encoding_era = EncodingEra::All,
            max_bytes = DEFAULT_MAX_BYTES
        ))]
        fn new(
            should_rename_legacy: bool,
            encoding_era: EncodingEra,
            max_bytes: usize,
        ) -> PyResult<Self> {
            // Security: Validate max_bytes parameter
            validate_max_bytes(max_bytes)
                .map_err(PyErr::new::<pyo3::exceptions::PyValueError, _>)?;

            Ok(Self {
                should_rename_legacy,
                encoding_era,
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
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    "feed() called after close() without reset()",
                ));
            }

            if self.done {
                return Ok(());
            }

            // Security: Check iteration limit to prevent DoS
            if self.feed_count >= self.max_feed_calls {
                return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    format!(
                        "Maximum feed() calls ({}) exceeded. Call reset() to start a new detection.",
                        self.max_feed_calls
                    ),
                ));
            }

            // Security: Validate input slice is not excessively large
            if byte_str.len() > MAX_FEED_SIZE {
                return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                    format!(
                        "feed() input size ({}) exceeds maximum ({})",
                        byte_str.len(), MAX_FEED_SIZE
                    ),
                ));
            }

            let remaining = self.max_bytes.saturating_sub(self.buffer.len());
            if remaining > 0 {
                self.buffer
                    .extend_from_slice(&byte_str[..byte_str.len().min(remaining)]);
            }
            
            self.feed_count += 1;

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
}

// Re-export Python type when feature is enabled
#[cfg(feature = "python")]
pub use py::UniversalDetector;
