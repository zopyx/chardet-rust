//! Stage 0: Binary content detection.

/// Threshold: if more than this fraction of bytes are binary indicators, it's binary.
const BINARY_THRESHOLD: f64 = 0.01;

/// Check for common binary file signatures
pub fn has_binary_signature(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }

    // Common binary file signatures (magic numbers)
    match &data[..4] {
        // PNG
        [0x89, 0x50, 0x4E, 0x47] => true,
        // GIF
        [0x47, 0x49, 0x46, 0x38] => true,
        // JPEG
        [0xFF, 0xD8, 0xFF, _] => true,
        // ZIP / Office documents
        [0x50, 0x4B, 0x03, 0x04] => true,
        // PDF
        [0x25, 0x50, 0x44, 0x46] => true,
        // RAR
        [0x52, 0x61, 0x72, 0x21] => true,
        // 7z
        [0x37, 0x7A, 0xBC, 0xAF] => true,
        // MP3 (ID3)
        [0x49, 0x44, 0x33, _] => true,
        // MP4 (ftyp)
        [_, _, _, _] if data.len() > 8 && &data[4..8] == b"ftyp" => true,
        _ => false,
    }
}

/// Check if data appears to be binary (not text) content.
pub fn is_binary(data: &[u8], max_bytes: usize) -> bool {
    let data = &data[..data.len().min(max_bytes)];
    if data.is_empty() {
        return false;
    }

    // First check for binary file signatures
    if has_binary_signature(data) {
        return true;
    }

    // Count binary-indicator control bytes (0x00-0x08, 0x0E-0x1F — excludes \t \n \v \f \r)
    // Also excludes 0x0E (SO), 0x0F (SI), 0x1B (ESC) used by ISO-2022 escape sequences
    // Also count null bytes (0x00) specifically
    let null_count: usize = data.iter().filter(|&&b| b == 0x00).count();
    let control_count: usize = data
        .iter()
        .filter(|&&b| (b <= 0x08) || (b >= 0x10 && b <= 0x1A) || (b >= 0x1C && b <= 0x1F))
        .count();

    let binary_count = null_count + control_count;

    // High proportion of null bytes is a strong indicator of binary
    if null_count > 0 && (null_count as f64) / (data.len() as f64) > 0.01 {
        return true;
    }

    (binary_count as f64) / (data.len() as f64) > BINARY_THRESHOLD
}
