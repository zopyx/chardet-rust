//! Bigram model loading and scoring for statistical detection.
//!
//! # Security: Model Integrity Verification
//!
//! The bigram models are loaded from `models.bin` with SHA-256 hash verification
//! to ensure file integrity and detect potential tampering. The expected hash
//! is stored in `src/chardet/models/models.bin.sha256`.
//!
//! ## Hash Verification Process
//!
//! 1. Before loading models, the SHA-256 hash of `models.bin` is computed
//! 2. The computed hash is compared against the expected hash
//! 3. If hashes don't match, loading fails with an error indicating potential tampering
//!
//! ## Regenerating the Hash
//!
//! If `models.bin` is updated, regenerate the hash file:
//! ```bash
//! sha256sum src/chardet/models/models.bin > src/chardet/models/models.bin.sha256
//! ```

use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

/// Size of the bigram lookup table (256 * 256)
const BIGRAM_TABLE_SIZE: usize = 65536;

/// Weight applied to non-ASCII bigrams
pub const NON_ASCII_BIGRAM_WEIGHT: i32 = 8;

/// Expected SHA-256 hash of models.bin for integrity verification.
/// This hash must match the actual models.bin file to prevent tampering.
///
/// To regenerate if models.bin changes:
/// ```bash
/// sha256sum src/chardet/models/models.bin
/// ```
/// Then update this constant with the new hash.
const MODELS_BIN_SHA256: &str = "90421949cfc7380de3fda8e9ce606e6a6e9834562ccbcc8f0b772393d05afb93";

/// Cached models
type ModelsMap = HashMap<String, Vec<u8>>;
#[allow(clippy::type_complexity)]
static MODELS: Lazy<Mutex<Option<ModelsMap>>> = Lazy::new(|| Mutex::new(None));

/// Sparse weighted bigram profile built from input data.
struct WeightedProfile {
    entries: Vec<(u16, i32)>,
    norm: f64,
}

/// Load bigram models from models.bin file content
pub fn load_models(data: &[u8]) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut models = HashMap::new();
    let mut offset = 0;

    if data.len() < 4 {
        return Err("models.bin too small".to_string());
    }

    // Read number of encodings (big-endian u32)
    let num_encodings = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
    offset += 4;

    if num_encodings > 10000 {
        return Err(format!(
            "corrupt models.bin: num_encodings={} exceeds limit",
            num_encodings
        ));
    }

    for _ in 0..num_encodings {
        // Read name length
        if offset + 4 > data.len() {
            return Err("truncated models.bin".to_string());
        }
        let name_len = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        if name_len > 256 {
            return Err(format!(
                "corrupt models.bin: name_len={} exceeds 256",
                name_len
            ));
        }

        // Read name
        if offset + name_len > data.len() {
            return Err("truncated models.bin".to_string());
        }
        let name = String::from_utf8(data[offset..offset + name_len].to_vec())
            .map_err(|e| format!("invalid UTF-8 in model name: {}", e))?;
        offset += name_len;

        // Read number of entries
        if offset + 4 > data.len() {
            return Err("truncated models.bin".to_string());
        }
        let num_entries = u32::from_be_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        if num_entries > BIGRAM_TABLE_SIZE {
            return Err(format!(
                "corrupt models.bin: num_entries={} exceeds {}",
                num_entries, BIGRAM_TABLE_SIZE
            ));
        }

        // Create table and fill with weights
        let mut table = vec![0u8; BIGRAM_TABLE_SIZE];
        for _ in 0..num_entries {
            if offset + 3 > data.len() {
                return Err("truncated models.bin".to_string());
            }
            let b1 = data[offset] as usize;
            let b2 = data[offset + 1] as usize;
            let weight = data[offset + 2];
            offset += 3;
            table[(b1 << 8) | b2] = weight;
        }

        models.insert(name, table);
    }

    Ok(models)
}

/// Verify the SHA-256 hash of models.bin data.
///
/// # Security
/// This function computes the SHA-256 hash of the provided data and compares
/// it against the expected hash to detect:
/// - File corruption during packaging or installation
/// - Tampering with the model file
/// - Version mismatches between code and models
///
/// # Arguments
///
/// * `data` - The raw bytes of models.bin to verify
///
/// # Returns
///
/// * `Ok(())` if the hash matches
/// * `Err(String)` if the hash doesn't match, indicating potential tampering
///
/// # Errors
///
/// Returns an error if:
/// - The computed hash doesn't match the expected hash
/// - This indicates the models.bin file may have been tampered with
fn verify_models_hash(data: &[u8]) -> Result<(), String> {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(data);
    let computed_hash = format!("{:x}", hasher.finalize());

    if computed_hash != MODELS_BIN_SHA256 {
        return Err(format!(
            "models.bin hash verification failed: expected {}, got {}. \
             The model file may have been tampered with or corrupted.",
            MODELS_BIN_SHA256, computed_hash
        ));
    }

    Ok(())
}

/// Initialize models from embedded data.
///
/// # Security
/// Before loading models, this function verifies the SHA-256 hash of the
/// embedded data to ensure integrity and detect tampering.
///
/// # Arguments
///
/// * `data` - The raw bytes of models.bin (typically from include_bytes!)
///
/// # Returns
///
/// * `Ok(())` if models loaded successfully and hash verified
/// * `Err(String)` if hash verification fails or models fail to load
pub fn init_models(data: &[u8]) -> Result<(), String> {
    // Security: Verify hash before loading models
    verify_models_hash(data)?;

    let models = load_models(data)?;
    let mut cache = MODELS.lock().unwrap();
    *cache = Some(models);
    let mut norms = MODEL_NORMS.lock().unwrap();
    norms.clear();
    Ok(())
}

/// Get a model by key (e.g., "French/windows-1252")
/// Check if models are loaded
pub fn models_loaded() -> bool {
    let cache = MODELS.lock().unwrap();
    cache.is_some()
}

/// Calculate L2 norm of a model
pub fn calculate_model_norm(model: &[u8]) -> f64 {
    let sq_sum: u64 = model.iter().map(|&w| (w as u64) * (w as u64)).sum();
    (sq_sum as f64).sqrt()
}

fn build_weighted_profile(data: &[u8]) -> Option<WeightedProfile> {
    if data.len() < 2 {
        return None;
    }

    let mut profile: HashMap<u16, i32> = HashMap::new();
    let mut total_weight = 0i32;

    for pair in data.windows(2) {
        let b1 = pair[0];
        let b2 = pair[1];
        let idx = ((b1 as u16) << 8) | (b2 as u16);

        let weight = if b1 > 0x7F || b2 > 0x7F {
            NON_ASCII_BIGRAM_WEIGHT
        } else {
            1
        };

        *profile.entry(idx).or_insert(0) += weight;
        total_weight += weight;
    }

    if total_weight == 0 {
        return None;
    }

    let mut entries = Vec::with_capacity(profile.len());
    let mut input_norm_sq = 0i64;
    for (idx, weight) in profile {
        let w = weight as i64;
        input_norm_sq += w * w;
        entries.push((idx, weight));
    }

    let norm = (input_norm_sq as f64).sqrt();
    if norm == 0.0 {
        return None;
    }

    Some(WeightedProfile { entries, norm })
}

/// Score a pre-built profile against a model using cosine similarity
fn score_profile_with_model(profile: &WeightedProfile, model: &[u8], model_norm: f64) -> f64 {
    if model_norm == 0.0 {
        return 0.0;
    }

    let mut dot_product = 0i64;

    for (idx, weight) in &profile.entries {
        let model_weight = model[*idx as usize] as i64;
        let w = *weight as i64;
        dot_product += model_weight * w;
    }

    dot_product as f64 / (model_norm * profile.norm)
}

/// Score data against all language variants of an encoding
pub fn score_best_language(data: &[u8], encoding: &str) -> (f64, Option<String>) {
    let profile = match build_weighted_profile(data) {
        Some(p) => p,
        None => return (0.0, None),
    };

    let cache = MODELS.lock().unwrap();
    let models = match cache.as_ref() {
        Some(m) => m,
        None => return (0.0, None),
    };

    let suffix = format!("/{}", encoding);
    let mut matching_keys = Vec::new();
    for key in models.keys() {
        if key.ends_with(&suffix) {
            matching_keys.push(key.clone());
        }
    }

    if matching_keys.is_empty() {
        return (0.0, None);
    }

    let mut best_score = 0.0;
    let mut best_lang = None;

    let mut norms = MODEL_NORMS.lock().unwrap();
    for key in matching_keys {
        let model = match models.get(&key) {
            Some(m) => m,
            None => continue,
        };

        let norm = if let Some(cached) = norms.get(&key) {
            *cached
        } else {
            let computed = calculate_model_norm(model);
            norms.insert(key.clone(), computed);
            computed
        };

        let score = score_profile_with_model(&profile, model, norm);

        // Extract language from key (format: "Language/encoding")
        let lang = key.split('/').next().map(|s| s.to_string());

        if score > best_score {
            best_score = score;
            best_lang = lang;
        }
    }

    (best_score, best_lang)
}

/// Pre-computed model norms cache
static MODEL_NORMS: Lazy<Mutex<HashMap<String, f64>>> = Lazy::new(|| Mutex::new(HashMap::new()));
