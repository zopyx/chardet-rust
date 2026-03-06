//! Enumerations for chardet.

use pyo3::prelude::*;

/// Bit flags representing encoding eras for filtering detection candidates.
#[pyclass(eq, eq_int, rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncodingEra {
    /// Modern web encodings (UTF-8, Windows-1252, etc.)
    ModernWeb = 1,
    /// Legacy ISO encodings (ISO-8859-*, etc.)
    LegacyIso = 2,
    /// Legacy Mac encodings (MacRoman, etc.)
    LegacyMac = 4,
    /// Legacy regional encodings
    LegacyRegional = 8,
    /// DOS codepages (CP437, etc.)
    Dos = 16,
    /// Mainframe encodings (EBCDIC, etc.)
    Mainframe = 32,
    /// All encodings
    All = 63,
}

#[pymethods]
impl EncodingEra {
    /// Check if this era includes the given era flag.
    pub fn contains(&self, other: &EncodingEra) -> bool {
        (*self as i32 & *other as i32) != 0
    }

    /// Support for `in` operator (x in era)
    pub fn __contains__(&self, other: &EncodingEra) -> bool {
        self.contains(other)
    }

    /// Bitwise AND operation - handles both EncodingEra and int
    pub fn __and__(&self, other: &Bound<'_, PyAny>) -> PyResult<i32> {
        let self_val = *self as i32;

        // Try to extract as EncodingEra first
        if let Ok(era) = other.extract::<EncodingEra>() {
            return Ok(self_val & (era as i32));
        }

        // Then try as int
        if let Ok(val) = other.extract::<i32>() {
            return Ok(self_val & val);
        }

        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "unsupported operand type for &",
        ))
    }

    /// Bitwise OR operation - handles both EncodingEra and int
    pub fn __or__(&self, other: &Bound<'_, PyAny>) -> PyResult<i32> {
        let self_val = *self as i32;

        // Try to extract as EncodingEra first
        if let Ok(era) = other.extract::<EncodingEra>() {
            return Ok(self_val | (era as i32));
        }

        // Then try as int
        if let Ok(val) = other.extract::<i32>() {
            return Ok(self_val | val);
        }

        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "unsupported operand type for |",
        ))
    }

    /// Convert to integer
    pub fn __int__(&self) -> i32 {
        *self as i32
    }

    /// Truthiness - return True if not zero
    pub fn __bool__(&self) -> bool {
        *self as i32 != 0
    }

    /// Return the name of this enum member
    #[getter]
    fn name(&self) -> String {
        format!("{:?}", self)
    }

    /// Return the value of this enum member
    #[getter]
    fn value(&self) -> i32 {
        *self as i32
    }
}

impl Default for EncodingEra {
    fn default() -> Self {
        EncodingEra::All
    }
}

/// Language filter flags for UniversalDetector (chardet 6.x API compat).
///
/// Accepted but not used — the pipeline does not filter by language group.
#[pyclass(eq, eq_int)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum LanguageFilter {
    CHINESE_SIMPLIFIED = 0x01,
    CHINESE_TRADITIONAL = 0x02,
    JAPANESE = 0x04,
    KOREAN = 0x08,
    NON_CJK = 0x10,
    ALL = 0x1F,
    /// Chinese (simplified + traditional)
    CHINESE = 0x03,
    /// CJK (all Chinese, Japanese, Korean)
    CJK = 0x0F,
}

#[pymethods]
impl LanguageFilter {
    /// Bitwise AND operation - handles both LanguageFilter and int
    pub fn __and__(&self, other: &Bound<'_, PyAny>) -> PyResult<i32> {
        let self_val = *self as i32;

        if let Ok(filter) = other.extract::<LanguageFilter>() {
            return Ok(self_val & (filter as i32));
        }

        if let Ok(val) = other.extract::<i32>() {
            return Ok(self_val & val);
        }

        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "unsupported operand type for &",
        ))
    }

    /// Bitwise OR operation - handles both LanguageFilter and int
    pub fn __or__(&self, other: &Bound<'_, PyAny>) -> PyResult<i32> {
        let self_val = *self as i32;

        if let Ok(filter) = other.extract::<LanguageFilter>() {
            return Ok(self_val | (filter as i32));
        }

        if let Ok(val) = other.extract::<i32>() {
            return Ok(self_val | val);
        }

        Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
            "unsupported operand type for |",
        ))
    }

    /// Convert to integer
    pub fn __int__(&self) -> i32 {
        *self as i32
    }
}

impl LanguageFilter {
    /// Get Chinese (simplified + traditional).
    pub const CHINESE: LanguageFilter = LanguageFilter::CHINESE_SIMPLIFIED;

    /// Get CJK (all Chinese, Japanese, Korean).
    pub const CJK: LanguageFilter = LanguageFilter::JAPANESE;
}

impl Default for LanguageFilter {
    fn default() -> Self {
        LanguageFilter::ALL
    }
}
