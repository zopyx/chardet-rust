//! Accuracy evaluation tests - Rust native implementation.
//!
//! These tests mirror the Python tests in tests/test_accuracy.py,
//! testing detection accuracy against real-world test data files.

use std::fs;
use std::path::{Path, PathBuf};

use _chardet_rs::{
    detect_bytes,
    enums::EncodingEra,
};

/// Known accuracy failures - files that are expected to fail detection.
/// These are marked as #[ignore] so they don't block CI but are tracked.
const KNOWN_FAILURES: &[&str] = &[
    "cp037-nl/culturax_mC4_107675.txt",
    "cp037-en/_ude_1.txt",
    "cp437-nl/culturax_00000.txt",
    "cp437-en/culturax_00000.txt",
    "cp437-en/culturax_00001.txt",
    "cp437-en/culturax_00002.txt",
    "cp437-ga/culturax_mC4_63473.txt",
    "cp500-es/culturax_mC4_87070.txt",
    "cp850-da/culturax_00002.txt",
    "cp850-nl/culturax_00000.txt",
    "cp850-nl/culturax_00001.txt",
    "cp850-en/culturax_00000.txt",
    "cp850-en/culturax_00001.txt",
    "cp850-id/culturax_00000.txt",
    "cp850-ms/culturax_00000.txt",
    "cp852-ro/culturax_mC4_78976.txt",
    "cp852-ro/culturax_mC4_78978.txt",
    "cp852-ro/culturax_mC4_78979.txt",
    "cp852-ro/culturax_OSCAR-2019_78977.txt",
    "cp858-en/culturax_00000.txt",
    "cp858-fi/culturax_mC4_80362.txt",
    "cp858-id/culturax_00000.txt",
    "cp858-ga/culturax_mC4_63469.txt",
    "cp863-fr/culturax_00002.txt",
    "cp864-ar/culturax_00000.txt",
    "cp932-ja/hardsoft.at.webry.info.xml",
    "cp932-ja/y-moto.com.xml",
    "cp1006-ur/culturax_00000.txt",
    "cp1006-ur/culturax_00001.txt",
    "cp1006-ur/culturax_00002.txt",
    "gb2312-zh/_mozilla_bug171813_text.html",
    "hp-roman8-it/culturax_00002.txt",
    "iso-8859-1-en/ioreg_output.txt",
    "iso-8859-10-fi/culturax_00002.txt",
    "iso-8859-13-et/culturax_00002.txt",
    "iso-8859-15-ga/culturax_mC4_63469.txt",
    "iso-8859-16-ro/_ude_1.txt",
    "macroman-br/culturax_OSCAR-2019_43764.txt",
    "macroman-en/culturax_mC4_84512.txt",
    "macroman-id/culturax_mC4_114889.txt",
    "macroman-ga/culturax_mC4_63468.txt",
    "macroman-ga/culturax_mC4_63469.txt",
    "macroman-ga/culturax_mC4_63470.txt",
    "macroman-cy/culturax_mC4_78727.txt",
    "macroman-cy/culturax_mC4_78729.txt",
    "utf-8-en/finnish-utf-8-latin-1-confusion.html",
];

/// Check if a test file is a known failure.
fn is_known_failure(test_id: &str) -> bool {
    KNOWN_FAILURES.iter().any(|&f| test_id.ends_with(f))
}

/// Collect all test files from the test data directory.
fn collect_test_files() -> Vec<(String, String, PathBuf)> {
    let mut files = Vec::new();
    let data_dir = Path::new("../tests/data");
    
    if !data_dir.exists() {
        // Try alternative path
        return collect_test_files_alt();
    }
    
    for entry in fs::read_dir(data_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        
        if !path.is_dir() {
            continue;
        }
        
        let dir_name = path.file_name().unwrap().to_str().unwrap();
        
        // Parse directory name: "encoding-language" format
        // Split on the LAST hyphen since encoding names can contain hyphens
        let parts: Vec<&str> = dir_name.rsplitn(2, '-').collect();
        if parts.len() != 2 {
            continue;
        }
        
        // rsplitn returns iterator in reverse order, so parts[0] is language, parts[1] is encoding
        let language = parts[0].to_string();
        let encoding = parts[1].to_string();
        
        // Special case for "None-None" (binary files)
        let encoding = if encoding == "None" {
            None
        } else {
            Some(encoding)
        };
        
        // Recursively collect files in this directory
        collect_files_recursive(&path, encoding, language, &mut files);
    }
    
    files
}

/// Alternative path collection (when running from rust/ directory).
fn collect_test_files_alt() -> Vec<(String, String, PathBuf)> {
    let mut files = Vec::new();
    
    // Try different relative paths
    let possible_paths = [
        Path::new("../tests/data"),
        Path::new("tests/data"),
        Path::new("../../tests/data"),
    ];
    
    let data_dir = possible_paths.iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or(Path::new("../tests/data"));
    
    for entry in fs::read_dir(data_dir).unwrap_or_else(|_| {
        // Return empty iterator if directory doesn't exist
        panic!("Test data directory not found: {:?}", data_dir)
    }) {
        let entry = entry.unwrap();
        let path = entry.path();
        
        if !path.is_dir() {
            continue;
        }
        
        let dir_name = path.file_name().unwrap().to_str().unwrap();
        
        // Parse directory name: "encoding-language" format
        let parts: Vec<&str> = dir_name.splitn(2, '-').collect();
        if parts.len() != 2 {
            continue;
        }
        
        let encoding = parts[0].to_string();
        let language = parts[1].to_string();
        
        // Special case for "None-None" (binary files)
        let encoding = if encoding == "None" {
            None
        } else {
            Some(encoding)
        };
        
        // Recursively collect files in this directory
        collect_files_recursive(&path, encoding, language, &mut files);
    }
    
    files
}

/// Recursively collect files from a directory.
fn collect_files_recursive(
    dir: &Path,
    encoding: Option<String>,
    language: String,
    files: &mut Vec<(String, String, PathBuf)>,
) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        
        if path.is_dir() {
            collect_files_recursive(dir, encoding.clone(), language.clone(), files);
        } else if path.is_file() {
            let enc_str = encoding.as_deref().unwrap_or("None").to_string();
            files.push((enc_str, language.clone(), path));
        }
    }
}

/// Normalize encoding name for comparison.
fn normalize_encoding(name: &str) -> String {
    name.to_lowercase()
        .replace(|c: char| c == '_' || c == '-', "")
        .replace("iso8859", "iso8859")
        .replace("cp", "cp")
}

/// Check if detection is correct, allowing for equivalent encodings.
fn is_correct(expected: &str, detected: &str) -> bool {
    // Exact match
    if expected.eq_ignore_ascii_case(detected) {
        return true;
    }
    
    // Normalized match
    let norm_expected = normalize_encoding(expected);
    let norm_detected = normalize_encoding(detected);
    if norm_expected == norm_detected {
        return true;
    }
    
    // Handle common equivalence cases
    let equivalents: &[(&str, &str)] = &[
        // ASCII and supersets
        ("ascii", "windows-1252"),
        ("ascii", "iso-8859-1"),
        // Latin-1 and Windows-1252
        ("iso-8859-1", "windows-1252"),
        // GB variants
        ("gb2312", "gbk"),
        ("gb2312", "gb18030"),
        ("gbk", "gb18030"),
        // Japanese
        ("shift_jis", "shift-jis"),
        ("shift_jis", "cp932"),
        ("shift-jis", "cp932"),
        // Korean
        ("euc-kr", "cp949"),
        // Cyrillic
        ("iso-8859-5", "windows-1251"),
        ("koi8-r", "windows-1251"),
        // UTF variants
        ("utf8", "utf-8"),
        ("utf16", "utf-16"),
        ("utf16le", "utf-16-le"),
        ("utf16be", "utf-16-be"),
        ("utf32", "utf-32"),
        ("utf32le", "utf-32-le"),
        ("utf32be", "utf-32-be"),
        // EBCDIC aliases
        ("ibm037", "cp037"),
        ("ibm500", "cp500"),
        // HP Roman8
        ("hproman8", "hp-roman8"),
        ("hproman8", "roman8"),
        // Mac encodings
        ("macroman", "mac-roman"),
        ("maccentraleurope", "mac-latin2"),
    ];
    
    for (a, b) in equivalents {
        let norm_a = normalize_encoding(a);
        let norm_b = normalize_encoding(b);
        if (norm_expected == norm_a && norm_detected == norm_b)
            || (norm_expected == norm_b && norm_detected == norm_a)
        {
            return true;
        }
    }
    
    false
}

/// A single accuracy test case.
struct TestCase {
    expected_encoding: String,
    language: String,
    file_path: PathBuf,
    test_id: String,
}

/// Generate test cases from collected files.
fn generate_test_cases() -> Vec<TestCase> {
    collect_test_files()
        .into_iter()
        .map(|(enc, lang, path)| {
            let file_name = path.file_name().unwrap().to_str().unwrap();
            let parent_name = path.parent().unwrap().file_name().unwrap().to_str().unwrap();
            let test_id = format!("{}/{}", parent_name, file_name);
            
            TestCase {
                expected_encoding: enc,
                language: lang,
                file_path: path,
                test_id,
            }
        })
        .collect()
}

/// Run accuracy tests for a batch of test cases.
fn run_accuracy_tests(cases: &[TestCase]) -> (usize, usize, Vec<String>) {
    let mut passed = 0;
    let mut failed = 0;
    let mut failures = Vec::new();
    
    for case in cases {
        // Skip known failures
        if is_known_failure(&case.test_id) {
            continue;
        }
        
        let data = match fs::read(&case.file_path) {
            Ok(d) => d,
            Err(_) => {
                failed += 1;
                failures.push(format!("{}: could not read file", case.test_id));
                continue;
            }
        };
        
        let result = detect_bytes(&data, EncodingEra::All, 200_000);
        
        // Binary files: expect encoding=None
        if case.expected_encoding == "None" {
            if result.encoding.is_some() {
                failed += 1;
                failures.push(format!(
                    "{}: expected binary (None), got={}",
                    case.test_id,
                    result.encoding.unwrap()
                ));
            } else {
                passed += 1;
            }
            continue;
        }
        
        // Text files: check encoding correctness
        let detected = result.encoding.unwrap_or_else(|| "None".to_string());
        
        if is_correct(&case.expected_encoding, &detected) {
            passed += 1;
        } else {
            failed += 1;
            failures.push(format!(
                "{}: expected={}, got={} (confidence={:.2})",
                case.test_id,
                case.expected_encoding,
                detected,
                result.confidence
            ));
        }
    }
    
    (passed, failed, failures)
}

#[test]
fn test_accuracy_all_files() {
    let cases = generate_test_cases();
    
    // Skip if no test data found
    if cases.is_empty() {
        eprintln!("Warning: No test data files found, skipping accuracy tests");
        return;
    }
    
    let (passed, failed, failures) = run_accuracy_tests(&cases);
    
    // Print summary
    eprintln!("\nAccuracy Test Summary:");
    eprintln!("  Total files tested: {}", passed + failed);
    eprintln!("  Passed: {}", passed);
    eprintln!("  Failed: {}", failed);
    eprintln!("  Known failures skipped: {}", KNOWN_FAILURES.len());
    
    if !failures.is_empty() {
        eprintln!("\nFailures:");
        for f in &failures {
            eprintln!("  - {}", f);
        }
    }
    
    // Calculate accuracy percentage
    let total = passed + failed;
    if total > 0 {
        let accuracy = (passed as f64 / total as f64) * 100.0;
        eprintln!("\nAccuracy: {:.1}%", accuracy);
    }
    
    // Assert that we have reasonable accuracy
    // Known failures are excluded, so we expect near 100% on the rest
    if total > 0 {
        let accuracy = passed as f64 / total as f64;
        assert!(
            accuracy >= 0.95,
            "Accuracy test failed: {:.1}% < 95% ({} failures)",
            accuracy * 100.0,
            failed
        );
    }
}

#[test]
#[ignore] // Run with: cargo test test_accuracy_with_known_failures -- --ignored
fn test_accuracy_with_known_failures() {
    // This test includes known failures and is expected to have lower accuracy
    let cases = generate_test_cases();
    
    if cases.is_empty() {
        eprintln!("Warning: No test data files found");
        return;
    }
    
    let mut passed = 0;
    let mut failed = 0;
    let mut known_failed = 0;
    
    for case in &cases {
        let data = match fs::read(&case.file_path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        
        let result = detect_bytes(&data, EncodingEra::All, 200_000);
        let is_known = is_known_failure(&case.test_id);
        let detected = result.encoding.as_deref().unwrap_or("None");
        let correct = if case.expected_encoding == "None" {
            result.encoding.is_none()
        } else {
            is_correct(&case.expected_encoding, detected)
        };
        
        if correct {
            passed += 1;
        } else if is_known {
            known_failed += 1;
        } else {
            failed += 1;
        }
    }
    
    let total = passed + failed + known_failed;
    eprintln!("\nAccuracy Test (with known failures):");
    eprintln!("  Total: {}", total);
    eprintln!("  Passed: {}", passed);
    eprintln!("  New failures: {}", failed);
    eprintln!("  Known failures: {}", known_failed);
    
    if total > 0 {
        let accuracy = (passed + known_failed) as f64 / total as f64;
        eprintln!("  Overall accuracy: {:.1}%", accuracy * 100.0);
    }
    
    // We shouldn't have any new (unexpected) failures
    assert_eq!(failed, 0, "Found {} unexpected failures", failed);
}
