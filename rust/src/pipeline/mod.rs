//! Detection pipeline stages and shared types.
//!
//! This module contains the multi-stage encoding detection pipeline.
//! Each stage progressively narrows down the possible encodings,
//! from cheap early checks to expensive statistical analysis.
//!
//! # Pipeline Architecture
//!
//! The pipeline is organized into stages that run in sequence:
//!
//! ## Stage 0: Early Deterministic Detection
//! - `bom`: Byte Order Mark detection (UTF-8/16/32)
//! - `utf1632`: UTF-16/32 pattern detection without BOM
//! - `binary`: Binary file detection
//! - `escape`: Escape sequence encodings (ISO-2022, HZ-GB-2312, UTF-7)
//!
//! ## Stage 1: Markup and Basic Text
//! - `markup`: HTML/XML charset extraction
//! - `ascii`: Pure ASCII detection
//! - `utf8`: UTF-8 validation
//!
//! ## Stage 2: Structural Analysis
//! - `validity`: Byte sequence validity filtering
//! - `structural`: CJK multi-byte structural probing
//!
//! ## Stage 3: Statistical Analysis
//! - `statistical`: Bigram model scoring
//! - `confusion`: Confusion group resolution
//!
//! # Pipeline Orchestration
//!
//! The `orchestrator` module coordinates the stages, passing data through
//! each stage in order and handling early exits for deterministic detections.

use crate::equivalences::apply_legacy_rename;

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
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

/// Confidence value for deterministic (non-BOM) detection stages.
///
/// When we detect an encoding through structural analysis (not just BOM),
/// we use this confidence level. It's high but not absolute (1.0) to
/// acknowledge that edge cases might exist.
pub const DETERMINISTIC_CONFIDENCE: f64 = 0.95;

/// Minimum threshold for filtering results.
///
/// Results below this confidence threshold are typically filtered out
/// unless the caller explicitly requests all results.
pub const MINIMUM_THRESHOLD: f64 = 0.20;

/// Default maximum number of bytes to examine.
///
/// Limiting analysis to the first 200KB provides a good balance between
/// accuracy and performance. Most encoding signatures are detectable
/// within this limit.
pub const DEFAULT_MAX_BYTES: usize = 200_000;

/// A single encoding detection result.
///
/// This struct represents the outcome of a detection attempt, containing
/// the detected encoding name, confidence score, and optional language code.
#[derive(Clone, Debug, PartialEq)]
pub struct DetectionResult {
    /// The detected encoding name, or None for binary content.
    ///
    /// A `None` encoding indicates that the content was detected as binary
    /// rather than text.
    pub encoding: Option<String>,
    
    /// Confidence score from 0.0 to 1.0.
    ///
    /// - 1.0: Absolute confidence (e.g., BOM detection)
    /// - 0.95: High confidence (structural detection)
    /// - <0.20: Low confidence (statistical guess)
    pub confidence: f64,
    
    /// Detected language as ISO 639-1 code, or None.
    ///
    /// Examples: "en", "ru", "ja", "zh"
    pub language: Option<String>,
}

impl DetectionResult {
    /// Create a new detection result.
    ///
    /// # Arguments
    ///
    /// * `encoding` - The encoding name, or None for binary
    /// * `confidence` - Confidence score from 0.0 to 1.0
    /// * `language` - ISO 639-1 language code, or None
    ///
    /// # Examples
    ///
    /// ```
    /// use _chardet_rs::pipeline::DetectionResult;
    ///
    /// let result = DetectionResult::new(Some("utf-8"), 0.99, None);
    /// assert_eq!(result.encoding.as_deref(), Some("utf-8"));
    /// assert_eq!(result.confidence, 0.99);
    /// ```
    pub fn new(encoding: Option<&str>, confidence: f64, language: Option<&str>) -> Self {
        Self {
            encoding: encoding.map(|s| s.to_string()),
            confidence,
            language: language.map(|s| s.to_string()),
        }
    }

    /// Convert the detection result to a Python dictionary.
    ///
    /// # Arguments
    ///
    /// * `py` - Python interpreter handle
    /// * `apply_rename` - Whether to apply legacy-to-modern encoding name remapping
    ///
    /// # Returns
    ///
    /// A PyObject containing a Python dict with keys:
    /// - "encoding": str or None
    /// - "confidence": float
    /// - "language": str or None
    ///
    /// # Note
    ///
    /// Uses the deprecated `into_py` method for compatibility with
    /// the current PyO3 version.
    #[cfg(feature = "python")]
    #[allow(deprecated)]
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
///
/// This context object is passed through the pipeline to cache intermediate
/// results and avoid redundant computations.
#[derive(Default)]
pub struct PipelineContext {
    /// Cache for structural analysis results.
    ///
    /// Maps encoding name to (valid_ratio, mb_byte_count, lead_diversity).
    pub analysis_cache: std::collections::HashMap<String, (f64, usize, usize)>,
    
    /// Pre-computed non-ASCII byte count.
    ///
    /// Computed once and cached to avoid scanning the data multiple times.
    pub non_ascii_count: Option<usize>,
    
    /// Multi-byte structural scores by encoding name.
    ///
    /// Caches the structural score for each multi-byte encoding.
    pub mb_scores: std::collections::HashMap<String, f64>,
    
    /// Multi-byte byte coverage scores by encoding name.
    ///
    /// Caches the byte coverage score for each multi-byte encoding.
    pub mb_coverage: std::collections::HashMap<String, f64>,
}

impl PipelineContext {
    /// Create a new pipeline context.
    ///
    /// Initializes empty caches for all computed values.
    ///
    /// # Examples
    ///
    /// ```
    /// use _chardet_rs::pipeline::PipelineContext;
    ///
    /// let ctx = PipelineContext::new();
    /// assert!(ctx.analysis_cache.is_empty());
    /// ```
    pub fn new() -> Self {
        Self::default()
    }
}
