//! Detection pipeline stages and shared types.

use crate::equivalences::apply_legacy_rename;
use pyo3::prelude::*;
use pyo3::types::PyDict;

pub mod ascii;
pub mod binary;
pub mod bom;
pub mod confusion;
pub mod escape;
pub mod markup;
pub mod orchestrator;
pub mod statistical;
pub mod structural;
pub mod utf1632;
pub mod utf8;
pub mod validity;

/// Confidence for deterministic (non-BOM) detection stages.
pub const DETERMINISTIC_CONFIDENCE: f64 = 0.95;

/// Minimum threshold for filtering results.
pub const MINIMUM_THRESHOLD: f64 = 0.20;

/// Default maximum number of bytes to examine.
pub const DEFAULT_MAX_BYTES: usize = 200_000;

/// A single encoding detection result.
#[derive(Clone, Debug, PartialEq)]
pub struct DetectionResult {
    /// The detected encoding name, or None for binary.
    pub encoding: Option<String>,
    /// Confidence score (0.0 to 1.0).
    pub confidence: f64,
    /// Detected language (ISO 639-1 code), or None.
    pub language: Option<String>,
}

impl DetectionResult {
    /// Create a new detection result.
    pub fn new(encoding: Option<&str>, confidence: f64, language: Option<&str>) -> Self {
        Self {
            encoding: encoding.map(|s| s.to_string()),
            confidence,
            language: language.map(|s| s.to_string()),
        }
    }

    /// Convert to a Python dict.
    pub fn to_py_dict(&self, py: Python, apply_rename: bool) -> PyResult<PyObject> {
        let dict = PyDict::new(py);

        // Handle encoding - None should be Python None, not empty string
        let encoding_obj: PyObject = if let Some(ref enc) = self.encoding {
            let enc_str = if apply_rename {
                apply_legacy_rename(enc)
            } else {
                enc.clone()
            };
            enc_str.into_py(py)
        } else {
            py.None()
        };

        // Handle language - None should be Python None, not empty string
        let language_obj: PyObject = if let Some(ref lang) = self.language {
            lang.clone().into_py(py)
        } else {
            py.None()
        };

        dict.set_item("encoding", encoding_obj)?;
        dict.set_item("confidence", self.confidence)?;
        dict.set_item("language", language_obj)?;

        Ok(dict.into())
    }
}

impl Default for DetectionResult {
    fn default() -> Self {
        Self {
            encoding: None,
            confidence: 0.0,
            language: None,
        }
    }
}

/// Per-run mutable state for a single pipeline invocation.
#[derive(Default)]
pub struct PipelineContext {
    /// Cache for analysis results.
    pub analysis_cache: std::collections::HashMap<String, (f64, usize, usize)>,
    /// Pre-computed non-ASCII byte count.
    pub non_ascii_count: Option<usize>,
    /// Multi-byte structural scores.
    pub mb_scores: std::collections::HashMap<String, f64>,
    /// Multi-byte byte coverage scores.
    pub mb_coverage: std::collections::HashMap<String, f64>,
}

impl PipelineContext {
    /// Create a new pipeline context.
    pub fn new() -> Self {
        Self::default()
    }
}
