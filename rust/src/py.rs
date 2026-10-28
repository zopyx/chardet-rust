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
use crate::pipeline::markup::declarations;
use crate::pipeline::orchestrator::{run_pipeline_with, PipelineOptions};
use crate::pipeline::DEFAULT_MAX_BYTES;
use std::collections::HashSet;

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

/// Run the detection pipeline with the reference's full option set.
///
/// Returns results under their internal canonical names; output-name
/// rendering (compat names, preferred supersets) stays on the Python side.
#[pyfunction]
#[pyo3(signature = (
    byte_str,
    encoding_era_bits = 63,
    max_bytes = DEFAULT_MAX_BYTES,
    include_encodings = None,
    exclude_encodings = None,
    no_match_encoding = "cp1252",
    empty_input_encoding = "utf-8",
    full_ranking = false,
    input_truncated = false
))]
#[allow(clippy::too_many_arguments)]
fn run_pipeline(
    py: Python,
    byte_str: &[u8],
    encoding_era_bits: u8,
    max_bytes: usize,
    include_encodings: Option<Vec<String>>,
    exclude_encodings: Option<Vec<String>>,
    no_match_encoding: &str,
    empty_input_encoding: &str,
    full_ranking: bool,
    input_truncated: bool,
) -> PyResult<PyObject> {
    validate_max_bytes(max_bytes).map_err(map_streaming_error)?;

    let include: Option<HashSet<String>> =
        include_encodings.map(|names| names.into_iter().collect());
    let exclude: Option<HashSet<String>> =
        exclude_encodings.map(|names| names.into_iter().collect());

    let options = PipelineOptions {
        include_encodings: include.as_ref(),
        exclude_encodings: exclude.as_ref(),
        no_match_encoding,
        empty_input_encoding,
        full_ranking,
        input_truncated,
    };

    let results = run_pipeline_with(
        byte_str,
        EncodingEra::from_bits(encoding_era_bits),
        max_bytes,
        &options,
    );

    let list = PyList::empty(py);
    for result in results {
        list.append(result.to_py_dict(py, false)?)?;
    }
    Ok(list.into())
}

/// Debug helper: list the charset declarations the markup stage finds.
#[pyfunction]
fn _debug_declarations(py: Python, byte_str: &[u8]) -> PyResult<PyObject> {
    let list = PyList::empty(py);
    for declaration in declarations(byte_str) {
        let item = PyDict::new(py);
        match declaration.encoding {
            Some(encoding) => item.set_item("encoding", encoding)?,
            None => item.set_item("encoding", py.None())?,
        }
        item.set_item("mime_type", declaration.mime_type)?;
        list.append(item)?;
    }
    Ok(list.into())
}

/// Debug helper: run the UTF-16/32 pattern stage alone.
#[pyfunction]
fn _debug_utf1632(py: Python, byte_str: &[u8]) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    match crate::pipeline::utf1632::detect_utf1632_patterns(byte_str) {
        Some(result) => {
            dict.set_item("encoding", result.encoding)?;
            dict.set_item("confidence", result.confidence)?;
        }
        None => {
            dict.set_item("encoding", py.None())?;
            dict.set_item("confidence", 0.0)?;
        }
    }
    Ok(dict.into())
}

/// Debug helper: list the encodings the bigram model set knows.
#[pyfunction]
fn _debug_model_names(py: Python) -> PyResult<PyObject> {
    crate::pipeline::statistical::ensure_models_public();
    let names = crate::pipeline::statistical::model_encoding_names();
    Ok(PyList::new(py, names)?.into())
}

/// Debug helper: per-encoding statistical scores for a byte string.
#[pyfunction]
fn _debug_scores(py: Python, byte_str: &[u8], encodings: Vec<String>) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    crate::pipeline::statistical::ensure_models_public();
    for name in encodings {
        let (score, lang) = crate::pipeline::statistical::debug_score(byte_str, &name);
        let item = PyDict::new(py);
        item.set_item("score", score)?;
        item.set_item("language", lang)?;
        dict.set_item(name, item)?;
    }
    Ok(dict.into())
}

/// Debug helper: report the niche-Latin demotion decision for a byte string.
#[pyfunction]
fn _debug_demote(py: Python, byte_str: &[u8]) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    let diff = crate::encoding_tables::differing_high_bytes("hp-roman8", "iso8859-1");
    dict.set_item("diff_len", diff.len())?;
    let present: Vec<u8> = byte_str
        .iter()
        .copied()
        .filter(|byte| diff.contains(byte))
        .collect();
    dict.set_item("differing_present", present)?;
    let candidates = crate::registry::get_candidates(crate::enums::EncodingEra::All);
    let valid = crate::pipeline::decode_validity::filter_by_validity(byte_str, &candidates);
    let scored = crate::pipeline::statistical::score_candidates(
        &byte_str[..byte_str.len().min(16384)],
        &valid,
        false,
    );
    let before: Vec<String> = scored
        .iter()
        .filter_map(|r| r.encoding.clone())
        .take(4)
        .collect();
    dict.set_item("scored", before)?;
    if let Some(top) = scored.first() {
        dict.set_item("top_language", top.language.clone())?;
        let target = scored
            .iter()
            .find(|r| r.encoding.as_deref() == Some("cp1252"));
        if let Some(target) = target {
            let diff = crate::encoding_tables::differing_high_bytes("hp-roman8", "iso8859-1");
            let winner = crate::pipeline::confusion::arbitrate_distinguishing_bytes(
                byte_str,
                "hp-roman8",
                "cp1252",
                &diff,
                Some(&std::collections::HashSet::from([top
                    .language
                    .clone()
                    .unwrap_or_default()])),
                Some(&std::collections::HashSet::from([target
                    .language
                    .clone()
                    .unwrap_or_default()])),
            );
            dict.set_item("arbitration_winner", winner)?;
            dict.set_item(
                "should_demote",
                crate::pipeline::postprocess::should_demote_public(byte_str, top, target),
            )?;
        }
    }
    let demoted = crate::pipeline::postprocess::demote_niche_latin(byte_str, scored);
    let after: Vec<String> = demoted
        .iter()
        .filter_map(|r| r.encoding.clone())
        .take(4)
        .collect();
    dict.set_item("demoted", after)?;
    Ok(dict.into())
}

/// Load the bigram model artifacts.
///
/// # Arguments
///
/// * `models_data` - Raw bytes of `models.bin` (`CMD2`)
/// * `idf_data` - Raw bytes of `idf.bin` (65536 IDF weights)
/// * `rowmax_data` - Raw bytes of `rowmax.bin` (`CRM1`)
/// * `langnorm_data` - Raw bytes of `langnorm.bin` (`CLN1`)
#[pyfunction]
fn _load_models(
    models_data: &[u8],
    idf_data: &[u8],
    rowmax_data: &[u8],
    langnorm_data: &[u8],
) -> PyResult<()> {
    init_models(models_data, idf_data, rowmax_data, langnorm_data).map_err(|e| {
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
    m.add_function(wrap_pyfunction!(run_pipeline, m)?)?;
    m.add_function(wrap_pyfunction!(_debug_declarations, m)?)?;
    m.add_function(wrap_pyfunction!(_debug_utf1632, m)?)?;
    m.add_function(wrap_pyfunction!(_debug_model_names, m)?)?;
    m.add_function(wrap_pyfunction!(_debug_scores, m)?)?;
    m.add_function(wrap_pyfunction!(_debug_demote, m)?)?;
    m.add_function(wrap_pyfunction!(_load_models, m)?)?;
    m.add_function(wrap_pyfunction!(_models_loaded, m)?)?;

    // Add classes
    m.add_class::<UniversalDetector>()?;
    m.add_class::<LanguageFilter>()?;
    m.add_class::<EncodingEra>()?;

    Ok(())
}
