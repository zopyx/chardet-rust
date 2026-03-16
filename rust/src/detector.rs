//! Core detection APIs and streaming detector state.

#[cfg(any(feature = "python", test))]
use std::fmt;

use crate::enums::EncodingEra;
use crate::pipeline::orchestrator::run_pipeline;
use crate::pipeline::{DetectionResult, MINIMUM_THRESHOLD};

/// Maximum allowed value for `max_bytes` parameter (100 MB).
///
/// Prevents memory exhaustion attacks via excessive buffer allocation.
#[cfg(any(feature = "python", test))]
pub(crate) const MAX_BYTES_LIMIT: usize = 100 * 1024 * 1024;

/// Maximum number of `feed()` calls allowed per detector instance.
///
/// Prevents denial-of-service via excessive iteration.
#[cfg(any(feature = "python", test))]
pub(crate) const MAX_FEED_CALLS: usize = 1_000_000;

/// Maximum size of an individual `feed()` input (50 MB).
///
/// The detector still enforces `max_bytes`, but this guards against
/// oversized single-chunk inputs.
#[cfg(any(feature = "python", test))]
pub(crate) const MAX_FEED_SIZE: usize = 50 * 1024 * 1024;

/// Errors raised by the streaming detector.
#[cfg(any(feature = "python", test))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StreamingDetectorError {
    /// The caller provided `max_bytes = 0`.
    ZeroMaxBytes,
    /// The caller provided a `max_bytes` value above the hard limit.
    MaxBytesLimitExceeded {
        max_bytes: usize,
        max_allowed: usize,
    },
    /// `feed()` was called after `close()` without `reset()`.
    FeedAfterClose,
    /// The detector exceeded its feed call limit.
    MaxFeedCallsExceeded { max_feed_calls: usize },
    /// A single `feed()` call exceeded the per-call size limit.
    FeedTooLarge { input_size: usize, max_size: usize },
}

#[cfg(any(feature = "python", test))]
impl fmt::Display for StreamingDetectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroMaxBytes => write!(f, "max_bytes must be a positive integer"),
            Self::MaxBytesLimitExceeded {
                max_bytes,
                max_allowed,
            } => write!(
                f,
                "max_bytes ({}) exceeds maximum allowed value ({})",
                max_bytes, max_allowed
            ),
            Self::FeedAfterClose => write!(f, "feed() called after close() without reset()"),
            Self::MaxFeedCallsExceeded { max_feed_calls } => write!(
                f,
                "Maximum feed() calls ({}) exceeded. Call reset() to start a new detection.",
                max_feed_calls
            ),
            Self::FeedTooLarge {
                input_size,
                max_size,
            } => write!(
                f,
                "feed() input size ({}) exceeds maximum ({})",
                input_size, max_size
            ),
        }
    }
}

/// Validate `max_bytes` parameter.
#[cfg(any(feature = "python", test))]
pub(crate) fn validate_max_bytes(max_bytes: usize) -> Result<(), StreamingDetectorError> {
    if max_bytes == 0 {
        return Err(StreamingDetectorError::ZeroMaxBytes);
    }
    if max_bytes > MAX_BYTES_LIMIT {
        return Err(StreamingDetectorError::MaxBytesLimitExceeded {
            max_bytes,
            max_allowed: MAX_BYTES_LIMIT,
        });
    }
    Ok(())
}

/// Stateful streaming detector shared by Python-facing wrappers.
#[cfg(any(feature = "python", test))]
#[derive(Debug)]
pub(crate) struct StreamingDetector {
    encoding_era: EncodingEra,
    max_bytes: usize,
    buffer: Vec<u8>,
    done: bool,
    closed: bool,
    result: Option<DetectionResult>,
    feed_count: usize,
    max_feed_calls: usize,
}

#[cfg(any(feature = "python", test))]
impl StreamingDetector {
    /// Create a new streaming detector.
    pub(crate) fn new(
        encoding_era: EncodingEra,
        max_bytes: usize,
    ) -> Result<Self, StreamingDetectorError> {
        validate_max_bytes(max_bytes)?;

        Ok(Self {
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

    /// Feed a chunk of bytes into the detector.
    pub(crate) fn feed(&mut self, byte_str: &[u8]) -> Result<(), StreamingDetectorError> {
        if self.closed {
            return Err(StreamingDetectorError::FeedAfterClose);
        }

        if self.done {
            return Ok(());
        }

        if self.feed_count >= self.max_feed_calls {
            return Err(StreamingDetectorError::MaxFeedCallsExceeded {
                max_feed_calls: self.max_feed_calls,
            });
        }

        if byte_str.len() > MAX_FEED_SIZE {
            return Err(StreamingDetectorError::FeedTooLarge {
                input_size: byte_str.len(),
                max_size: MAX_FEED_SIZE,
            });
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

    /// Finalize detection and return the cached best result.
    pub(crate) fn close(&mut self) -> &DetectionResult {
        if !self.closed {
            self.closed = true;
            self.done = true;
        }

        self.result
            .get_or_insert_with(|| detect_bytes(&self.buffer, self.encoding_era, self.max_bytes))
    }

    /// Reset detector state for reuse.
    pub(crate) fn reset(&mut self) {
        self.buffer.clear();
        self.done = false;
        self.closed = false;
        self.result = None;
        self.feed_count = 0;
    }

    /// Whether the detector has enough bytes or has been closed.
    #[cfg(feature = "python")]
    pub(crate) fn done(&self) -> bool {
        self.done
    }

    /// Return the cached result, if detection has been finalized.
    #[cfg(feature = "python")]
    pub(crate) fn result(&self) -> Option<&DetectionResult> {
        self.result.as_ref()
    }
}

/// Detect the encoding of a byte string.
pub fn detect_bytes(data: &[u8], encoding_era: EncodingEra, max_bytes: usize) -> DetectionResult {
    run_pipeline(data, encoding_era, max_bytes)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Detect all possible encodings of the given byte string.
pub fn detect_all_bytes(
    data: &[u8],
    encoding_era: EncodingEra,
    max_bytes: usize,
    ignore_threshold: bool,
) -> Vec<DetectionResult> {
    let results = run_pipeline(data, encoding_era, max_bytes);

    if !ignore_threshold {
        let filtered: Vec<_> = results
            .iter()
            .filter(|r| r.confidence > MINIMUM_THRESHOLD)
            .cloned()
            .collect();

        if !filtered.is_empty() {
            return filtered;
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_detector_should_truncate_input_to_max_bytes() {
        let mut detector = StreamingDetector::new(EncodingEra::All, 10).unwrap();
        let data = b"1234567890+ZeVnLIqe-";

        detector.feed(data).unwrap();
        let result = detector.close().clone();
        let expected = detect_bytes(&data[..10], EncodingEra::All, 10);

        assert_eq!(result, expected);
    }

    #[test]
    fn reset_should_clear_feed_count_and_allow_reuse() {
        let mut detector = StreamingDetector::new(EncodingEra::All, 32).unwrap();
        detector.max_feed_calls = 2;

        detector.feed(b"one").unwrap();
        detector.feed(b"two").unwrap();
        assert_eq!(detector.feed_count, 2);

        detector.reset();
        assert_eq!(detector.feed_count, 0);

        detector.feed(b"three").unwrap();
        assert_eq!(detector.feed_count, 1);
    }
}
