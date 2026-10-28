//! Stage 13: post-processing rank corrections.
//!
//! After statistical scoring produces a ranked list, a chain of rank
//! corrections fixes up the ranking where bigrams alone are insufficient.
//! The order is weakest evidence first (dead-heat priors), then byte-level
//! evidence (confusion groups, niche Latin demotion, KOI8-T promotion), then
//! platform evidence (classic-Mac line endings).
//!
//! This mirrors `chardet/pipeline/postprocess.py`.
//!
//! The confusion-group arbitration and the niche-Latin distinguishing-byte
//! re-litigation are still handled by the simplified
//! [`crate::pipeline::confusion`] stage; this module owns the dead-heat
//! priors, the rare-language arbitration, the KOI8-T promotion and the
//! classic-Mac line-ending promotion.

use crate::bigram_models::get_models;
use crate::encoding_tables::differing_high_bytes;
use crate::pipeline::confusion::{
    arbitrate_distinguishing_bytes, confusion_pair_winner, resolve_confusion_groups, CONFUSION_BAND,
};
use crate::pipeline::statistical::{ART_LANGUAGE, RARE_LANGUAGES};
use crate::pipeline::DetectionResult;
use crate::registry::REGISTRY;
use std::collections::HashSet;

/// Confidence gap below which two candidates are a statistical dead heat.
const DEAD_HEAT_EPSILON: f64 = 1e-4;

/// Maximum lead over the best prevalent candidate for a rare-language winner
/// to count as a coin flip rather than evidence.
const RARE_ARBITRATION_MARGIN: f64 = 0.02;

/// Maximum absolute confidence for rare-language arbitration to apply.
const RARE_ARBITRATION_MAX_CONFIDENCE: f64 = 0.15;

/// Cap on the data scanned for high-byte bigram evidence.
const EVIDENCE_SCAN_MAX_BYTES: usize = 16384;

/// Bytes where KOI8-T maps to Tajik letters but KOI8-R maps box-drawing.
const KOI8_T_DISTINGUISHING: [u8; 12] = [
    0x80, 0x81, 0x83, 0x8A, 0x8C, 0x8D, 0x8E, 0x90, 0xA1, 0xA2, 0xA5, 0xB5,
];

/// Common Western Latin encodings used as swap targets for niche demotion.
const COMMON_LATIN_ENCODINGS: [&str; 3] = ["iso8859-1", "iso8859-15", "cp1252"];

/// Encodings whose models can win without byte-level support.
const DEMOTION_CANDIDATES: [&str; 4] = ["iso8859-10", "iso8859-14", "cp1254", "hp-roman8"];

/// Minimum bare-`\r` line endings before the classic-Mac promotion fires.
const CR_MAC_MIN_LINES: usize = 3;

/// `EncodingEra::LegacyMac` as a bit value.
const LEGACY_MAC_ERA: u32 = 4;

/// Lowest era bit of *encoding* (lower = more prevalent today).
fn era_rank(encoding: &str) -> u32 {
    let info = match REGISTRY.get(encoding) {
        Some(info) => info,
        None => return 1 << 30,
    };
    let era = info.era as u32;
    era.isolate_lowest_one()
}

/// The Windows/CP code page standing in for a multi-byte CJK encoding.
fn cjk_superset(encoding: &str) -> Option<&'static str> {
    let info = REGISTRY.get(encoding)?;
    if info.is_multibyte_cjk {
        info.superset
    } else {
        None
    }
}

/// Move `results[i]` to the top, carrying the current top confidence.
fn promote_to_top(results: Vec<DetectionResult>, i: usize) -> Vec<DetectionResult> {
    let mut results = results;
    let top_confidence = results[0].confidence;
    let promoted = results.remove(i);
    let promoted = DetectionResult::new_with_mime(
        promoted.encoding.as_deref(),
        top_confidence,
        promoted.language.as_deref(),
        promoted.mime_type.as_deref(),
    );
    let mut out = Vec::with_capacity(results.len() + 1);
    out.push(promoted);
    out.extend(results);
    out
}

/// Whether *encoding*'s winning model weights a high-byte bigram in *data*.
///
/// A candidate whose model assigns zero weight to every non-ASCII bigram
/// earned its score purely from ASCII bigrams — noise that cannot distinguish
/// encodings.
fn has_high_byte_evidence(data: &[u8], encoding: &str, language: Option<&str>) -> bool {
    let models = match get_models() {
        Some(models) => models,
        None => return false,
    };
    let variants = match models.variants(encoding) {
        Some(variants) => variants,
        None => return false,
    };

    let window = &data[..data.len().min(EVIDENCE_SCAN_MAX_BYTES)];
    if window.len() < 2 {
        return false;
    }

    let mut seen: Vec<u16> = Vec::new();
    let mut prev = window[0];
    for &b in &window[1..] {
        if prev >= 0x80 || b >= 0x80 {
            let idx = ((prev as u16) << 8) | b as u16;
            if !seen.contains(&idx) {
                seen.push(idx);
            }
        }
        prev = b;
    }
    if seen.is_empty() {
        return false;
    }

    for variant in variants {
        if let Some(language) = language {
            if variant.language.as_deref() != Some(language) {
                continue;
            }
        }
        let table = match models.tables.get(&variant.key) {
            Some(table) => table,
            None => continue,
        };
        if seen.iter().any(|&idx| table[idx as usize] != 0) {
            return true;
        }
    }
    false
}

/// Break statistical dead heats in favor of the more prevalent era.
fn prefer_prevalent_on_dead_heat(
    data: &[u8],
    results: Vec<DetectionResult>,
) -> Vec<DetectionResult> {
    if results.len() < 2 {
        return results;
    }
    let top = &results[0];
    let top_encoding = match top.encoding.as_deref() {
        Some(encoding) => encoding,
        None => return results,
    };

    let mut best_idx = 0usize;
    let mut best_rank = era_rank(top_encoding);
    for (i, result) in results.iter().enumerate().skip(1) {
        let encoding = match result.encoding.as_deref() {
            Some(encoding) => encoding,
            None => continue,
        };
        if top.confidence - result.confidence > DEAD_HEAT_EPSILON {
            break;
        }
        let rank = era_rank(encoding);
        if rank < best_rank {
            best_rank = rank;
            best_idx = i;
        }
    }

    if best_idx == 0 {
        return results;
    }

    // A top whose models weight no observed high-byte bigram has no evidence
    // at all and yields outright.
    if !has_high_byte_evidence(data, top_encoding, top.language.as_deref()) {
        return promote_to_top(results, best_idx);
    }

    // Otherwise the prevalent candidate has to win the distinguishing-byte
    // arbitration outright; a tie keeps the top.
    let rival = results[best_idx]
        .encoding
        .clone()
        .unwrap_or_else(|| top_encoding.to_string());
    let mut languages = languages_of(&results[0]);
    languages.extend(languages_of(&results[best_idx]));
    let comparable = comparable_languages(top_encoding, &rival, &languages);
    let diff = differing_high_bytes(top_encoding, &rival);

    let winner = arbitrate_distinguishing_bytes(
        data,
        top_encoding,
        &rival,
        &diff,
        comparable.as_ref(),
        comparable.as_ref(),
    );

    if winner.as_deref() == Some(rival.as_str()) {
        return promote_to_top(results, best_idx);
    }
    results
}

/// Promote a Windows superset over its base encoding on a dead heat.
fn promote_superset_on_dead_heat(
    data: &[u8],
    results: Vec<DetectionResult>,
) -> Vec<DetectionResult> {
    if results.len() < 2 {
        return results;
    }
    let top = &results[0];
    let top_encoding = match top.encoding.as_deref() {
        Some(encoding) => encoding,
        None => return results,
    };
    let superset = match cjk_superset(top_encoding) {
        Some(superset) => superset,
        None => return results,
    };

    for (i, result) in results.iter().enumerate().skip(1) {
        if top.confidence - result.confidence > DEAD_HEAT_EPSILON {
            break;
        }
        if result.encoding.as_deref() == Some(superset) && decodes_without_error(data, superset) {
            return promote_to_top(results, i);
        }
    }
    results
}

/// Demote a rare-language winner that leads a prevalent rival by a coin flip.
fn arbitrate_rare_language(results: Vec<DetectionResult>) -> Vec<DetectionResult> {
    if results.len() < 2 {
        return results;
    }
    let top = &results[0];
    let top_language = match top.language.as_deref() {
        Some(language) => language,
        None => return results,
    };
    if top.encoding.is_none()
        || !RARE_LANGUAGES.contains(&top_language)
        || top.confidence >= RARE_ARBITRATION_MAX_CONFIDENCE
    {
        return results;
    }

    for (i, result) in results.iter().enumerate().skip(1) {
        if top.confidence - result.confidence > RARE_ARBITRATION_MARGIN {
            break;
        }
        let language = match result.language.as_deref() {
            Some(language) => language,
            None => continue,
        };
        if !RARE_LANGUAGES.contains(&language) {
            return promote_to_top(results, i);
        }
    }
    results
}

/// Promote KOI8-T over KOI8-R when Tajik-specific bytes are present.
fn promote_koi8t(data: &[u8], results: Vec<DetectionResult>) -> Vec<DetectionResult> {
    if results.is_empty() || results[0].encoding.as_deref() != Some("koi8-r") {
        return results;
    }
    let koi8t_idx = results
        .iter()
        .position(|r| r.encoding.as_deref() == Some("koi8-t"));
    let koi8t_idx = match koi8t_idx {
        Some(idx) => idx,
        None => return results,
    };
    if data.iter().any(|b| KOI8_T_DISTINGUISHING.contains(b)) {
        return promote_to_top(results, koi8t_idx);
    }
    results
}

/// Pick the common Latin candidate that replaces a demoted top.
fn swap_target<'a>(candidates: &[&'a DetectionResult]) -> Option<&'a DetectionResult> {
    let lead = candidates
        .iter()
        .map(|result| result.confidence)
        .fold(f64::MIN, f64::max);
    let mut best: Option<&DetectionResult> = None;
    let mut best_rank = u32::MAX;
    for candidate in candidates {
        if lead - candidate.confidence > DEAD_HEAT_EPSILON {
            continue;
        }
        let rank = era_rank(candidate.encoding.as_deref().unwrap_or(""));
        if rank < best_rank {
            best_rank = rank;
            best = Some(candidate);
        }
    }
    best
}

/// Whether *top* has no byte evidence over *target* and should be demoted.
pub fn should_demote_public(data: &[u8], top: &DetectionResult, target: &DetectionResult) -> bool {
    should_demote(data, top, target)
}

const ISO_8859_10_DISTINGUISHING: [u8; 46] = [
    0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA8, 0xA9, 0xAA, 0xAB, 0xAC, 0xAE, 0xAF, 0xB1, 0xB2, 0xB3,
    0xB4, 0xB5, 0xB6, 0xB8, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0, 0xC7, 0xC8, 0xCA, 0xCC,
    0xD1, 0xD2, 0xD7, 0xD9, 0xE0, 0xE7, 0xE8, 0xEA, 0xEC, 0xF1, 0xF2, 0xF7, 0xF9, 0xFF,
];

const ISO_8859_14_DISTINGUISHING: [u8; 31] = [
    0xA1, 0xA2, 0xA4, 0xA5, 0xA6, 0xA8, 0xAA, 0xAB, 0xAC, 0xAF, 0xB0, 0xB1, 0xB2, 0xB3, 0xB4, 0xB5,
    0xB7, 0xB8, 0xB9, 0xBA, 0xBB, 0xBC, 0xBD, 0xBE, 0xBF, 0xD0, 0xD7, 0xDE, 0xF0, 0xF7, 0xFE,
];

const WINDOWS_1254_DISTINGUISHING: [u8; 6] = [0xD0, 0xDD, 0xDE, 0xF0, 0xFD, 0xFE];

const HP_ROMAN8_DISTINGUISHING: [u8; 23] = [
    0xC0, 0xC1, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB, 0xCC, 0xCD, 0xCE, 0xCF,
    0xD1, 0xD4, 0xD5, 0xD6, 0xD9, 0xDD, 0xDE,
];

/// Distinguishing bytes for each niche-Latin demotion candidate.
///
/// These are the reference's curated sets: the bytes where the candidate reads
/// differently from ISO-8859-1 *and* the difference can be attributed to the
/// niche encoding rather than to a Windows/ISO variant.  They are not the full
/// table difference — arbitration on the wider set builds a different focused
/// profile and the models then "decide" pairs they have no evidence for.
fn demotion_distinguishing(encoding: &str) -> Option<&'static [u8]> {
    match encoding {
        "iso8859-10" => Some(&ISO_8859_10_DISTINGUISHING),
        "iso8859-14" => Some(&ISO_8859_14_DISTINGUISHING),
        "cp1254" => Some(&WINDOWS_1254_DISTINGUISHING),
        "hp-roman8" => Some(&HP_ROMAN8_DISTINGUISHING),
        _ => None,
    }
}

fn should_demote(data: &[u8], top: &DetectionResult, target: &DetectionResult) -> bool {
    let encoding = match top.encoding.as_deref() {
        Some(encoding) => encoding,
        None => return false,
    };
    // The reference's curated distinguishing bytes for this candidate.
    let diff: Vec<u8> = match demotion_distinguishing(encoding) {
        Some(bytes) => bytes.to_vec(),
        None => differing_high_bytes(encoding, "iso8859-1"),
    };
    if !data.iter().any(|byte| diff.contains(byte)) {
        return true;
    }
    if top.confidence - target.confidence > CONFUSION_BAND {
        return false;
    }

    let target_encoding = target.encoding.as_deref().unwrap_or("");
    let languages_a = if top.language.is_some() {
        Some(languages_of(top))
    } else {
        None
    };
    let languages_b = if target.language.is_some() {
        Some(languages_of(target))
    } else {
        None
    };

    let winner = arbitrate_distinguishing_bytes(
        data,
        encoding,
        target_encoding,
        &diff,
        languages_a.as_ref(),
        languages_b.as_ref(),
    );
    winner.as_deref() != Some(encoding)
}

/// Demote a niche Latin top that its distinguishing bytes do not support.
pub fn demote_niche_latin(data: &[u8], results: Vec<DetectionResult>) -> Vec<DetectionResult> {
    if results.len() < 2 {
        return results;
    }
    let demoted_encoding = match results[0].encoding.as_deref() {
        Some(encoding) => encoding.to_string(),
        None => return results,
    };
    if !DEMOTION_CANDIDATES.contains(&demoted_encoding.as_str()) {
        return results;
    }

    let candidates: Vec<&DetectionResult> = results[1..]
        .iter()
        .filter(|result| {
            result
                .encoding
                .as_deref()
                .is_some_and(|enc| COMMON_LATIN_ENCODINGS.contains(&enc))
        })
        .collect();
    if candidates.is_empty() {
        return results;
    }

    let target = match swap_target(&candidates) {
        Some(target) => target,
        None => return results,
    };
    if !should_demote(data, &results[0], target) {
        return results;
    }

    let top_confidence = results[0].confidence;
    let target_encoding = target.encoding.clone();
    let target_language = target.language.clone();
    let target_mime = target.mime_type.clone();

    let mut others: Vec<DetectionResult> = Vec::with_capacity(results.len());
    for result in results.iter() {
        if result.encoding == target_encoding {
            continue;
        }
        if result.encoding.as_deref() == Some(demoted_encoding.as_str()) {
            continue;
        }
        others.push(result.clone());
    }
    let tail_confidence = others
        .last()
        .map(|result| result.confidence)
        .unwrap_or(top_confidence);

    let mut out: Vec<DetectionResult> = Vec::with_capacity(results.len());
    out.push(DetectionResult::new_with_mime(
        target_encoding.as_deref(),
        top_confidence,
        target_language.as_deref(),
        target_mime.as_deref(),
    ));
    out.extend(others);
    for result in results.iter() {
        if result.encoding.as_deref() == Some(demoted_encoding.as_str()) {
            out.push(DetectionResult::new_with_mime(
                result.encoding.as_deref(),
                result.confidence.min(tail_confidence),
                result.language.as_deref(),
                result.mime_type.as_deref(),
            ));
        }
    }
    out
}

/// Languages reported by one result, as a set.
fn languages_of(result: &DetectionResult) -> HashSet<String> {
    let mut set = HashSet::new();
    if let Some(language) = result.language.as_ref() {
        set.insert(language.clone());
    }
    set
}

/// Languages both sides can be compared in, or `None` for unrestricted.
fn comparable_languages(
    enc_a: &str,
    enc_b: &str,
    languages: &HashSet<String>,
) -> Option<HashSet<String>> {
    let models = get_models()?;
    if languages.is_empty() {
        return None;
    }
    let modelled = |enc: &str| -> HashSet<String> {
        models
            .variants(enc)
            .map(|variants| {
                variants
                    .iter()
                    .filter_map(|variant| variant.language.clone())
                    .collect()
            })
            .unwrap_or_default()
    };
    let langs_a = modelled(enc_a);
    let langs_b = modelled(enc_b);
    let shared: HashSet<String> = langs_a.intersection(&langs_b).cloned().collect();
    if !languages.is_subset(&shared) {
        return None;
    }
    if langs_a.contains(ART_LANGUAGE) || langs_b.contains(ART_LANGUAGE) {
        let mut with_art = shared;
        with_art.insert(ART_LANGUAGE.to_string());
        return Some(with_art);
    }
    Some(shared)
}

/// Whether *encoding* decodes *data* without an invalid sequence.
fn decodes_without_error(data: &[u8], encoding: &str) -> bool {
    let codec = match encoding_rs::Encoding::for_label(encoding.as_bytes()) {
        Some(codec) => codec,
        None => return true,
    };
    codec
        .decode_without_bom_handling_and_without_replacement(data)
        .is_some()
}

/// Promote a classic-Mac candidate when line endings are bare `\r`.
fn promote_mac_on_cr_line_endings(
    data: &[u8],
    results: Vec<DetectionResult>,
) -> Vec<DetectionResult> {
    if results.len() < 2 {
        return results;
    }
    let top = &results[0];
    let top_encoding = match top.encoding.as_deref() {
        Some(encoding) => encoding,
        None => return results,
    };
    if era_rank(top_encoding) == LEGACY_MAC_ERA {
        return results;
    }
    // An art-model win is not up for prose-based review.
    if top.language.as_deref() == Some(ART_LANGUAGE) {
        return results;
    }
    if data.contains(&b'\n') || data.iter().filter(|&&b| b == b'\r').count() < CR_MAC_MIN_LINES {
        return results;
    }

    for (i, result) in results.iter().enumerate().skip(1) {
        if top.confidence - result.confidence > CONFUSION_BAND {
            break;
        }
        let encoding = match result.encoding.as_deref() {
            Some(encoding) => encoding,
            None => continue,
        };
        if era_rank(encoding) != LEGACY_MAC_ERA {
            continue;
        }
        let mut languages = languages_of(top);
        languages.extend(languages_of(result));
        if confusion_pair_winner(data, top_encoding, encoding, &languages).as_deref()
            == Some(top_encoding)
        {
            // Byte-level evidence says the top beats the best-ranked Mac
            // candidate: stop rather than let a lower-ranked sibling win.
            break;
        }
        return promote_to_top(results, i);
    }
    results
}

/// Strict decode check: the caller's whole input must decode cleanly.
fn decodes_completely(data: &[u8], encoding: &str) -> bool {
    decodes_without_error(data, encoding)
}

/// Whether the winner's tolerant decode is non-empty pure ASCII.
///
/// That means its only multi-byte evidence is the undecodable trailing
/// sequence, so the ranking carries no real evidence for it.
/// Whether *data* is ASCII text plus an incomplete tail under *encoding*.
///
/// One pass answers both halves of the decode-safety question: the tolerant
/// decode yields the text before any deferred tail, and the strict decode of
/// the whole window failing is exactly the deferred tail existing.  True means
/// the candidate decoded real ASCII characters and then hit an incomplete
/// multi-byte sequence — its only non-ASCII evidence is the tail itself.
///
/// Deliberately false when the tolerant decode yields nothing at all: a
/// clipped multi-byte fragment has no decoded evidence, and the ranking's own
/// judgment serves it better.
fn dangling_tail_with_ascii_prefix(data: &[u8], encoding: &str) -> bool {
    let Some(codec) = encoding_rs::Encoding::for_label(encoding.as_bytes()) else {
        return false;
    };
    if codec
        .decode_without_bom_handling_and_without_replacement(data)
        .is_some()
    {
        // The whole window decodes: no deferred tail, so no dangling tail.
        return false;
    }
    let mut prefix: Option<&[u8]> = None;
    for trim in 0..=3usize {
        if trim >= data.len() {
            break;
        }
        let window = &data[..data.len() - trim];
        if codec
            .decode_without_bom_handling_and_without_replacement(window)
            .is_some()
        {
            prefix = Some(window);
            break;
        }
    }
    let Some(prefix) = prefix else {
        return false;
    };
    if prefix.is_empty() {
        return false;
    }
    let (decoded, _) = codec.decode_without_bom_handling(prefix);
    !decoded.is_empty() && decoded.is_ascii()
}

/// Promote a strictly decoding rival over a winner with no real evidence.
fn prefer_decodable_on_tie(data: &[u8], results: Vec<DetectionResult>) -> Vec<DetectionResult> {
    if results.len() < 2 {
        return results;
    }
    let top = &results[0];
    let top_encoding = match top.encoding.as_deref() {
        Some(encoding) => encoding,
        None => return results,
    };

    // A dangling multi-byte tail needs a high byte among the final bytes.
    let tail_start = data.len().saturating_sub(4);
    if !data[tail_start..].iter().any(|&b| b >= 0x80) {
        return results;
    }
    if !dangling_tail_with_ascii_prefix(data, top_encoding) {
        return results;
    }

    for (i, result) in results.iter().enumerate().skip(1) {
        let encoding = match result.encoding.as_deref() {
            Some(encoding) => encoding,
            None => continue,
        };
        if !decodes_completely(data, encoding) {
            continue;
        }
        return promote_to_top(results, i);
    }
    results
}

/// Apply the rank-correction chain.
pub fn postprocess_results(
    data: &[u8],
    results: Vec<DetectionResult>,
    input_truncated: bool,
) -> Vec<DetectionResult> {
    let results = promote_superset_on_dead_heat(data, results);
    let results = prefer_prevalent_on_dead_heat(data, results);
    let results = arbitrate_rare_language(results);
    let results = resolve_confusion_groups(data, results);
    let results = demote_niche_latin(data, results);
    let results = promote_koi8t(data, results);
    let results = promote_mac_on_cr_line_endings(data, results);
    if input_truncated {
        return results;
    }
    prefer_decodable_on_tie(data, results)
}
