//! Bigram model artifacts and scoring support for statistical detection.
//!
//! The model set is the digest-locked artifact bundle the trainer writes as
//! one unit (see `chardet/models/_format.py` on the Python side):
//!
//! * `models.bin` — `CMD2`: header of per-model names and L2 norms, followed
//!   by zlib-compressed dense 65536-byte bigram tables in header order.
//! * `rowmax.bin` — `CRM1`: SHA-256 of the matching `models.bin`, then one
//!   256-byte row-maxima table per model, for upper-bound prescreening.
//! * `idf.bin` — 65536 quantized inverse-document-frequency weights.
//! * `langnorm.bin` — `CLN1`: SHA-256 of the matching `models.bin`, then one
//!   big-endian f64 per model: its norm in the IDF-weighted language space.
//!
//! # Security: Model Integrity Verification
//!
//! `models.bin` is verified against the SHA-256 recorded in
//! `src/chardet/models/models.bin.sha256` before parsing. `rowmax.bin` and
//! `langnorm.bin` embed the digest of the exact `models.bin` they were
//! derived from; a mismatch makes them unusable (they are recomputed from the
//! tables instead), which keeps the prescreen upper bound and the language
//! norms honest.
//!
//! Regenerate the hash file after replacing `models.bin`:
//!
//! ```bash
//! shasum -a 256 src/chardet/models/models.bin > src/chardet/models/models.bin.sha256
//! ```

use crate::registry::REGISTRY;
use flate2::read::ZlibDecoder;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};

/// Size of one dense bigram table (256 * 256).
pub const BIGRAM_TABLE_SIZE: usize = 65536;

/// Length of one row-maxima table (one entry per lead byte).
pub const ROWMAX_TABLE_SIZE: usize = 256;

const MODELS_MAGIC: &[u8; 4] = b"CMD2";
const ROWMAX_MAGIC: &[u8; 4] = b"CRM1";
const LANGNORM_MAGIC: &[u8; 4] = b"CLN1";

/// Magic plus the embedded models.bin digest.
const ARTIFACT_HEADER_SIZE: usize = 4 + 32;

/// Maximum number of models accepted from a header (defensive bound).
const MAX_MODELS: u32 = 10_000;

/// Maximum accepted length of a model name in bytes.
const MAX_NAME_LEN: u32 = 256;

/// Scale factor folded into the language profile's root-IDF table so its
/// entries fill a byte (`round(16 * sqrt(idf))` runs from 16 to 255).
pub const LANGUAGE_SCALE_FACTOR: f64 = 16.0;

/// Expected SHA-256 of `models.bin`, shipped beside the model file.
const MODELS_BIN_SHA256: &str = include_str!("../../src/chardet/models/models.bin.sha256");

/// Bytes whose runs training collapsed: ASCII whitespace plus NBSP.
///
/// Training counts bigrams on decoded text with a run collapse, so models
/// carry no weight for a repeated whitespace bigram; skipping them while
/// profiling keeps an uncollapsed input run from matching unrelated models.
pub fn is_collapsed_whitespace(byte: u8) -> bool {
    matches!(byte, 0x20 | 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0xA0)
}

/// One language variant of an encoding, in model-file order.
#[derive(Clone, Debug)]
pub struct ModelVariant {
    /// ISO 639-1 language code of the variant, when the key carries one.
    pub language: Option<String>,
    /// Model key as stored in `models.bin` (`"fr/cp1252"`).
    pub key: String,
}

/// The parsed model artifact set.
///
/// Built once at load time; every accessor is read-only afterwards.
pub struct ModelSet {
    /// Model keys in `models.bin` header order.
    pub keys: Vec<String>,
    /// Dense 65536-byte bigram tables keyed by model key.
    pub tables: HashMap<String, Vec<u8>>,
    /// L2 norms of the dense tables, keyed by model key.
    pub norms: HashMap<String, f64>,
    /// 65536 quantized IDF weights.
    pub idf: Vec<u8>,
    /// 65536 root-IDF weights for the language tiers.
    pub language_scale: Vec<u8>,
    /// 256-byte row maxima keyed by model key (prescreen upper bound).
    pub rowmax: HashMap<String, Vec<u8>>,
    /// IDF-weighted norms keyed by model key (language tiers).
    pub langnorm: HashMap<String, f64>,
    /// Canonical encoding name -> its language variants.
    pub index: HashMap<String, Vec<ModelVariant>>,
}

impl ModelSet {
    /// Return the variants of *encoding*, if any model covers it.
    pub fn variants(&self, encoding: &str) -> Option<&Vec<ModelVariant>> {
        self.index.get(encoding)
    }

    /// Whether any model exists for *encoding*.
    pub fn has_variants(&self, encoding: &str) -> bool {
        self.index.contains_key(encoding)
    }
}

static MODELS: Lazy<Mutex<Option<Arc<ModelSet>>>> = Lazy::new(|| Mutex::new(None));

/// Return the loaded model set, if models were loaded successfully.
pub fn get_models() -> Option<Arc<ModelSet>> {
    MODELS.lock().ok().and_then(|guard| guard.clone())
}

/// Whether the model set is loaded.
pub fn models_loaded() -> bool {
    MODELS.lock().map(|guard| guard.is_some()).unwrap_or(false)
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, String> {
    data.get(offset..offset + 4)
        .map(|slice| u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
        .ok_or_else(|| "truncated model artifact".to_string())
}

fn read_f64(data: &[u8], offset: usize) -> Result<f64, String> {
    data.get(offset..offset + 8)
        .map(|slice| {
            f64::from_be_bytes([
                slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
            ])
        })
        .ok_or_else(|| "truncated model artifact".to_string())
}

/// Keys, per-key norms, and the table area offset of a `CMD2` header.
type ParsedHeader = (Vec<String>, HashMap<String, f64>, usize);

/// Parse the `CMD2` header into keys, per-key norms, and the table area offset.
fn parse_header(data: &[u8]) -> Result<ParsedHeader, String> {
    if data.len() < 8 || &data[..4] != MODELS_MAGIC {
        return Err("corrupt models.bin: missing CMD2 magic".to_string());
    }

    let num_models = read_u32(data, 4)?;
    if num_models > MAX_MODELS {
        return Err(format!(
            "corrupt models.bin: num_models={} exceeds limit",
            num_models
        ));
    }

    let mut offset = 8usize;
    let mut keys: Vec<String> = Vec::with_capacity(num_models as usize);
    let mut norms: HashMap<String, f64> = HashMap::with_capacity(num_models as usize);

    for _ in 0..num_models {
        let name_len = read_u32(data, offset)?;
        offset += 4;
        if name_len > MAX_NAME_LEN {
            return Err(format!(
                "corrupt models.bin: name_len={} exceeds 256",
                name_len
            ));
        }
        let name_end = offset + name_len as usize;
        let name_bytes = data
            .get(offset..name_end)
            .ok_or_else(|| "truncated models.bin".to_string())?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|e| format!("invalid UTF-8 in model name: {}", e))?
            .to_string();
        offset = name_end;

        let norm = read_f64(data, offset)?;
        offset += 8;

        keys.push(name.clone());
        norms.insert(name, norm);
    }

    Ok((keys, norms, offset))
}

/// Decompress the dense tables that follow the header.
fn decompress_tables(data: &[u8], offset: usize, keys: &[String]) -> Result<Vec<Vec<u8>>, String> {
    let expected = keys.len() * BIGRAM_TABLE_SIZE;
    let mut decoder = ZlibDecoder::new(&data[offset..]);
    let mut raw = Vec::with_capacity(expected);
    decoder
        .read_to_end(&mut raw)
        .map_err(|e| format!("corrupt models.bin: {}", e))?;

    if raw.len() < expected {
        return Err(format!(
            "corrupt models.bin: decompressed size {} != expected {}",
            raw.len(),
            expected
        ));
    }

    Ok(raw[..expected]
        .as_chunks::<BIGRAM_TABLE_SIZE>()
        .0
        .iter()
        .map(|chunk| chunk.to_vec())
        .collect())
}

/// Parse the `idf.bin` weight table, falling back to uniform weights.
fn parse_idf(data: &[u8]) -> Vec<u8> {
    if data.len() == BIGRAM_TABLE_SIZE {
        data.to_vec()
    } else {
        vec![1u8; BIGRAM_TABLE_SIZE]
    }
}

/// Build the root-IDF table the language profile multiplies by.
fn build_language_scale(idf: &[u8]) -> Vec<u8> {
    let roots: Vec<u8> = (0..=255u8)
        .map(|v| (LANGUAGE_SCALE_FACTOR * (v as f64).sqrt()).round() as u8)
        .collect();
    idf.iter().map(|&v| roots[v as usize]).collect()
}

/// Compute one model's row maxima from its dense table.
fn rowmax_from_table(table: &[u8]) -> Vec<u8> {
    table
        .as_chunks::<ROWMAX_TABLE_SIZE>()
        .0
        .iter()
        .map(|row| row.iter().copied().max().unwrap_or(0))
        .collect()
}

/// Compute one model's IDF-weighted norm from its dense table.
fn langnorm_from_table(table: &[u8], idf: &[u8]) -> f64 {
    let mut total = 0.0f64;
    for (i, &v) in table.iter().enumerate() {
        if v != 0 {
            let w = v as f64;
            total += w * w * idf[i] as f64;
        }
    }
    total.sqrt()
}

/// Parse `rowmax.bin`, rejecting a file whose digest does not match.
fn parse_rowmax(data: &[u8], digest: &[u8], keys: &[String]) -> Option<HashMap<String, Vec<u8>>> {
    if data.len() != ARTIFACT_HEADER_SIZE + keys.len() * ROWMAX_TABLE_SIZE
        || &data[..4] != ROWMAX_MAGIC
        || data[4..ARTIFACT_HEADER_SIZE] != *digest
    {
        return None;
    }
    let mut table = HashMap::with_capacity(keys.len());
    for (i, key) in keys.iter().enumerate() {
        let start = ARTIFACT_HEADER_SIZE + i * ROWMAX_TABLE_SIZE;
        table.insert(key.clone(), data[start..start + ROWMAX_TABLE_SIZE].to_vec());
    }
    Some(table)
}

/// Parse `langnorm.bin`, rejecting a file whose digest does not match.
fn parse_langnorm(data: &[u8], digest: &[u8], keys: &[String]) -> Option<HashMap<String, f64>> {
    if data.len() != ARTIFACT_HEADER_SIZE + keys.len() * 8
        || &data[..4] != LANGNORM_MAGIC
        || data[4..ARTIFACT_HEADER_SIZE] != *digest
    {
        return None;
    }
    let mut norms = HashMap::with_capacity(keys.len());
    for (i, key) in keys.iter().enumerate() {
        let offset = ARTIFACT_HEADER_SIZE + i * 8;
        norms.insert(key.clone(), read_f64(data, offset).ok()?);
    }
    Some(norms)
}

/// Group model keys by canonical encoding name, resolving aliases.
fn build_index(keys: &[String]) -> HashMap<String, Vec<ModelVariant>> {
    let mut index: HashMap<String, Vec<ModelVariant>> = HashMap::new();

    for key in keys {
        let (language, encoding) = match key.split_once('/') {
            Some((lang, enc)) => (Some(lang.to_string()), enc.to_string()),
            None => (None, key.clone()),
        };
        index.entry(encoding).or_default().push(ModelVariant {
            language,
            key: key.clone(),
        });
    }

    // A model key may use a non-canonical (alias) encoding name; expose the
    // same variants under the registry's canonical name.
    let alias_names: Vec<(String, String)> = index
        .keys()
        .filter_map(|name| {
            let canonical = REGISTRY.get(name.as_str())?.name;
            (canonical != name.as_str()).then(|| (name.clone(), canonical.to_string()))
        })
        .collect();

    for (alias, canonical) in alias_names {
        if !index.contains_key(&canonical) {
            let variants = index[&alias].clone();
            index.insert(canonical, variants);
        }
    }

    index
}

/// Verify `models.bin` against the digest shipped beside it.
fn verify_models_hash(data: &[u8]) -> Result<(), String> {
    // The digest file carries explanatory comment lines above the checksum.
    let expected = MODELS_BIN_SHA256
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or("")
        .to_ascii_lowercase();
    if expected.is_empty() {
        return Ok(());
    }

    let mut hasher = Sha256::new();
    hasher.update(data);
    let computed = format!("{:x}", hasher.finalize());

    if computed != expected {
        return Err(format!(
            "models.bin hash verification failed: expected {}, got {}. \
             The model file may have been tampered with or corrupted.",
            expected, computed
        ));
    }
    Ok(())
}

/// Parse the full artifact bundle and install it as the active model set.
pub fn init_models(
    models: &[u8],
    idf_data: &[u8],
    rowmax_data: &[u8],
    langnorm_data: &[u8],
) -> Result<(), String> {
    verify_models_hash(models)?;

    let (keys, norms, offset) = parse_header(models)?;
    let tables_vec = decompress_tables(models, offset, &keys)?;

    let idf = parse_idf(idf_data);
    let language_scale = build_language_scale(&idf);

    let mut digest = Sha256::new();
    digest.update(models);
    let models_digest = digest.finalize().to_vec();

    let rowmax = match parse_rowmax(rowmax_data, &models_digest, &keys) {
        Some(table) => table,
        None => tables_vec
            .iter()
            .enumerate()
            .map(|(i, table)| (keys[i].clone(), rowmax_from_table(table)))
            .collect(),
    };

    let langnorm = match parse_langnorm(langnorm_data, &models_digest, &keys) {
        Some(norms) => norms,
        None => tables_vec
            .iter()
            .enumerate()
            .map(|(i, table)| (keys[i].clone(), langnorm_from_table(table, &idf)))
            .collect(),
    };

    let tables: HashMap<String, Vec<u8>> = keys.iter().cloned().zip(tables_vec).collect();

    let index = build_index(&keys);

    let model_set = ModelSet {
        keys,
        tables,
        norms,
        idf,
        language_scale,
        rowmax,
        langnorm,
        index,
    };

    let mut cache = MODELS
        .lock()
        .map_err(|_| "model lock poisoned".to_string())?;
    *cache = Some(Arc::new(model_set));
    Ok(())
}

/// Embedded artifacts for the native (non-Python) build and Rust tests.
pub mod embedded {
    pub const MODELS_BIN: &[u8] = include_bytes!("../../src/chardet/models/models.bin");
    pub const IDF_BIN: &[u8] = include_bytes!("../../src/chardet/models/idf.bin");
    pub const ROWMAX_BIN: &[u8] = include_bytes!("../../src/chardet/models/rowmax.bin");
    pub const LANGNORM_BIN: &[u8] = include_bytes!("../../src/chardet/models/langnorm.bin");
}

/// Load the artifacts embedded in the binary.
pub fn init_embedded_models() -> Result<(), String> {
    init_models(
        embedded::MODELS_BIN,
        embedded::IDF_BIN,
        embedded::ROWMAX_BIN,
        embedded::LANGNORM_BIN,
    )
}
