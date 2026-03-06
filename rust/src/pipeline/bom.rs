//! Stage 1a: BOM (Byte Order Mark) detection.

use super::DetectionResult;

// Ordered longest-first so UTF-32 is checked before UTF-16
const BOMS: &[(&[u8], &str)] = &[
    (b"\x00\x00\xfe\xff", "utf-32-be"),
    (b"\xff\xfe\x00\x00", "utf-32-le"),
    (b"\xef\xbb\xbf", "utf-8-sig"),
    (b"\xfe\xff", "utf-16-be"),
    (b"\xff\xfe", "utf-16-le"),
];

/// Check for a byte order mark at the start of data.
pub fn detect_bom(data: &[u8]) -> Option<DetectionResult> {
    for (bom_bytes, encoding) in BOMS {
        if data.starts_with(bom_bytes) {
            // UTF-32 BOMs overlap with UTF-16 BOMs (e.g. FF FE 00 00 starts
            // with the UTF-16-LE BOM FF FE). Validate that the payload after
            // a UTF-32 BOM is a valid number of UTF-32 code units (multiple of 4 bytes).
            if bom_bytes.len() == 4 {
                let payload_len = data.len() - bom_bytes.len();
                if payload_len % 4 != 0 {
                    continue;
                }
            }
            return Some(DetectionResult::new(Some(encoding), 1.0, None));
        }
    }
    None
}
