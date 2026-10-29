# Changelog

All notable changes to chardet-rust are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.5] - 2026-10-29

### Fixed

- `is_valid_utf7_b64` (`rust/src/pipeline/escape.rs`) built its padding mask as
  `1u8 << padding_bits`. `padding_bits` is `n * 6 % 16` and reaches 14, so the
  shift overflows a `u8`: builds with overflow checks abort the interpreter with
  `pyo3_runtime.PanicException: attempt to shift left with overflow`, and
  release builds only avoid it because the wrapped `255` mask happens to give
  the same verdict for a 6-bit sextet. Plain ASCII carrying a `+` followed by a
  five-character base64 run was enough to trigger it, e.g.
  `ID|AMOUNT|NAME\n16847+|100|SMITH,+JONES`. The mask is computed in `u32` now,
  matching the Python fallback, which has no width limit. Covered by
  `tests/test_github_issues.py::TestEscapeSequences::test_plus_uppercase_word_not_utf7`.

### Added

- Linux wheels for CPython 3.11–3.15 on `x86_64` (`manylinux`). 0.2.4 shipped
  only `cp313-macosx_11_0_arm64`, so Linux installations fell back to the sdist
  and needed a Rust toolchain.

## [0.2.4] - 2026-10-29

Documentation-only release; no code changes. The PyPI project page renders
`README.md`, and that file still carried the upstream 7.0 figures that the
benchmark work had already superseded.

### Changed

- `README.md` publishes the measured numbers — 99.7% accuracy on 3,137 files,
  4,394 files/s, 2.3x chardet 7.7.0 (mypyc) — instead of the inherited
  "98.1%, 43x faster than chardet 6.0.0, 6.8x faster than
  charset-normalizer" claims, and drops the free-threaded CI claim, which no
  longer holds: the job was removed and PyO3 0.23 does not build for
  free-threaded CPython.
- `README.md` fixes two stale pointers: the extension is built with
  `setuptools-rust`, not `maturin`, and the documentation lives at
  `chardet.readthedocs.io` (`chardet-rust.readthedocs.io` answers 404).

## [0.2.3] - 2026-10-29

No functional changes. The tree is identical to 0.2.2 apart from this version
number: 0.2.2 stopped after its TestPyPI verification, so there was nothing to
fix before the PyPI publication.

## [0.2.2] - 2026-10-29

The detection engine is unchanged. This release corrects the benchmark harness
that produced the published speed numbers — repository tooling, it is not part
of the sdist or the wheel — and republishes the numbers that harness distorted.

### Fixed

- `scripts/compare_detectors.py` timed every detector concurrently in a thread
  pool. The subprocesses are CPU-bound, so each measurement was inflated by the
  number of detectors in flight and the ratios were meaningless. Timing now
  runs sequentially.
- The same script's "old chardet is too slow for three runs" shortcut keyed on
  `major < 7`, which also matched the local build: chardet-rust reports 0.x, so
  the detector under test was timed once, cold, and looked roughly twice as
  slow as it is. The shortcut now applies to chardet 5 and 6 only.

### Changed

- `docs/performance.rst`, `docs/index.rst` and `PERFORMANCE_COMPARISON.md`
  publish the re-measured comparison: 99.7% encoding accuracy, identical to
  chardet 7.7.0 file for file, at 4,394 files/s — 2.3x the reference's mypyc
  build and 2.8x charset-normalizer. A Baselines section adds the pure-Python
  (16x) and chardet 6.0.0 (300x) figures and marks the inherited
  21.9x/113x/"43x" claims as superseded.
- `wheelhouse/`, cibuildwheel's local output directory, is ignored.

## [0.2.1] - 2026-10-29

First release of the 7.7 port that is published from artifacts verified end to
end: sdist and wheel are built from a clean tree, `twine check` passes, and both
install into an empty environment. Detection behaviour is unchanged from 0.2.0,
which was uploaded to TestPyPI only.

### Changed

- The `Changelog` project URL points at this file instead of the vendored
  reference's documentation page.

## [0.2.0] - 2026-10-28

Detection engine ported to chardet 7.7. The Rust core now reproduces the
reference pipeline stage for stage, and the reference itself is vendored under
`src/chardet` so the Python-facing surface and its tests keep working.

### Added

- Rust ports of the escape stage (ISO-2022, HZ-GB2312, UTF-7 with the RFC 2152
  padding/surrogate guards and the whole-window decode gate), the magic-number
  stage, UTF-16/32 null-pattern detection, markup declarations (including the
  cp037 scan for EBCDIC markup and PEP 263 cookies), language fill, the
  postprocess rank corrections and confusion arbitration.
- Generated lookup tables instead of hand-maintained ones: the encoding
  registry, the codec-alias table, per-encoding case/category/code-point tables,
  the cp037 page, and per-encoding byte-pair grammars. They exist because
  `encoding_rs` follows the WHATWG mappings — its `shift_jis` is CP932 — while
  the reference asks the Python codec, so validity checks needed the codecs'
  own answers. Regenerate with `scripts/gen_encoding_tables.py` and
  `scripts/gen_codec_aliases.py`.
- `scripts/rust_parity.py` for Rust-vs-reference parity runs over the corpus.
- Python 3.15 support (the abi3 extension runs unchanged; verified on 3.15.2).
- `chardet._version`, tracked in-tree: the vendored package imports it and
  setuptools does not generate it.

### Changed

- `src/chardet` is now the vendored chardet 7.7 reference implementation. The
  public API routes detection through the Rust pipeline in `chardet._request`,
  while the Python pipeline stays available for the internals tests.
- `chardet.__version__` and `chardet_rs.__version__` report the installed
  distribution version instead of separate hard-coded numbers.
- `chardet.UniversalDetector` keeps the reference's fallback semantics: an
  excluded fallback encoding warns at the caller's frame.

### Fixed

- `mime_type` survived no stage rebuild, so declared HTML pages came back as
  `text/plain`; the language fill, postprocess and confusion stages now carry it.
- UTF-7 was claimed for plain ASCII (`+/v8` diffs, `+row`, `+100`): the marker,
  the base64 padding bits, embedded-base64 and script-range guards are all
  enforced again, and the whole window must decode.
- Null-separated ASCII (`find -print0` output) was classified as binary, and a
  window cut mid-sequence was not treated as valid UTF-8.
- The CJK gate used thresholds that dropped two-byte CJK input entirely; the
  reference's 0.05/2/0.35/4 values are in place.
- Decode-safety: a winner that cannot decode the caller's input (a trailing
  cut sequence, `mam\xe1`) now yields to a rival that can.
- Niche-Latin demotion arbitrated on the full table difference instead of the
  reference's curated byte sets, so `hp-roman8` won ties it should have lost.
- EBCDIC text with HT/NL whitespace was classified as binary.
- The packaged `chardet_rs` wrapper (`src/chardet_rs/__init__.py`) had drifted
  from the copy the tests import (`rust/chardet_rs/__init__.py`) and called
  `_load_models` with one argument, so an installed wheel raised `TypeError` on
  `import chardet`. The shipped module now matches the tested one.

### Removed

- `chardet._fallback` and `chardet.universal_detector`: dead leftovers of the
  previous implementation (`_fallback` no longer imported at all, nothing
  referenced either module).

### Build and CI

- Version the generated Rust tables and `chardet._version`; `.gitignore` had
  swallowed them, so a clean checkout could not compile the crate.
- The sdist carries `rust/src/data/*.bin`, without which it cannot build, and
  `src/chardet/models/models.bin.sha256`, which `bigram_models.rs` includes: a
  wheel built from the sdist failed at compile time without it.
- CI: dropped the free-threaded job (PyO3 0.23 does not build for free-threaded
  CPython below 3.14), added 3.15 to the matrix, and the test job no longer
  syncs the dev group's data tooling.

## [0.1.12] - 2026-03-15

### Security

- **SEC-020**: Consolidated duplicate `UniversalDetector` implementations
  - Created unified `UniversalDetector` with automatic backend selection
  - Automatically selects Rust (performance) or Python fallback
  - Moved pure Python implementation to `chardet._fallback`
  - Deprecated `chardet.detector` (emits `DeprecationWarning`)
  - Single entry point for all detection needs

### Changed

- `chardet.UniversalDetector` now has a `backend` property showing 'rust' or 'python'
- `chardet.detector` module is deprecated; use `chardet.UniversalDetector` instead

## [0.1.11] - 2026-03-15

### Security

- **SEC-001**: Added comprehensive input validation at Rust FFI boundary
  - Added `validate_max_bytes()` function with 100 MB limit to prevent memory exhaustion
  - Added `validate_byte_input()` function for input validation
  - All `detect()` and `detect_all()` functions now validate inputs before processing
  - Added `MAX_BYTES_LIMIT` constant (100 MB)

- **SEC-002**: Implemented file size and symlink checks in CLI
  - Added `_validate_file_path()` function with security checks
  - Symlinks are now rejected to prevent symlink attacks
  - File size limited to 50 MB to prevent DoS via large files
  - Directory and special file detection added
  - Added `_MAX_FILE_SIZE` constant (50 MB)

- **SEC-003**: Added iteration limits to `UniversalDetector.feed()`
  - Maximum 1,000,000 feed() calls per detector instance
  - Individual feed() input limited to 50 MB
  - Prevents denial-of-service via excessive iteration
  - Added `MAX_FEED_CALLS` and `MAX_FEED_SIZE` constants

- **SEC-004**: Validated `max_bytes` parameter consistently in Rust
  - Both Rust implementations (`py.rs` and `detector.rs`) now validate max_bytes
  - Consistent error messages across Python and Rust boundaries
  - Zero/negative max_bytes values now properly rejected

- **SEC-011**: Replaced broad exceptions with specific exception types
  - Added custom exception hierarchy in `src/chardet/exceptions.py`:
    - `ChardetError`: Base exception for all chardet errors
    - `DetectionError`: For detection pipeline failures
    - `InvalidInputError`: For input validation failures
    - `ConfigurationError`: For configuration/initialization failures
    - `FileError`: For file operation failures (avoids leaking internal paths)
  - Updated CLI to catch specific exceptions (`ValueError`, `UnicodeDecodeError`, `ChardetError`)
  - Updated model loading to catch specific exceptions (`OSError`, `ImportError`)
  - Error messages no longer expose internal implementation details

### Breaking Changes

- **SEC-010**: Removed deprecated `chunk_size` and `lang_filter` parameters
  - Removed `chunk_size` parameter from `detect()` and `detect_all()` functions
  - Removed `lang_filter` parameter from `UniversalDetector` class
  - Removed deprecation warning code for these parameters
  - These parameters were deprecated and had no effect; their removal simplifies the API

### Changed

- `UniversalDetector` struct (Rust) now tracks `feed_count` and `max_feed_calls`
- CLI file handling now uses `_validate_file_path()` before opening files
- Error messages now include specific limits and thresholds
- Increased `MAX_FEED_SIZE` from 10 MB to 50 MB to support large test files

### Fixed

- Rust `UniversalDetector::new()` now returns `PyResult` to properly propagate validation errors
- CLI no longer exposes internal paths in error messages
- Accuracy tests with large files (~17 MB) now work correctly with increased feed size limit

---

## [0.1.10] - 2026-03-15

### Changed

- Updated security documentation in SECURITY.md
- Added comprehensive security backlog with 15 tracked issues

---

## [0.1.9] - 2026-03-10

### Added

- Performance improvements for bigram model loading
- Enhanced language detection accuracy

---

## [0.1.8] - 2026-03-05

### Fixed

- Fixed UTF-8 detection edge cases with ANSI control bytes
- Improved CJK gating for short inputs

---

## [0.1.7] - 2026-02-28

### Added

- Added encoding era filtering support
- Added language detection for all results

### Changed

- Improved statistical scoring for multi-byte encodings

---

## [0.1.6] - 2026-02-20

### Fixed

- Fixed confusion resolution for ISO-8859 variants
- Improved KOI8-T vs KOI8-R discrimination

---

## [0.1.5] - 2026-02-15

### Added

- Added markup charset extraction for HTML/XML
- Added escape sequence detection (ISO-2022, HZ-GB-2312, UTF-7)

---

## [0.1.4] - 2026-02-10

### Fixed

- Fixed binary detection false positives with UTF-8 multi-byte sequences
- Improved null byte threshold handling

---

## [0.1.3] - 2026-02-05

### Added

- Added UTF-16/32 pattern detection without BOM
- Added structural score caching for performance

---

## [0.1.2] - 2026-01-30

### Fixed

- Fixed legacy encoding name remapping
- Improved Windows-1252 detection accuracy

---

## [0.1.1] - 2026-01-25

### Added

- Added streaming detection with `UniversalDetector`
- Added `detect_all()` for multiple candidate encodings

---

## [0.1.0] - 2026-01-20

### Added

- Initial Rust-powered release
- Multi-stage detection pipeline
- Bigram statistical models
- Python API compatibility with chardet 6.x

---

## Version History Summary

| Version | Release Date | Key Changes |
|---------|-------------|-------------|
| 0.2.4 | 2026-10-29 | README carries the measured numbers (PyPI page) |
| 0.2.3 | 2026-10-29 | No functional changes over 0.2.2 |
| 0.2.2 | 2026-10-29 | Corrected benchmark harness, re-measured performance numbers |
| 0.2.1 | 2026-10-29 | 7.7 port released from verified artifacts; Changelog URL |
| 0.2.0 | 2026-10-28 | Detection engine ported to chardet 7.7; Python 3.15 |
| 0.1.12 | 2026-03-15 | SEC-020: Consolidated UniversalDetector implementations |
| 0.1.11 | 2026-03-15 | Security audit, input validation, exception hierarchy |
| 0.1.10 | 2026-03-15 | Security hardening |
| 0.1.9 | 2026-03-10 | Performance improvements |
| 0.1.8 | 2026-03-05 | UTF-8 edge cases |
| 0.1.7 | 2026-02-28 | Encoding era filtering |
| 0.1.6 | 2026-02-20 | Confusion resolution |
| 0.1.5 | 2026-02-15 | Markup/escape detection |
| 0.1.4 | 2026-02-10 | Binary detection fixes |
| 0.1.3 | 2026-02-05 | UTF-16/32 detection |
| 0.1.2 | 2026-01-30 | Legacy remapping |
| 0.1.1 | 2026-01-25 | Streaming detection |
| 0.1.0 | 2026-01-20 | Initial release |

---

*Last updated: March 15, 2026*
