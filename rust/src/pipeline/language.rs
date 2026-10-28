//! Three-tier language detection for filling result languages.
//!
//! Tier 1: registry mapping for single-language encodings (Big5 -> zh, ...).
//! Tier 2: bigram scoring against the encoding's language-model variants, in
//! the IDF-weighted space the language tiers use.
//! Tier 3: decode to UTF-8 and score against the UTF-8 byte-level models.
//!
//! This mirrors `chardet/pipeline/language.py`.

use crate::bigram_models::get_models;
use crate::pipeline::statistical::{score_best_language_weighted, BigramProfile, RARE_LANGUAGES};
use crate::pipeline::DetectionResult;
use crate::registry::REGISTRY;

/// Maximum bytes of data used for language scoring.
const LANG_SCORE_MAX_BYTES: usize = 2048;

/// Inputs shorter than this are "thin": bigram cosines stop discriminating, so
/// a rare-language winner may be demoted to the best prevalent variant.
const THIN_RARE_MAX_BYTES: usize = 128;

/// Language of a single-language encoding, if the registry maps exactly one.
fn infer_language(encoding: &str) -> Option<&'static str> {
    let info = REGISTRY.get(encoding).copied()?;
    if info.languages.len() == 1 {
        Some(info.languages[0])
    } else {
        None
    }
}

/// Decode *data* from *encoding* and re-encode as UTF-8 for language scoring.
///
/// Returns `None` when the codec is unknown to `encoding_rs`; the caller then
/// leaves the tier out rather than guessing.
fn to_utf8(data: &[u8], encoding: &str) -> Option<Vec<u8>> {
    match encoding {
        "utf-8" => Some(data.to_vec()),
        "utf-16" => Some(decode_utf16(data, None)),
        "utf-16-le" | "utf-16le" => Some(decode_utf16(data, Some(false))),
        "utf-16-be" | "utf-16be" => Some(decode_utf16(data, Some(true))),
        "utf-32" => Some(decode_utf32(data, None)),
        "utf-32-le" | "utf-32le" => Some(decode_utf32(data, Some(false))),
        "utf-32-be" | "utf-32be" => Some(decode_utf32(data, Some(true))),
        "utf-7" => Some(decode_utf7(data)),
        _ => {
            let codec = encoding_rs::Encoding::for_label(encoding.as_bytes())?;
            let (decoded, _) = codec.decode_without_bom_handling(data);
            Some(decoded.into_owned().into_bytes())
        }
    }
}

/// Decode UTF-16 to UTF-8, honouring a BOM when *big_endian* is `None`.
#[allow(clippy::chunks_exact_to_as_chunks)] // pairs are read positionally; as_chunks would obscure the surrogate lookahead
fn decode_utf16(data: &[u8], big_endian: Option<bool>) -> Vec<u8> {
    let (body, big_endian) = match (big_endian, data) {
        (Some(flag), _) => (data, flag),
        (None, [0xFF, 0xFE, ..]) => (&data[2..], false),
        (None, [0xFE, 0xFF, ..]) => (&data[2..], true),
        (None, _) => (data, false),
    };

    let mut out = String::with_capacity(body.len());
    let mut units = body.chunks_exact(2);
    while let Some(unit) = units.next() {
        let value = if big_endian {
            u16::from_be_bytes([unit[0], unit[1]])
        } else {
            u16::from_le_bytes([unit[0], unit[1]])
        };
        if let Some(ch) = char::from_u32(value as u32) {
            out.push(ch);
        } else if (0xD800..=0xDBFF).contains(&value) {
            // Surrogate pair: pair it with the next unit when present.
            if let Some(low) = units.next() {
                let low_value = if big_endian {
                    u16::from_be_bytes([low[0], low[1]])
                } else {
                    u16::from_le_bytes([low[0], low[1]])
                };
                let combined =
                    0x10000 + (((value as u32) - 0xD800) << 10) + ((low_value as u32) - 0xDC00);
                if let Some(ch) = char::from_u32(combined) {
                    out.push(ch);
                }
            }
        }
    }
    out.into_bytes()
}

/// Decode UTF-32 to UTF-8, honouring a BOM when *big_endian* is `None`.
fn decode_utf32(data: &[u8], big_endian: Option<bool>) -> Vec<u8> {
    let (body, big_endian) = match (big_endian, data) {
        (Some(flag), _) => (data, flag),
        (None, [0x00, 0x00, 0xFE, 0xFF, ..]) => (&data[4..], true),
        (None, [0xFF, 0xFE, 0x00, 0x00, ..]) => (&data[4..], false),
        (None, _) => (data, false),
    };

    let mut out = String::with_capacity(body.len() / 2);
    for unit in body.as_chunks::<4>().0 {
        let value = if big_endian {
            u32::from_be_bytes([unit[0], unit[1], unit[2], unit[3]])
        } else {
            u32::from_le_bytes([unit[0], unit[1], unit[2], unit[3]])
        };
        if let Some(ch) = char::from_u32(value) {
            out.push(ch);
        }
    }
    out.into_bytes()
}

/// Decode UTF-7 (RFC 2152) to UTF-8.
///
/// ASCII passes through; ``+`` opens a modified-base64 run of UTF-16 code
/// units terminated by ``-`` (or by any non-base64 byte), and ``+-`` is a
/// literal plus.
fn decode_utf7(data: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(data.len());
    let mut i = 0usize;

    while i < data.len() {
        let byte = data[i];
        if byte != b'+' {
            out.push(byte);
            i += 1;
            continue;
        }
        i += 1;
        if i < data.len() && data[i] == b'-' {
            out.push(b'+');
            i += 1;
            continue;
        }

        let mut bits: u32 = 0;
        let mut nbits: u32 = 0;
        let mut units: Vec<u16> = Vec::new();
        while i < data.len() {
            let value = match data[i] {
                b'A'..=b'Z' => data[i] - b'A',
                b'a'..=b'z' => data[i] - b'a' + 26,
                b'0'..=b'9' => data[i] - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => break,
            };
            i += 1;
            bits = (bits << 6) | value as u32;
            nbits += 6;
            if nbits >= 16 {
                nbits -= 16;
                units.push(((bits >> nbits) & 0xFFFF) as u16);
            }
        }
        if i < data.len() && data[i] == b'-' {
            i += 1;
        }

        let mut k = 0usize;
        while k < units.len() {
            let unit = units[k] as u32;
            if (0xD800..=0xDBFF).contains(&unit) && k + 1 < units.len() {
                let low = units[k + 1] as u32;
                if let Some(ch) = char::from_u32(
                    0x10000 + ((unit - 0xD800) << 10) + (low.wrapping_sub(0xDC00) & 0x3FF),
                ) {
                    push_char(&mut out, ch);
                }
                k += 2;
                continue;
            }
            if let Some(ch) = char::from_u32(unit) {
                push_char(&mut out, ch);
            }
            k += 1;
        }
    }

    out
}

fn push_char(out: &mut Vec<u8>, ch: char) {
    let mut buffer = [0u8; 4];
    out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
}

fn is_rare(language: Option<&str>) -> bool {
    match language {
        Some(lang) => RARE_LANGUAGES.contains(&lang),
        None => false,
    }
}

/// Fill missing `language` fields on text results via the three-tier algorithm.
pub fn fill_languages(data: &[u8], results: Vec<DetectionResult>) -> Vec<DetectionResult> {
    let data = &data[..data.len().min(LANG_SCORE_MAX_BYTES)];
    let thin = !data.is_empty() && data.len() < THIN_RARE_MAX_BYTES;

    let models = match get_models() {
        Some(models) => models,
        None => return results,
    };

    let mut profile: Option<BigramProfile> = None;
    let mut utf8_profile: Option<BigramProfile> = None;
    let mut utf8_src: Option<Vec<u8>> = None;
    let mut filled: Vec<DetectionResult> = Vec::with_capacity(results.len());

    for result in results {
        let recheck = thin && is_rare(result.language.as_deref()) && result.encoding.is_some();

        if result.encoding.is_none() || (result.language.is_some() && !recheck) {
            filled.push(result);
            continue;
        }

        let encoding = match result.encoding.as_ref() {
            Some(encoding) => encoding.clone(),
            None => {
                filled.push(result);
                continue;
            }
        };

        // Tier 1: single-language encoding (skipped on re-check: the label
        // exists, only a scored tier may justify demoting it).
        let mut language: Option<String> = if recheck {
            None
        } else {
            infer_language(&encoding).map(str::to_string)
        };

        // Tier 2: statistical scoring for multi-language encodings.
        if language.is_none() && !data.is_empty() && models.has_variants(&encoding) {
            let profile = profile.get_or_insert_with(|| {
                BigramProfile::for_language(data, &models.idf, &models.language_scale)
            });
            let (_, best) = score_best_language_weighted(&models, profile, &encoding, thin);
            language = best;
        }

        // Tier 3: decode to UTF-8 and score against the UTF-8 models. Also
        // entered by a thin rare Tier-2 label so a prevalent rival gets a vote.
        let escalate = thin && is_rare(language.as_deref());
        if (language.is_none() || escalate) && !data.is_empty() && models.has_variants("utf-8") {
            if let Some(utf8_data) = to_utf8(data, &encoding) {
                if !utf8_data.is_empty() {
                    if utf8_src.as_deref() != Some(utf8_data.as_slice()) {
                        utf8_profile = Some(BigramProfile::for_language(
                            &utf8_data,
                            &models.idf,
                            &models.language_scale,
                        ));
                        utf8_src = Some(utf8_data.clone());
                    }
                    if let Some(profile) = utf8_profile.as_ref() {
                        let (_, utf8_lang) =
                            score_best_language_weighted(&models, profile, "utf-8", thin);
                        // Keep the pipeline's label unless this stage has a
                        // prevalent-language one to offer.
                        if language.is_none()
                            || (utf8_lang.is_some() && !is_rare(utf8_lang.as_deref()))
                        {
                            language = utf8_lang;
                        }
                    }
                }
            }
        }

        // A re-check only has authority to demote; never swap one rare label
        // for another, and never drop a label the pipeline attached.
        if language.is_none() || (recheck && is_rare(language.as_deref())) {
            filled.push(result);
        } else {
            filled.push(DetectionResult::new_with_mime(
                Some(&encoding),
                result.confidence,
                language.as_deref(),
                result.mime_type.as_deref(),
            ));
        }
    }

    filled
}
