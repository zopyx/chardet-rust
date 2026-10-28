//! Universal character encoding detector - Rust implementation
//!
//! This is a port of the chardet Python library to Rust with Python bindings.

pub mod bigram_models;
pub mod detector;
pub mod encoding_tables;
pub mod enums;
pub mod equivalences;
pub mod equivalences_full;
pub mod models;
pub mod pipeline;
pub mod registry;

// Re-export core types for convenience
pub use detector::{detect_all_bytes, detect_bytes};
pub use enums::EncodingEra;
pub use pipeline::DetectionResult;

// Python bindings - only compiled when "python" feature is enabled
#[cfg(feature = "python")]
mod py;

#[cfg(feature = "python")]
pub use py::{detect, detect_all};
