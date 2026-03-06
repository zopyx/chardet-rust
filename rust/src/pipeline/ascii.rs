//! Stage 1c: Pure ASCII detection.

use super::DetectionResult;

/// Check if all bytes are printable ASCII plus common whitespace.
pub fn detect_ascii(data: &[u8]) -> Option<DetectionResult> {
    if data.is_empty() {
        return None;
    }

    // Check if all bytes are allowed ASCII:
    // tab (0x09), newline (0x0A), carriage return (0x0D),
    // and printable ASCII (0x20-0x7E)
    let is_ascii = data
        .iter()
        .all(|&b| b == 0x09 || b == 0x0A || b == 0x0D || (b >= 0x20 && b <= 0x7E));

    if is_ascii {
        Some(DetectionResult::new(Some("ascii"), 1.0, None))
    } else {
        None
    }
}
