//! Pipeline orchestrator - runs all detection stages in sequence.

use crate::enums::EncodingEra;
use crate::pipeline::language;
use crate::pipeline::postprocess;
use crate::pipeline::structural::*;
use crate::pipeline::*;
use crate::registry::{self, EncodingInfo};
use std::collections::HashSet;

/// The low-confidence fallback, or no answer when the caller excluded it.
fn fallback(encoding: &str, allowed: &HashSet<String>, param_name: &str) -> DetectionResult {
    if allowed.contains(encoding) {
        DetectionResult::new(Some(encoding), DEFAULT_LOW_CONFIDENCE, None)
    } else {
        DetectionResult::new(None, 0.0, None).with_fallback(param_name)
    }
}

/// Drop a stage result whose encoding the caller's filters exclude.
fn gated(result: Option<DetectionResult>, allowed: &HashSet<String>) -> Option<DetectionResult> {
    result.filter(|r| match r.encoding.as_deref() {
        Some(encoding) => allowed.contains(encoding),
        None => true,
    })
}

/// Maximum bytes used for statistical bigram scoring.
///
/// Bigram models converge quickly; 16 KB is sufficient for discrimination
/// across every language model, and scoring beyond it costs time without
/// changing the ranking.  Mirrors the reference's `_STAT_SCORE_MAX_BYTES`.
const STAT_SCORE_MAX_BYTES: usize = 16384;

/// Slice the data statistical scoring is allowed to see.
fn stat_window(data: &[u8]) -> &[u8] {
    &data[..data.len().min(STAT_SCORE_MAX_BYTES)]
}

/// Confidence threshold for structural detection to trigger statistical scoring.
pub const STRUCTURAL_CONFIDENCE_THRESHOLD: f64 = 0.85;

/// Run the full detection pipeline.
pub fn run_pipeline(
    data: &[u8],
    encoding_era: EncodingEra,
    max_bytes: usize,
) -> Vec<DetectionResult> {
    run_pipeline_with(data, encoding_era, max_bytes, &PipelineOptions::default())
}

/// Caller-selectable pipeline options, mirroring the reference's
/// `_run_pipeline_core` keyword arguments.
#[derive(Clone, Copy)]
pub struct PipelineOptions<'a> {
    /// Restrict detection to these canonical encoding names, if any.
    pub include_encodings: Option<&'a HashSet<String>>,
    /// Remove these canonical encoding names from the candidate set.
    pub exclude_encodings: Option<&'a HashSet<String>>,
    /// Encoding reported when no candidate survives.
    pub no_match_encoding: &'a str,
    /// Encoding reported for empty input.
    pub empty_input_encoding: &'a str,
    /// Rank every candidate (needed by `detect_all`).
    pub full_ranking: bool,
    /// The window is already a cut-off view of the caller's input.
    pub input_truncated: bool,
}

impl Default for PipelineOptions<'_> {
    fn default() -> Self {
        Self {
            include_encodings: None,
            exclude_encodings: None,
            no_match_encoding: "cp1252",
            empty_input_encoding: "utf-8",
            full_ranking: false,
            input_truncated: false,
        }
    }
}

/// Run the pipeline with explicit options.
pub fn run_pipeline_with(
    data: &[u8],
    encoding_era: EncodingEra,
    max_bytes: usize,
    options: &PipelineOptions<'_>,
) -> Vec<DetectionResult> {
    // A window shorter than the caller's input is a chardet-made cut, whatever
    // the caller passed: the bytes here are not the whole story.
    let mut options = *options;
    options.input_truncated = options.input_truncated || data.len() > max_bytes;
    let options = &options;
    let results = run_pipeline_inner(data, encoding_era, max_bytes, options);
    let window = &data[..data.len().min(max_bytes)];
    finalize(language::fill_languages(window, results))
}

/// The pipeline proper; [`run_pipeline`] finalizes what this returns.
fn run_pipeline_inner(
    data: &[u8],
    encoding_era: EncodingEra,
    max_bytes: usize,
    options: &PipelineOptions<'_>,
) -> Vec<DetectionResult> {
    let data = &data[..data.len().min(max_bytes)];
    let mut ctx = PipelineContext::new();

    // The caller's filters gate every stage, including the early exits: a
    // BOM, escape sequence, or markup declaration naming an encoding the
    // caller excluded is no answer.
    let allowed: HashSet<String> = registry::get_candidates_filtered(
        encoding_era,
        options.include_encodings,
        options.exclude_encodings,
    )
    .iter()
    .map(|enc| enc.name.to_string())
    .collect();

    if data.is_empty() {
        return vec![fallback(
            options.empty_input_encoding,
            &allowed,
            "empty_input_encoding",
        )];
    }

    // Stage 0a: BOM detection (before binary check, as BOM indicates text encoding)
    if let Some(result) = gated(
        bom::detect_bom_window(data, options.input_truncated),
        &allowed,
    ) {
        return vec![result];
    }

    // Magic numbers identify binary formats outright.  They run before the
    // UTF-16/32 pattern stage: a recognized container is never text, and its
    // byte soup can otherwise satisfy the null-pattern heuristics.
    if let Some(result) = magic::detect_magic(data) {
        return vec![result];
    }

    // Stage 0b: UTF-16/32 null-byte pattern detection (before binary check)
    // UTF-32/16 have many null bytes but are valid text encodings
    // We check these first to avoid misclassifying them as binary
    if let Some(result) = gated(utf1632::detect_utf1632_patterns(data), &allowed) {
        // The stage samples the first few KB; the answer has to decode the
        // whole window, so a lone surrogate or out-of-range unit past the
        // sample sends the input on to the other stages instead.
        let decodes = match result.encoding.as_deref() {
            Some("utf-16-le") | Some("utf-16le") => decode_validity::utf16_window_ok(data, false),
            Some("utf-16-be") | Some("utf-16be") => decode_validity::utf16_window_ok(data, true),
            Some("utf-32-le") | Some("utf-32le") => decode_validity::utf32_window_ok(data, false),
            Some("utf-32-be") | Some("utf-32be") => decode_validity::utf32_window_ok(data, true),
            Some(encoding) => decode_validity::decodes_without_error(data, encoding),
            None => false,
        };
        if decodes {
            return vec![result];
        }
    }

    // Escape-sequence encodings (ISO-2022, HZ-GB-2312, UTF-7)
    if let Some(result) = gated(escape::detect_escape_encoding(data), &allowed) {
        return vec![result];
    }

    // Pre-check UTF-8: a result carries the multi-byte evidence, while
    // validity is a separate question.  ``detect_utf8`` returns None both for
    // data that is not UTF-8 and for valid UTF-8 with no *complete* sequence
    // (pure ASCII, ASCII plus a tail cut by the window), and only the first
    // justifies ruling UTF-8 out downstream.
    let utf8_precheck = utf8::detect_utf8(data);
    // Validity is the exhaustive question: the whole window must be UTF-8.
    // Only a final sequence cut by the end of the window is tolerated, which
    // is what makes a clipped prefix still count as UTF-8 while a stray 0xFF
    // in the same position does not.
    let utf8_window_valid = match std::str::from_utf8(data) {
        Ok(_) => true,
        Err(_) => {
            let mut cut_tail = false;
            for k in data.len().saturating_sub(3)..data.len() {
                let needed = utf8::utf8_sequence_length(data[k]);
                if needed > 1 && k + needed > data.len() {
                    cut_tail = true;
                    break;
                }
            }
            cut_tail
        }
    };
    let utf8_failed_validation = utf8_precheck.is_none() && !utf8_window_valid;

    // Pre-check ASCII for the same reason as UTF-8: ASCII text carrying NUL
    // separators (find -print0 output) would otherwise trip the binary check.
    let ascii_precheck = ascii::detect_ascii(data);

    // Binary detection (encoding=None) is not gated by filters, and is
    // skipped when the data is already known to be text.
    if utf8_precheck.is_none() && ascii_precheck.is_none() && binary::is_binary(data, max_bytes) {
        return vec![DetectionResult::new(None, DEFAULT_BINARY_CONFIDENCE, None)];
    }

    // Stage 1b: Markup charset extraction
    if let Some(result) = gated(
        markup::detect_markup_charset(data, &allowed, utf8_precheck.as_ref()),
        &allowed,
    ) {
        return vec![result];
    }

    // Stage 1c: ASCII
    if let Some(result) = gated(ascii_precheck, &allowed) {
        return vec![result];
    }

    // Stage 1d: UTF-8
    if let Some(result) = gated(utf8_precheck, &allowed) {
        return vec![result];
    }

    // Stage 2a: Byte validity filtering
    let candidates = registry::get_candidates_filtered(
        encoding_era,
        options.include_encodings,
        options.exclude_encodings,
    );
    let mut valid_candidates = decode_validity::filter_by_validity(data, &candidates);

    // If UTF-8 failed structural validation, exclude it from candidates
    // to prevent the statistical model from scoring it
    if utf8_failed_validation {
        valid_candidates.retain(|c| c.name != "utf-8" && c.name != "utf-8-sig");
    }

    if valid_candidates.is_empty() {
        return vec![fallback(
            options.no_match_encoding,
            &allowed,
            "no_match_encoding",
        )];
    }

    // Gate: eliminate CJK multi-byte candidates that lack genuine multi-byte structure
    let valid_candidates = gate_cjk_candidates(data, valid_candidates, &mut ctx);

    if valid_candidates.is_empty() {
        return vec![fallback(
            options.no_match_encoding,
            &allowed,
            "no_match_encoding",
        )];
    }

    // Stage 2b: Structural probing for multi-byte encodings
    let mut structural_scores: Vec<(String, f64)> = Vec::new();
    for enc in &valid_candidates {
        if enc.is_multibyte_cjk {
            let score = ctx
                .mb_scores
                .get(enc.name)
                .copied()
                .unwrap_or_else(|| structural::compute_structural_score(data, enc, &mut ctx));
            if score > MIN_CONFIDENCE {
                structural_scores.push((enc.name.to_string(), score));
            }
        }
    }

    // If a multi-byte encoding scored very high, score all candidates statistically
    if !structural_scores.is_empty() {
        structural_scores.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .expect("Confidence scores should always be comparable")
        });
        let (_, best_score) = structural_scores[0];

        if best_score >= STRUCTURAL_CONFIDENCE_THRESHOLD {
            let results = score_structural_candidates(
                data,
                &structural_scores,
                &valid_candidates,
                &mut ctx,
                options.full_ranking,
            );
            if !results.is_empty() {
                return postprocess_results(data, results, options.input_truncated);
            }
            // No candidate could be scored (issue #367: a two-byte CJK pair
            // scores structurally but carries no bigram evidence).  Fall
            // through to the statistical stage, as the reference does.
        }
    }

    // Stage 3: Statistical scoring for all remaining candidates
    let results =
        statistical::score_candidates(stat_window(data), &valid_candidates, options.full_ranking);

    if results.is_empty() {
        return vec![fallback(
            options.no_match_encoding,
            &allowed,
            "no_match_encoding",
        )];
    }

    let results = postprocess_results(data, results, options.input_truncated);
    if results.is_empty() {
        return vec![fallback(
            options.no_match_encoding,
            &allowed,
            "no_match_encoding",
        )];
    }
    results
}

fn gate_cjk_candidates<'a>(
    data: &[u8],
    candidates: Vec<&'a EncodingInfo>,
    ctx: &mut PipelineContext,
) -> Vec<&'a EncodingInfo> {
    let mut gated: Vec<&EncodingInfo> = Vec::new();

    for enc in candidates {
        if enc.is_multibyte_cjk {
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
    full_ranking: bool,
) -> Vec<DetectionResult> {
    // Get multi-byte candidates that passed structural check
    let valid_mb_names: std::collections::HashSet<&str> = structural_scores
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();

    let mut candidates_to_score: Vec<&EncodingInfo> = valid_candidates
        .iter()
        .filter(|e| e.is_multibyte_cjk && valid_mb_names.contains(e.name))
        .copied()
        .collect();

    // Add single-byte candidates
    candidates_to_score.extend(
        valid_candidates
            .iter()
            .filter(|e| !e.is_multibyte_cjk)
            .copied(),
    );

    // Score all candidates
    let mut results =
        statistical::score_candidates(stat_window(data), &candidates_to_score, full_ranking);

    // Boost multi-byte candidates with high byte coverage
    for result in &mut results {
        if let Some(ref enc_name) = result.encoding {
            if let Some(&coverage) = ctx.mb_coverage.get(enc_name) {
                if coverage >= HIGH_BYTE_COVERAGE_THRESHOLD {
                    result.confidence *= MAX_CONFIDENCE + coverage;
                }
            }
        }
    }

    // Sort by confidence
    results.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .expect("Confidence scores should always be comparable")
    });

    // Clamp confidence to [0.0, 1.0]
    for result in &mut results {
        result.confidence = result.confidence.clamp(MIN_CONFIDENCE, MAX_CONFIDENCE);
    }

    results
}

/// Finalize results at the public API boundary.
///
/// The ANSI/ASCII-art model is keyed under the "zxx" pseudo-language (ISO 639
/// for "no linguistic content"); callers see `language = None` instead.  A
/// MIME type is defaulted for text and binary results alike.
fn finalize(results: Vec<DetectionResult>) -> Vec<DetectionResult> {
    results
        .into_iter()
        .map(|result| {
            let language = match result.language.as_deref() {
                Some("zxx") => None,
                _ => result.language.clone(),
            };
            let mut finished = DetectionResult::new_with_mime(
                result.encoding.as_deref(),
                result.confidence,
                language.as_deref(),
                result.mime_type.as_deref(),
            )
            .with_default_mime();
            finished.fallback = result.fallback.clone();
            finished
        })
        .collect()
}

fn postprocess_results(
    data: &[u8],
    results: Vec<DetectionResult>,
    input_truncated: bool,
) -> Vec<DetectionResult> {
    postprocess::postprocess_results(data, results, input_truncated)
}
