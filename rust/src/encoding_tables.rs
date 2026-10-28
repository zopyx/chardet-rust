//! Generated per-encoding byte tables for the confusion stage.
//!
//! Python reads these from `codecs` and `unicodedata`: how each encoding
//! renders a byte, which Unicode general category the result has, and whether
//! two encodings read a byte differently at all. Rust has neither codec for
//! EBCDIC/DOS pages, so `scripts/gen_encoding_tables.py` derives the answers
//! from the reference implementation and ships them as `CET1`.

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Sentinel char id for a byte the encoding cannot decode.
pub const UNDECODABLE_CHAR_ID: u16 = 0xFFFF;

const MAGIC: &[u8; 4] = b"CET1";
const BLOCK_SIZE: usize = 256 * 4;

/// One encoding's 256-entry byte tables.
#[derive(Clone)]
pub struct EncodingTables {
    /// 0 = not a letter, 1 = uppercase letter, 2 = other letter.
    pub case: [u8; 256],
    /// Index into [`CATEGORIES`].
    pub category: [u8; 256],
    /// Stable id of the decoded character (shared across encodings).
    pub char_id: [u16; 256],
}

/// Unicode general category names, in the order the artifact encodes them.
pub const CATEGORIES: [&str; 30] = [
    "Lu", "Ll", "Lt", "Lm", "Lo", "Mn", "Mc", "Me", "Nd", "Nl", "No", "Pc", "Pd", "Ps", "Pe", "Pi",
    "Pf", "Po", "Sm", "Sc", "Sk", "So", "Zs", "Zl", "Zp", "Cc", "Cf", "Cs", "Co", "Cn",
];

const CAT_LU: u8 = 0;
const CAT_LL: u8 = 1;
const CAT_MN: u8 = 5;
const CAT_MC: u8 = 6;
const CAT_CN: u8 = 29;

/// Preference score for a category, mirroring the reference's table.
pub fn category_preference(category: u8) -> i32 {
    match category {
        CAT_LU | 2 | CAT_LL => 10, // Lu, Ll, Lt
        3 | 4 => 9,                // Lm, Lo
        8 => 8,                    // Nd
        9 | 10 => 7,               // Nl, No
        11..=17 => 6,              // Pc..Po
        19 | 18 => 5,              // Sm, Sc
        20 | 21 => 4,              // Sk, So
        22..=24 => 3,              // Zs, Zl, Zp
        26 => 1,                   // Cf
        25 | 28 => 1,              // Cc, Co
        27 | CAT_CN => 0,          // Cs, Cn
        CAT_MN | CAT_MC | 7 => 5,  // Mn, Mc, Me
        _ => 0,
    }
}

/// Whether *category* is a letter category (`L*`).
pub fn category_is_letter(category: u8) -> bool {
    category <= 4
}

/// Whether *category* is lowercase (`Ll`).
pub fn category_is_lowercase(category: u8) -> bool {
    category == CAT_LL
}

static TABLES: Lazy<HashMap<String, EncodingTables>> = Lazy::new(parse_tables);

fn parse_tables() -> HashMap<String, EncodingTables> {
    let data: &[u8] = include_bytes!("data/encoding_tables.bin");
    let mut tables: HashMap<String, EncodingTables> = HashMap::new();

    if data.len() < 8 || &data[..4] != MAGIC {
        return tables;
    }
    let count = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let mut offset = 8usize;

    for _ in 0..count {
        let name_len = match data.get(offset) {
            Some(&len) => len as usize,
            None => break,
        };
        offset += 1;
        let name = match data.get(offset..offset + name_len) {
            Some(bytes) => String::from_utf8_lossy(bytes).into_owned(),
            None => break,
        };
        offset += name_len;

        let block = match data.get(offset..offset + BLOCK_SIZE) {
            Some(block) => block,
            None => break,
        };
        offset += BLOCK_SIZE;

        let mut case = [0u8; 256];
        let mut category = [0u8; 256];
        let mut char_id = [0u16; 256];
        for i in 0..256 {
            case[i] = block[i * 4];
            category[i] = block[i * 4 + 1];
            char_id[i] = u16::from_be_bytes([block[i * 4 + 2], block[i * 4 + 3]]);
        }
        tables.insert(
            name,
            EncodingTables {
                case,
                category,
                char_id,
            },
        );
    }

    tables
}

/// Return the byte tables for *encoding*, if the artifact covers it.
pub fn tables_for(encoding: &str) -> Option<&'static EncodingTables> {
    TABLES.get(encoding)
}

/// Byte values >= 0x80 that the two encodings read differently.
pub fn differing_high_bytes(enc_a: &str, enc_b: &str) -> Vec<u8> {
    let (a, b) = match (tables_for(enc_a), tables_for(enc_b)) {
        (Some(a), Some(b)) => (a, b),
        _ => return Vec::new(),
    };
    (0x80..=0xFF)
        .filter(|&byte| a.char_id[byte as usize] != b.char_id[byte as usize])
        .map(|byte| byte as u8)
        .collect()
}

/// The cp037 code page: byte -> code point (`0xFFFF_FFFF` = undecodable).
///
/// The reference decodes EBCDIC markup heads through cp037, whose letter,
/// digit and tag-anchor positions every supported EBCDIC variant shares.
static CP037_CHARS: Lazy<[u32; 256]> = Lazy::new(|| {
    let raw = include_bytes!("data/cp037.bin");
    let mut table = [u32::MAX; 256];
    for (index, chunk) in raw.as_chunks::<4>().0.iter().enumerate().take(256) {
        table[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    table
});

/// Decode *data* through cp037, replacing undecodable bytes with U+FFFD.
pub fn decode_cp037(data: &[u8]) -> String {
    data.iter()
        .map(|&byte| {
            let code_point = CP037_CHARS[byte as usize];
            if code_point == u32::MAX {
                '\u{FFFD}'
            } else {
                char::from_u32(code_point).unwrap_or('\u{FFFD}')
            }
        })
        .collect()
}

/// Generated byte-sequence validity for one multi-byte codec (``NPB1``).
pub struct BytePairs {
    /// `single[byte / 8]` bit set: the byte decodes on its own.
    pub single: [u8; 32],
    /// `pairs[index / 8]` bit set: `(lead << 8 | trail)` decodes as a pair.
    pub pairs: [u8; 8192],
}

impl BytePairs {
    /// Whether *byte* decodes as a single high byte.
    pub fn single_ok(&self, byte: u8) -> bool {
        self.single[byte as usize / 8] & (1 << (byte % 8)) != 0
    }

    /// Whether the pair *lead*, *trail* decodes.
    pub fn pair_ok(&self, lead: u8, trail: u8) -> bool {
        let index = ((lead as usize) << 8) | trail as usize;
        self.pairs[index / 8] & (1 << (index % 8)) != 0
    }
}

static BYTE_PAIRS: Lazy<HashMap<String, BytePairs>> = Lazy::new(parse_byte_pairs);

/// Parse the ``NPB1`` pair-table artifact.
fn parse_byte_pairs() -> HashMap<String, BytePairs> {
    let raw: &[u8] = include_bytes!("data/byte_pairs.bin");
    let mut tables = HashMap::new();
    if raw.len() < 8 || &raw[..4] != b"NPB1" {
        return tables;
    }
    let count = u32::from_be_bytes([raw[4], raw[5], raw[6], raw[7]]) as usize;
    let mut offset = 8usize;
    for _ in 0..count {
        if offset >= raw.len() {
            break;
        }
        let name_len = raw[offset] as usize;
        offset += 1;
        let name = String::from_utf8_lossy(&raw[offset..offset + name_len]).into_owned();
        offset += name_len;
        let mut single = [0u8; 32];
        single.copy_from_slice(&raw[offset..offset + 32]);
        offset += 32;
        let mut pairs = [0u8; 8192];
        pairs.copy_from_slice(&raw[offset..offset + 8192]);
        offset += 8192;
        tables.insert(name, BytePairs { single, pairs });
    }
    tables
}

/// Byte-sequence validity tables for *encoding*, when one was generated.
pub fn byte_pairs_for(encoding: &str) -> Option<&'static BytePairs> {
    BYTE_PAIRS.get(encoding)
}
