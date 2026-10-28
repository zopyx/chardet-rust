//! Stage 0d: magic number detection for known binary file types.
//!
//! Recognized binary formats answer immediately with `encoding = None` and the
//! format's MIME type, before any text analysis runs.  Mirrors
//! `chardet/pipeline/magic.py`.

use crate::pipeline::DetectionResult;

/// Confidence reported for a deterministic binary identification.
const DETECTION_CONFIDENCE: f64 = 1.0;

/// Binary format result (encoding is `None`).
fn make_result(mime: &str) -> DetectionResult {
    DetectionResult::new_with_mime(None, DETECTION_CONFIDENCE, None, Some(mime))
}

/// Formats recognized by a prefix at offset 0.
///
/// Longest prefix first, so a short signature cannot shadow a longer one.
const MAGIC_NUMBERS: &[(&[u8], &str)] = &[
    // Images
    (b"\x89PNG\r\n\x1a\n", "image/png"),
    (b"GIF87a", "image/gif"),
    (b"GIF89a", "image/gif"),
    (b"MM\x00\x2a", "image/tiff"),
    (b"II\x2a\x00", "image/tiff"),
    (b"8BPS", "image/vnd.adobe.photoshop"),
    (b"qoif", "image/qoi"),
    (b"BM", "image/bmp"),
    (b"\xff\xd8\xff", "image/jpeg"),
    (
        b"\x00\x00\x00\x0c\x4a\x58\x4c\x20\x0d\x0a\x87\x0a",
        "image/jxl",
    ),
    (b"\xff\x0a", "image/jxl"),
    (b"\x00\x00\x01\x00", "image/vnd.microsoft.icon"),
    // Audio / video
    (b"ID3", "audio/mpeg"),
    (b"MThd", "audio/midi"),
    (b"OggS", "audio/ogg"),
    (b"fLaC", "audio/flac"),
    (b"\x1a\x45\xdf\xa3", "video/webm"),
    // Archives
    (b"\x1f\x8b", "application/gzip"),
    (b"BZh", "application/x-bzip2"),
    (b"\xfd7zXZ\x00", "application/x-xz"),
    (b"7z\xbc\xaf\x27\x1c", "application/x-7z-compressed"),
    (b"Rar!\x1a\x07\x01\x00", "application/vnd.rar"),
    (b"Rar!\x1a\x07\x00", "application/vnd.rar"),
    (b"\x28\xb5\x2f\xfd", "application/zstd"),
    // Documents / data
    (b"%PDF-", "application/pdf"),
    (b"SQLite format 3\x00", "application/x-sqlite3"),
    (b"ARROW1", "application/vnd.apache.arrow.file"),
    (b"PAR1", "application/vnd.apache.parquet"),
    (b"\x00asm", "application/wasm"),
    // Executables / bytecode
    (b"dex\n", "application/vnd.android.dex"),
    (b"\x7fELF", "application/x-elf"),
    (b"\xfe\xed\xfa\xce", "application/x-mach-binary"),
    (b"\xfe\xed\xfa\xcf", "application/x-mach-binary"),
    (b"\xce\xfa\xed\xfe", "application/x-mach-binary"),
    (b"\xcf\xfa\xed\xfe", "application/x-mach-binary"),
    (b"MZ", "application/vnd.microsoft.portable-executable"),
    // Fonts
    (b"wOFF", "font/woff"),
    (b"wOF2", "font/woff2"),
    (b"OTTO", "font/otf"),
    (b"\x00\x01\x00\x00", "font/ttf"),
];

/// TAR archives carry "ustar" at offset 257.
const TAR_OFFSET: usize = 257;
const TAR_SIGNATURES: &[&[u8]] = &[b"ustar\x00", b"ustar "];

/// RIFF container subtypes, from bytes 8-11.
const RIFF_SUBTYPES: &[(&[u8], &str)] = &[
    (b"WEBP", "image/webp"),
    (b"WAVE", "audio/wav"),
    (b"AVI ", "video/x-msvideo"),
];

/// FORM container subtypes (AIFF), same layout as RIFF.
const FORM_SUBTYPES: &[(&[u8], &str)] = &[(b"AIFF", "audio/aiff"), (b"AIFC", "audio/aiff")];

const ZIP_SIGNATURE: &[u8] = b"PK\x03\x04";
const ZIP_SCAN_LIMIT: usize = 4096;

/// ZIP entry filename prefix -> MIME type.
const ZIP_FILENAME_PREFIXES: &[(&[u8], &str)] = &[
    (
        b"xl/",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    (
        b"word/",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    (
        b"ppt/",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ),
    (b"META-INF/MANIFEST.MF", "application/java-archive"),
    (
        b"AndroidManifest.xml",
        "application/vnd.android.package-archive",
    ),
    (b"META-INF/container.xml", "application/epub+zip"),
];

/// ZIP entry filename substring -> MIME type.
const ZIP_FILENAME_SUFFIXES: &[(&[u8], &str)] = &[(b".dist-info/", "application/x-wheel+zip")];

/// OpenDocument MIME types found in a `mimetype` entry.
const OPENDOCUMENT_MIMES: &[&[u8]] = &[
    b"application/vnd.oasis.opendocument.text",
    b"application/vnd.oasis.opendocument.spreadsheet",
    b"application/vnd.oasis.opendocument.presentation",
    b"application/vnd.oasis.opendocument.graphics",
];

const FTYP_MARKER: &[u8] = b"ftyp";
const FTYP_OFFSET: usize = 4;

const FTYP_AVIF_BRANDS: &[&[u8]] = &[b"avif", b"avis"];
const FTYP_HEIC_BRANDS: &[&[u8]] = &[b"heic", b"heix"];
const FTYP_HEIF_BRANDS: &[&[u8]] = &[b"mif1", b"msf1"];
const FTYP_AUDIO_BRANDS: &[&[u8]] = &[b"M4A ", b"M4B ", b"F4A "];
const FTYP_QUICKTIME_BRANDS: &[&[u8]] = &[b"qt  "];

const CAFEBABE: &[u8] = b"\xca\xfe\xba\xbe";
const CAFEBABE_MAX_FAT_ARCHES: u32 = 20;

fn u16_le(data: &[u8], offset: usize) -> Option<u16> {
    let slice = data.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_le(data: &[u8], offset: usize) -> Option<u32> {
    let slice = data.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

/// Classify a ZIP container from its entry filenames and the `mimetype` entry.
fn classify_zip(data: &[u8]) -> String {
    let scan = &data[..data.len().min(ZIP_SCAN_LIMIT)];
    let mut offset = 0usize;

    while let Some(idx) = find_from(scan, ZIP_SIGNATURE, offset) {
        if scan.len() < idx + 30 {
            break;
        }
        let name_len = match u16_le(scan, idx + 26) {
            Some(value) => value as usize,
            None => break,
        };
        let extra_len = match u16_le(scan, idx + 28) {
            Some(value) => value as usize,
            None => break,
        };
        let name_start = idx + 30;
        if scan.len() < name_start + name_len {
            break;
        }
        let name = &scan[name_start..name_start + name_len];

        for (prefix, mime) in ZIP_FILENAME_PREFIXES {
            if name.starts_with(prefix) {
                return (*mime).to_string();
            }
        }
        for (suffix, mime) in ZIP_FILENAME_SUFFIXES {
            if contains(name, suffix) {
                return (*mime).to_string();
            }
        }

        // OpenDocument: a stored "mimetype" entry whose content names the type.
        if name == b"mimetype" {
            let compression = u16_le(scan, idx + 8).unwrap_or(1);
            if compression == 0 {
                let content_start = name_start + name_len + extra_len;
                let content_len = u32_le(scan, idx + 22).unwrap_or(0) as usize;
                if scan.len() >= content_start + content_len {
                    let content = &scan[content_start..content_start + content_len];
                    if OPENDOCUMENT_MIMES.contains(&content) {
                        return match std::str::from_utf8(content) {
                            Ok(mime) => mime.to_string(),
                            Err(_) => "application/zip".to_string(),
                        };
                    }
                }
            }
        }

        let flags = u16_le(scan, idx + 6).unwrap_or(0);
        let content_size = if flags & 0x0008 != 0 {
            0
        } else {
            u32_le(scan, idx + 18).unwrap_or(0) as usize
        };
        let next = name_start + name_len + extra_len + content_size;
        if next <= offset {
            break;
        }
        offset = next;
    }

    "application/zip".to_string()
}

fn find_from(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= haystack.len() || needle.is_empty() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|pos| pos + from)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    find_from(haystack, needle, 0).is_some()
}

/// Check *data* for known binary file magic numbers.
pub fn detect_magic(data: &[u8]) -> Option<DetectionResult> {
    if data.is_empty() {
        return None;
    }

    // ftyp box (MP4/MOV/HEIC/AVIF): bytes 0-3 are the box size.  The size
    // bounds keep ASCII text ("The " -> 0x54686520) from matching.
    if data.len() >= 12 && &data[FTYP_OFFSET..FTYP_OFFSET + 4] == FTYP_MARKER {
        let box_size = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        if box_size >= 8 && box_size as usize <= data.len() {
            let brand = &data[8..12];
            if FTYP_AVIF_BRANDS.contains(&brand) {
                return Some(make_result("image/avif"));
            }
            if FTYP_HEIC_BRANDS.contains(&brand) {
                return Some(make_result("image/heic"));
            }
            if FTYP_HEIF_BRANDS.contains(&brand) {
                return Some(make_result("image/heif"));
            }
            if FTYP_AUDIO_BRANDS.contains(&brand) {
                return Some(make_result("audio/mp4"));
            }
            if FTYP_QUICKTIME_BRANDS.contains(&brand) {
                return Some(make_result("video/quicktime"));
            }
            return Some(make_result("video/mp4"));
        }
    }

    if data.len() >= 12 && &data[..4] == b"RIFF" {
        for (subtype, mime) in RIFF_SUBTYPES {
            if &data[8..12] == *subtype {
                return Some(make_result(mime));
            }
        }
    }

    if data.len() >= 12 && &data[..4] == b"FORM" {
        for (subtype, mime) in FORM_SUBTYPES {
            if &data[8..12] == *subtype {
                return Some(make_result(mime));
            }
        }
    }

    if data.starts_with(ZIP_SIGNATURE) {
        let mime = classify_zip(data);
        return Some(make_result(&mime));
    }

    // Java class files and Mach-O fat binaries share \xca\xfe\xba\xbe.
    if data.len() >= 8 && &data[..4] == CAFEBABE {
        let nfat_arch = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        if nfat_arch <= CAFEBABE_MAX_FAT_ARCHES {
            return Some(make_result("application/x-mach-binary"));
        }
        return Some(make_result("application/java-vm"));
    }

    for (prefix, mime) in MAGIC_NUMBERS {
        if data.starts_with(prefix) {
            return Some(make_result(mime));
        }
    }

    if data.len() >= TAR_OFFSET + 6 {
        let signature = &data[TAR_OFFSET..TAR_OFFSET + 6];
        if TAR_SIGNATURES.contains(&signature) {
            return Some(make_result("application/x-tar"));
        }
    }

    None
}
