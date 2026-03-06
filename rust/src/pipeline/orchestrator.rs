//! Pipeline orchestrator - runs all detection stages in sequence.

use crate::enums::EncodingEra;
use crate::pipeline::structural::*;
use crate::pipeline::*;
use crate::registry::{get_candidates, EncodingInfo};

const STRUCTURAL_CONFIDENCE_THRESHOLD: f64 = 0.85;

/// Run the full detection pipeline.
pub fn run_pipeline(
    data: &[u8],
    encoding_era: EncodingEra,
    max_bytes: usize,
) -> Vec<DetectionResult> {
    let data = &data[..data.len().min(max_bytes)];
    let mut ctx = PipelineContext::new();

    if data.is_empty() {
        return vec![DetectionResult::new(Some("utf-8"), 0.10, None)];
    }

    // Stage 0a: BOM detection (before binary check, as BOM indicates text encoding)
    if let Some(result) = bom::detect_bom(data) {
        return vec![result];
    }

    // Stage 0b: UTF-16/32 null-byte pattern detection (before binary check)
    // UTF-32/16 have many null bytes but are valid text encodings
    // We check these first to avoid misclassifying them as binary
    if let Some(result) = utf1632::detect_utf1632_patterns(data) {
        return vec![result];
    }

    // Stage 0c: Binary detection - run after UTF-16/32 check
    if binary::is_binary(data, max_bytes) {
        return vec![DetectionResult::new(None, DETERMINISTIC_CONFIDENCE, None)];
    }

    // Escape-sequence encodings (ISO-2022, HZ-GB-2312, UTF-7)
    if let Some(result) = escape::detect_escape_encoding(data) {
        // Check era filter
        let enc_name = result.encoding.as_ref().unwrap();
        if let Some(enc_info) = get_candidates(encoding_era)
            .into_iter()
            .find(|e| e.name == enc_name.as_str())
        {
            if encoding_era.contains(&enc_info.era) {
                return vec![result];
            }
        }
    }

    // Pre-check UTF-8 and track if it failed validation
    let (utf8_precheck, utf8_failed_validation) = match utf8::detect_utf8(data) {
        Some(result) => (Some(result), false),
        None => {
            // Check if it failed due to invalid bytes (not just pure ASCII)
            let has_high_bytes = data.iter().any(|&b| b >= 0x80);
            (None, has_high_bytes) // Failed validation if high bytes exist
        }
    };

    // Stage 1b: Markup charset extraction
    if let Some(result) = markup::detect_markup_charset(data) {
        return vec![result];
    }

    // Stage 1c: ASCII
    if let Some(result) = ascii::detect_ascii(data) {
        return vec![result];
    }

    // Stage 1d: UTF-8
    if let Some(result) = utf8_precheck {
        return vec![result];
    }

    // Stage 2a: Byte validity filtering
    let candidates = get_candidates(encoding_era);
    let mut valid_candidates = validity::filter_by_validity(data, &candidates);

    // If UTF-8 failed structural validation, exclude it from candidates
    // to prevent the statistical model from scoring it
    if utf8_failed_validation {
        valid_candidates.retain(|c| c.name != "utf-8" && c.name != "utf-8-sig");
    }

    if valid_candidates.is_empty() {
        return vec![DetectionResult::new(Some("windows-1252"), 0.10, None)];
    }

    // Gate: eliminate CJK multi-byte candidates that lack genuine multi-byte structure
    let valid_candidates = gate_cjk_candidates(data, valid_candidates, &mut ctx);

    if valid_candidates.is_empty() {
        return vec![DetectionResult::new(Some("windows-1252"), 0.10, None)];
    }

    // Stage 2b: Structural probing for multi-byte encodings
    let mut structural_scores: Vec<(String, f64)> = Vec::new();
    for enc in &valid_candidates {
        if enc.is_multibyte {
            let score = ctx
                .mb_scores
                .get(enc.name)
                .copied()
                .unwrap_or_else(|| structural::compute_structural_score(data, enc, &mut ctx));
            if score > 0.0 {
                structural_scores.push((enc.name.to_string(), score));
            }
        }
    }

    // If a multi-byte encoding scored very high, score all candidates statistically
    if !structural_scores.is_empty() {
        structural_scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        let (_, best_score) = structural_scores[0];

        if best_score >= STRUCTURAL_CONFIDENCE_THRESHOLD {
            let results =
                score_structural_candidates(data, &structural_scores, &valid_candidates, &mut ctx);
            return postprocess_results(data, results);
        }
    }

    // Stage 3: Statistical scoring for all remaining candidates
    let results = statistical::score_candidates(data, &valid_candidates);

    if results.is_empty() {
        return vec![DetectionResult::new(Some("windows-1252"), 0.10, None)];
    }

    postprocess_results(data, results)
}

fn gate_cjk_candidates<'a>(
    data: &[u8],
    candidates: Vec<&'a EncodingInfo>,
    ctx: &mut PipelineContext,
) -> Vec<&'a EncodingInfo> {
    let mut gated: Vec<&EncodingInfo> = Vec::new();

    for enc in candidates {
        if enc.is_multibyte {
            let mb_score = structural::compute_structural_score(data, enc, ctx);
            ctx.mb_scores.insert(enc.name.to_string(), mb_score);

            if mb_score < CJK_MIN_MB_RATIO {
                continue; // No multi-byte structure -> eliminate
            }

            let non_ascii_count = ctx.non_ascii_count.unwrap_or_else(|| {
                let count = data.iter().filter(|&&b| b > 0x7F).count();
                ctx.non_ascii_count = Some(count);
                count
            });

            if non_ascii_count < CJK_MIN_NON_ASCII {
                continue; // Too few high bytes
            }

            let byte_coverage =
                structural::compute_multibyte_byte_coverage(data, enc, ctx, Some(non_ascii_count));
            ctx.mb_coverage.insert(enc.name.to_string(), byte_coverage);

            if byte_coverage < CJK_MIN_BYTE_COVERAGE {
                continue; // Most high bytes are orphans
            }

            if non_ascii_count >= CJK_DIVERSITY_MIN_NON_ASCII {
                let lead_diversity = structural::compute_lead_byte_diversity(data, enc, ctx);
                if lead_diversity < CJK_MIN_LEAD_DIVERSITY {
                    continue; // Too few distinct lead bytes
                }
            }
        }
        gated.push(enc);
    }

    gated
}

fn score_structural_candidates(
    data: &[u8],
    structural_scores: &[(String, f64)],
    valid_candidates: &[&EncodingInfo],
    ctx: &mut PipelineContext,
) -> Vec<DetectionResult> {
    // Get multi-byte candidates that passed structural check
    let valid_mb_names: std::collections::HashSet<&str> = structural_scores
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();

    let mut candidates_to_score: Vec<&EncodingInfo> = valid_candidates
        .iter()
        .filter(|e| e.is_multibyte && valid_mb_names.contains(e.name))
        .copied()
        .collect();

    // Add single-byte candidates
    candidates_to_score.extend(valid_candidates.iter().filter(|e| !e.is_multibyte).copied());

    // Score all candidates
    let mut results = statistical::score_candidates(data, &candidates_to_score);

    // Boost multi-byte candidates with high byte coverage
    for result in &mut results {
        if let Some(ref enc_name) = result.encoding {
            if let Some(&coverage) = ctx.mb_coverage.get(enc_name) {
                if coverage >= 0.95 {
                    result.confidence *= 1.0 + coverage;
                }
            }
        }
    }

    // Sort by confidence
    results.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

    // Clamp confidence to [0.0, 1.0]
    for result in &mut results {
        result.confidence = result.confidence.min(1.0);
    }

    results
}

fn postprocess_results(data: &[u8], results: Vec<DetectionResult>) -> Vec<DetectionResult> {
    let results = confusion::resolve_confusion_groups(data, results);
    demote_niche_latin(data, results)
}

fn demote_niche_latin(_data: &[u8], results: Vec<DetectionResult>) -> Vec<DetectionResult> {
    // Simplified version - check for niche Latin encodings with no distinguishing bytes
    // Note: We no longer demote windows-1254 as it's the standard encoding for Turkish,
    // not a "niche" encoding. Demoting it causes Turkish text to be misdetected.
    if results.len() < 2 {
        return results;
    }

    let top_encoding = results[0].encoding.as_ref().cloned();

    // Truly niche encodings to demote (mostly obsolete ISO-8859 variants)
    let niche_encodings: &[&str] = &["iso-8859-10", "iso-8859-14"];

    if let Some(ref enc) = top_encoding {
        if niche_encodings.contains(&enc.as_str()) {
            // Find a common Latin encoding to promote
            let common_latin = &["iso-8859-1", "iso-8859-15", "windows-1252"];

            for (i, result) in results.iter().enumerate().skip(1) {
                if let Some(ref enc_name) = result.encoding {
                    if common_latin.contains(&enc_name.as_str()) {
                        // Swap results
                        let mut new_results = results.clone();
                        new_results.swap(0, i);
                        return new_results;
                    }
                }
            }
        }
    }

    results
}
