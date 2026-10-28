//! Stage 1b: charset declarations in markup (and Python source cookies).
//!
//! The stage owns the whole declaration decision: it walks the declarations
//! the parser yields, stops at the first whose name (or Windows superset)
//! decodes the window, answers with it when the caller's filters allow it and
//! with nothing when they do not, and yields to the UTF-8 precheck when the
//! data has genuine multi-byte UTF-8 structure.  Mirrors
//! `chardet/pipeline/markup.py`.

use crate::pipeline::structural::compute_structural_score;
use crate::pipeline::{DetectionResult, PipelineContext, DETERMINISTIC_CONFIDENCE};
use crate::registry::{lookup_encoding, REGISTRY};
use std::collections::HashSet;

/// How much of the data the declaration scan reads.
const SCAN_LIMIT: usize = 4096;

/// Labels that cannot describe a document whose tag was read as ASCII bytes.
const SELF_CONTRADICTING_LABELS: &[&str] = &[
    "utf-16",
    "utf-16-be",
    "utf-16-le",
    "utf-32",
    "utf-32-be",
    "utf-32-le",
];

/// One declaration found in the head, with the MIME type of its construct.
pub struct Declaration {
    /// Registry name the label resolves to, if any.
    pub encoding: Option<String>,
    /// MIME type of the construct the declaration sat in.
    pub mime_type: &'static str,
}

/// What one declaration says about the window.
struct Verdict {
    answer: Option<String>,
    settled: bool,
}

/// The codec the *reported* name of a declared CJK encoding resolves to.
fn display_codec(encoding: &str) -> &'static str {
    match encoding {
        "shift_jis_2004" => "shift_jis",
        "euc_jis_2004" => "euc_jp",
        "iso2022_jp_2" => "iso2022_jp",
        "big5hkscs" => "big5",
        _ => "",
    }
}

/// Windows superset used for markup promotion, limited to CJK encodings.
fn markup_promotion(encoding: &str) -> Option<(&'static str, &'static str)> {
    let info = REGISTRY.get(encoding)?;
    if !info.is_multibyte_cjk {
        return None;
    }
    let superset = info.superset?;
    let reported = display_codec(encoding);
    Some((
        if reported.is_empty() {
            info.name
        } else {
            reported
        },
        superset,
    ))
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

fn eq_ignore_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() == needle.len()
        && haystack
            .iter()
            .zip(needle)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

fn find_ignore_case(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.len() > haystack.len() || from >= haystack.len() {
        return None;
    }
    (from..=haystack.len() - needle.len())
        .find(|&i| eq_ignore_case(&haystack[i..i + needle.len()], needle))
}

/// Return *head* without its `<!-- -->` comments.
fn strip_comments(head: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(head.len());
    let mut pos = 0usize;
    loop {
        match find_ignore_case(head, b"<!--", pos) {
            Some(start) => {
                out.extend_from_slice(&head[pos..start]);
                // The dashes that open a comment may also close it.
                match find_ignore_case(head, b"-->", start + 2) {
                    Some(end) => pos = end + 3,
                    None => return out,
                }
            }
            None => {
                out.extend_from_slice(&head[pos..]);
                return out;
            }
        }
    }
}

/// Labels the WHATWG prescan rewrites rather than honours.
fn markup_encoding(raw_name: &str) -> Option<String> {
    let lowered = raw_name.trim().to_lowercase();
    if lowered == "x-user-defined" {
        return Some("cp1252".to_string());
    }
    let resolved = lookup_encoding(raw_name)?;
    if SELF_CONTRADICTING_LABELS.contains(&resolved) {
        return Some("utf-8".to_string());
    }
    Some(resolved.to_string())
}

/// Parse one meta tag body into lowercased attribute name/value pairs.
fn meta_attributes(body: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut attrs: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut i = 0usize;
    while i < body.len() {
        // Skip separators.
        while i < body.len() && (is_space(body[i]) || body[i] == b'/') {
            i += 1;
        }
        if i >= body.len() {
            break;
        }
        let name_start = i;
        while i < body.len() && !is_space(body[i]) && !matches!(body[i], b'=' | b'/' | b'>') {
            i += 1;
        }
        let mut name: Vec<u8> = body[name_start..i].to_vec();
        name.make_ascii_lowercase();
        while i < body.len() && is_space(body[i]) {
            i += 1;
        }

        let mut value: Vec<u8> = Vec::new();
        if i < body.len() && body[i] == b'=' {
            i += 1;
            while i < body.len() && is_space(body[i]) {
                i += 1;
            }
            if i < body.len() && (body[i] == b'"' || body[i] == b'\'') {
                let quote = body[i];
                i += 1;
                let start = i;
                while i < body.len() && body[i] != quote {
                    i += 1;
                }
                value = body[start..i].to_vec();
                if i < body.len() {
                    i += 1;
                }
            } else {
                let start = i;
                while i < body.len() && !is_space(body[i]) && body[i] != b'>' {
                    i += 1;
                }
                value = body[start..i].to_vec();
            }
        }

        if !name.is_empty() && !attrs.iter().any(|(existing, _)| *existing == name) {
            attrs.push((name, value));
        }
    }
    attrs
}

fn attr<'a>(attrs: &'a [(Vec<u8>, Vec<u8>)], name: &[u8]) -> Option<&'a [u8]> {
    attrs
        .iter()
        .find(|(key, _)| key.as_slice() == name)
        .map(|(_, value)| value.as_slice())
}

/// Extract the `charset=` value from a `content` attribute.
fn content_charset(content: &[u8]) -> Option<Vec<u8>> {
    let mut pos = 0usize;
    while let Some(idx) = find_ignore_case(content, b"charset", pos) {
        let mut i = idx + 7;
        while i < content.len() && is_space(content[i]) {
            i += 1;
        }
        if i < content.len() && content[i] == b'=' {
            i += 1;
            while i < content.len() && is_space(content[i]) {
                i += 1;
            }
            if i < content.len() && (content[i] == b'"' || content[i] == b'\'') {
                let quote = content[i];
                i += 1;
                let start = i;
                while i < content.len() && content[i] != quote {
                    i += 1;
                }
                return Some(content[start..i].to_vec());
            }
            let start = i;
            while i < content.len()
                && !is_space(content[i])
                && !matches!(content[i], b';' | b'"' | b'\'')
            {
                i += 1;
            }
            return Some(content[start..i].to_vec());
        }
        pos = idx + 7;
    }
    None
}

/// Charset labels from real meta declarations, in document order.
fn html_charsets(head: &[u8]) -> Vec<Vec<u8>> {
    let mut labels: Vec<Vec<u8>> = Vec::new();
    let mut pos = 0usize;
    while let Some(idx) = find_ignore_case(head, b"<meta", pos) {
        let mut i = idx + 5;
        let after_meta_ok =
            i >= head.len() || is_space(head[i]) || head[i] == b'/' || head[i] == b'>';
        if !after_meta_ok {
            pos = i;
            continue;
        }
        let body_start = i;
        while i < head.len() && head[i] != b'>' && head[i] != b'<' {
            i += 1;
        }
        if i >= head.len() || head[i] != b'>' {
            pos = i;
            continue;
        }
        let attrs = meta_attributes(&head[body_start..i]);
        pos = i + 1;

        if let Some(charset) = attr(&attrs, b"charset") {
            let trimmed: Vec<u8> = charset.to_vec();
            if !trimmed.iter().all(|&b| is_space(b)) && !trimmed.is_empty() {
                labels.push(trimmed);
                continue;
            }
        }
        let equiv = attr(&attrs, b"http-equiv").or_else(|| attr(&attrs, b"name"));
        if let (Some(equiv), Some(content)) = (equiv, attr(&attrs, b"content")) {
            let mut lowered = equiv.to_vec();
            lowered.make_ascii_lowercase();
            let mut end = 0usize;
            while end < lowered.len() && !is_space(lowered[end]) {
                end += 1;
            }
            if &lowered[..end] == b"content-type" {
                if let Some(label) = content_charset(content) {
                    labels.push(label);
                }
            }
        }
    }
    labels
}

/// `<?xml ... encoding="..."?>` from the head, if any.
fn xml_encoding(head: &[u8]) -> Option<&[u8]> {
    let start = find_ignore_case(head, b"<?xml", 0)?;
    let mut i = start + 5;
    // Require whitespace after the name, then a tag body without angle brackets.
    if i >= head.len() || !is_space(head[i]) {
        return None;
    }
    let body_start = i;
    while i < head.len() && head[i] != b'<' && head[i] != b'>' {
        i += 1;
    }
    let body = &head[body_start..i];

    let idx = find_ignore_case(body, b"encoding", 0)?;
    let mut k = idx + 8;
    while k < body.len() && is_space(body[k]) {
        k += 1;
    }
    if k >= body.len() || body[k] != b'=' {
        return None;
    }
    k += 1;
    while k < body.len() && is_space(body[k]) {
        k += 1;
    }
    if k >= body.len() || (body[k] != b'"' && body[k] != b'\'') {
        return None;
    }
    let quote = body[k];
    k += 1;
    let value_start = k;
    while k < body.len() && body[k] != quote {
        k += 1;
    }
    if k >= body.len() {
        return None;
    }
    Some(&body[value_start..k])
}

fn ascii_label(raw: &[u8]) -> Option<&str> {
    if raw.iter().all(|&b| b.is_ascii()) {
        std::str::from_utf8(raw).ok().map(str::trim)
    } else {
        None
    }
}

/// Minimum fraction of high bytes in the head for an EBCDIC scan to be worth
/// attempting: EBCDIC text is dominated by bytes >= 0x80, ASCII-compatible
/// markup by bytes < 0x80.
const EBCDIC_SCAN_MIN_HIGH_FRACTION: f64 = 0.25;

/// Charset declarations in EBCDIC-encoded markup.
///
/// The ASCII scanners cannot see declarations in EBCDIC bytes, so when the head
/// looks like EBCDIC text it is decoded through cp037 — the page whose letter,
/// digit and tag-anchor positions every supported EBCDIC variant shares — and
/// scanned.  Only declarations naming a MAINFRAME-era encoding are yielded: the
/// tag anchor keeps plain EBCDIC prose that merely mentions ``encoding=NAME``
/// from counting as a declaration.  Quote characters are not invariant (cp1026
/// moves ``"``), so one optional junk character stands in for the opening quote.
fn ebcdic_declarations(data: &[u8]) -> Vec<Declaration> {
    let head = &data[..data.len().min(SCAN_LIMIT)];
    if head.is_empty() {
        return Vec::new();
    }
    let high_count = head.iter().filter(|&&b| b >= 0x80).count();
    if (high_count as f64) < (head.len() as f64) * EBCDIC_SCAN_MIN_HIGH_FRACTION {
        return Vec::new();
    }
    let decoded = crate::encoding_tables::decode_cp037(head);
    let bytes = decoded.as_bytes();
    let mut out: Vec<Declaration> = Vec::new();
    let mut pos = 0usize;
    while let Some(offset) = find_ignore_case(bytes, b"<meta", pos) {
        let tag_start = offset;
        let is_xml = false;
        let mut end = tag_start + 1;
        while end < bytes.len() && bytes[end] != b'<' && bytes[end] != b'>' {
            end += 1;
        }
        collect_ebcdic_labels(&bytes[tag_start..end], "text/html", is_xml, &mut out);
        pos = end.max(tag_start + 1);
    }
    pos = 0;
    while let Some(offset) = find_ignore_case(bytes, b"<?xml", pos) {
        let tag_start = offset;
        let mut end = tag_start + 1;
        while end < bytes.len() && bytes[end] != b'<' && bytes[end] != b'>' {
            end += 1;
        }
        collect_ebcdic_labels(&bytes[tag_start..end], "text/xml", true, &mut out);
        pos = end.max(tag_start + 1);
    }
    out
}

/// Scan one EBCDIC anchor tag for ``charset=``/``encoding=`` labels.
fn collect_ebcdic_labels(
    tag: &[u8],
    mime_type: &'static str,
    _is_xml: bool,
    out: &mut Vec<Declaration>,
) {
    for key in [b"charset".as_slice(), b"encoding".as_slice()] {
        let mut pos = 0usize;
        while let Some(offset) = find_ignore_case(tag, key, pos) {
            let mut i = offset + key.len();
            while i < tag.len() && is_space(tag[i]) {
                i += 1;
            }
            if i >= tag.len() || tag[i] != b'=' {
                pos = offset + key.len();
                continue;
            }
            i += 1;
            while i < tag.len() && is_space(tag[i]) {
                i += 1;
            }
            // One optional junk character stands in for the opening quote.
            if i < tag.len() && !is_space(tag[i]) && !is_label_byte(tag[i]) {
                i += 1;
                while i < tag.len() && is_space(tag[i]) {
                    i += 1;
                }
            }
            let start = i;
            if start < tag.len() && tag[start].is_ascii_alphabetic() {
                i += 1;
                while i < tag.len() && is_label_byte(tag[i]) {
                    i += 1;
                }
                if i - start >= 2 {
                    if let Ok(label) = std::str::from_utf8(&tag[start..i]) {
                        if let Some(encoding) = markup_encoding(label.trim()) {
                            let info = crate::registry::REGISTRY.get(encoding.as_str());
                            if info.is_some_and(|entry| {
                                entry.era.contains(crate::enums::EncodingEra::Mainframe)
                            }) {
                                out.push(Declaration {
                                    encoding: Some(encoding),
                                    mime_type,
                                });
                            }
                        }
                    }
                }
            }
            pos = i.max(offset + key.len());
        }
    }
}

/// Bytes that may appear in an encoding label.
fn is_label_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')
}

/// Every declaration in the head, in scan order.
pub fn declarations(data: &[u8]) -> Vec<Declaration> {
    let head_raw = &data[..data.len().min(SCAN_LIMIT)];
    let head = strip_comments(head_raw);
    let mut out: Vec<Declaration> = Vec::new();

    if let Some(raw) = xml_encoding(&head) {
        if let Some(label) = ascii_label(raw) {
            out.push(Declaration {
                encoding: if label.is_empty() {
                    None
                } else {
                    markup_encoding(label)
                },
                mime_type: "text/xml",
            });
        }
    }

    for raw in html_charsets(&head) {
        if let Some(label) = ascii_label(&raw) {
            out.push(Declaration {
                encoding: markup_encoding(label),
                mime_type: "text/html",
            });
        }
    }

    out.extend(ebcdic_declarations(head_raw));

    if let Some(raw) = pep263_cookie(head_raw) {
        if let Some(encoding) = resolve_pep263(&raw) {
            out.push(Declaration {
                encoding: Some(encoding),
                mime_type: "text/x-python",
            });
        }
    }

    out
}

fn decodes(data: &[u8], encoding: &str) -> bool {
    crate::pipeline::decode_validity::decodes_without_error(data, encoding)
}

fn structural_score(data: &[u8], encoding: &str) -> f64 {
    match REGISTRY.get(encoding) {
        Some(info) => {
            let mut ctx = PipelineContext::new();
            compute_structural_score(data, info, &mut ctx)
        }
        None => 0.0,
    }
}

/// What a declaration of *encoding* says about *data*.
fn declared_answer(data: &[u8], encoding: &str, allowed: &HashSet<String>) -> Verdict {
    let promotion = markup_promotion(encoding);

    let Some((reported_codec, superset)) = promotion else {
        if decodes(data, encoding) {
            return Verdict {
                answer: allowed.contains(encoding).then(|| encoding.to_string()),
                settled: true,
            };
        }
        return Verdict {
            answer: None,
            settled: false,
        };
    };

    if !allowed.contains(superset) {
        if decodes(data, encoding) {
            return Verdict {
                answer: allowed.contains(encoding).then(|| encoding.to_string()),
                settled: true,
            };
        }
        return Verdict {
            answer: None,
            settled: decodes(data, superset),
        };
    }

    if !allowed.contains(encoding) {
        if decodes(data, encoding) {
            return Verdict {
                answer: None,
                settled: true,
            };
        }
        if decodes(data, superset) {
            return Verdict {
                answer: Some(superset.to_string()),
                settled: true,
            };
        }
        return Verdict {
            answer: None,
            settled: false,
        };
    }

    let head = &data[..data.len().min(SCAN_LIMIT)];
    let base_score = structural_score(head, encoding);
    let superset_score = structural_score(head, superset);

    if superset_score > base_score {
        if decodes(data, superset) {
            return Verdict {
                answer: Some(superset.to_string()),
                settled: true,
            };
        }
        if decodes(data, encoding) {
            return Verdict {
                answer: Some(encoding.to_string()),
                settled: true,
            };
        }
        return Verdict {
            answer: None,
            settled: false,
        };
    }

    if decodes(data, reported_codec) {
        return Verdict {
            answer: Some(encoding.to_string()),
            settled: true,
        };
    }
    if decodes(data, superset) {
        return Verdict {
            answer: Some(superset.to_string()),
            settled: true,
        };
    }
    if reported_codec != encoding && decodes(data, encoding) {
        return Verdict {
            answer: Some(encoding.to_string()),
            settled: true,
        };
    }
    Verdict {
        answer: None,
        settled: false,
    }
}

/// Return the answer the charset declarations in *data* give, if any.
pub fn detect_markup_charset(
    data: &[u8],
    allowed: &HashSet<String>,
    utf8_precheck: Option<&DetectionResult>,
) -> Option<DetectionResult> {
    if data.is_empty() {
        return None;
    }

    let mut rejected: HashSet<String> = HashSet::new();
    for declaration in declarations(data) {
        let encoding = match declaration.encoding.as_deref() {
            Some(encoding) => encoding,
            None => continue,
        };
        if rejected.contains(encoding) {
            continue;
        }
        let verdict = declared_answer(data, encoding, allowed);
        let answer = match verdict.answer {
            Some(answer) => answer,
            None => {
                if verdict.settled {
                    return None;
                }
                rejected.insert(encoding.to_string());
                continue;
            }
        };

        if let Some(precheck) = utf8_precheck {
            let precheck_encoding = precheck.encoding.as_deref().unwrap_or("");
            if precheck_encoding != answer && allowed.contains(precheck_encoding) {
                return Some(DetectionResult::new_with_mime(
                    Some(precheck_encoding),
                    precheck.confidence,
                    precheck.language.as_deref(),
                    Some(declaration.mime_type),
                ));
            }
        }

        return Some(DetectionResult::new_with_mime(
            Some(&answer),
            DETERMINISTIC_CONFIDENCE,
            None,
            Some(declaration.mime_type),
        ));
    }
    None
}

/// Return the coding cookie named in the first two lines of *head*.
fn pep263_cookie(head: &[u8]) -> Option<String> {
    let mut lines = head.split(|&byte| byte == b'\n');
    let first = lines.next()?;
    if let Some(cookie) = coding_cookie(first) {
        return Some(cookie);
    }
    if !line_is_blank_or_comment(first) {
        return None;
    }
    let second = lines.next()?;
    coding_cookie(second)
}

fn line_is_blank_or_comment(line: &[u8]) -> bool {
    let mut i = 0usize;
    while i < line.len() && matches!(line[i], b' ' | b'\t' | 0x0c) {
        i += 1;
    }
    i >= line.len() || line[i] == b'#' || line[i] == b'\r'
}

fn coding_cookie(line: &[u8]) -> Option<String> {
    let mut start = 0usize;
    while start < line.len() && matches!(line[start], b' ' | b'\t' | 0x0c) {
        start += 1;
    }
    if start >= line.len() || line[start] != b'#' {
        return None;
    }
    let comment = &line[start..];

    let mut i = 0usize;
    while i + 6 <= comment.len() {
        if &comment[i..i + 6] == b"coding" {
            let mut k = i + 6;
            if k < comment.len() && (comment[k] == b':' || comment[k] == b'=') {
                k += 1;
                while k < comment.len() && (comment[k] == b' ' || comment[k] == b'\t') {
                    k += 1;
                }
                let name_start = k;
                while k < comment.len()
                    && (comment[k].is_ascii_alphanumeric()
                        || matches!(comment[k], b'-' | b'_' | b'.'))
                {
                    k += 1;
                }
                if k > name_start {
                    return Some(String::from_utf8_lossy(&comment[name_start..k]).into_owned());
                }
            }
        }
        i += 1;
    }
    None
}

/// Resolve a cookie the way `tokenize.detect_encoding` does.
fn resolve_pep263(raw: &str) -> Option<String> {
    let name: String = raw
        .chars()
        .take(12)
        .collect::<String>()
        .to_lowercase()
        .replace('_', "-");
    if name == "utf-8" || name.starts_with("utf-8-") {
        return Some("utf-8".to_string());
    }
    if matches!(name.as_str(), "latin-1" | "iso-8859-1" | "iso-latin-1")
        || name.starts_with("latin-1-")
        || name.starts_with("iso-8859-1-")
        || name.starts_with("iso-latin-1-")
    {
        return Some("iso8859-1".to_string());
    }
    lookup_encoding(raw).map(str::to_string)
}
