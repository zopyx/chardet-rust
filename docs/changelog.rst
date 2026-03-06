Changelog
=========

7.0.0 (2026-03-02)
-------------------

Ground-up, LGPL-licensed rewrite of chardet. Same package name, same
public API — drop-in replacement for chardet 5.x/6.x.

**Highlights:**

- **LGPL license**
- **96.8% accuracy** on 2,179 test files (+2.3pp vs chardet 6.0.0,
  +7.7pp vs charset-normalizer)
- **41x faster** than chardet 6.0.0 and **7.5x faster** than
  charset-normalizer on the project benchmark suite
- **Language detection** for every result (90.5% accuracy across 49
  languages)
- **99 encodings** across six eras (MODERN_WEB, LEGACY_ISO, LEGACY_MAC,
  LEGACY_REGIONAL, DOS, MAINFRAME)
- **12-stage detection pipeline** — BOM, UTF-16/32 patterns, escape
  sequences, binary detection, markup charset, ASCII, UTF-8 validation,
  byte validity, CJK gating, structural probing, statistical scoring,
  post-processing
- **Bigram frequency models** trained on CulturaX multilingual corpus
  data for all supported language/encoding pairs
- **Rust core with Python bindings** via PyO3 (``chardet_rs._chardet_rs``)
- **Thread-safe** ``detect()`` and ``detect_all()`` with no measurable
  overhead; scales on free-threaded Python 3.13t+
- **Negligible import memory** (96 B)
- **Zero runtime dependencies**

**Breaking changes vs 6.0.0:**

- ``detect()`` and ``detect_all()`` now default to
  ``encoding_era=EncodingEra.ALL`` (6.0.0 defaulted to ``MODERN_WEB``)
- Internal architecture is completely different (probers replaced by
  pipeline stages). Only the public API is preserved.
- ``LanguageFilter`` is accepted but ignored (deprecation warning
  emitted)
- ``chunk_size`` is accepted but ignored (deprecation warning emitted)

6.0.0 (2026-02-22)
-------------------

**Features:**

- Unified single-byte charset detection with proper language-specific
  bigram models for all single-byte encodings (replaces ``Latin1Prober``
  and ``MacRomanProber`` heuristics)
- 38 new languages: Arabic, Belarusian, Breton, Croatian, Czech, Danish,
  Dutch, English, Esperanto, Estonian, Farsi, Finnish, French, German,
  Icelandic, Indonesian, Irish, Italian, Kazakh, Latvian, Lithuanian,
  Macedonian, Malay, Maltese, Norwegian, Polish, Portuguese, Romanian,
  Scottish Gaelic, Serbian, Slovak, Slovene, Spanish, Swedish, Tajik,
  Ukrainian, Vietnamese, Welsh
- ``EncodingEra`` filtering via new ``encoding_era`` parameter
- ``max_bytes`` and ``chunk_size`` parameters for ``detect()``,
  ``detect_all()``, and ``UniversalDetector``
- ``-e``/``--encoding-era`` CLI flag
- EBCDIC detection (CP037, CP500)
- Direct GB18030 support (replaces redundant GB2312 prober)
- Binary file detection
- Python 3.12, 3.13, and 3.14 support

**Breaking changes:**

- Dropped Python 3.7, 3.8, and 3.9 (requires Python 3.10+)
- Removed ``Latin1Prober`` and ``MacRomanProber``
- Removed EUC-TW support
- Removed ``LanguageFilter.NONE``
- ``detect()`` default changed to ``encoding_era=EncodingEra.MODERN_WEB``

**Fixes:**

- Fixed CP949 state machine
- Fixed SJIS distribution analysis (second-byte range >= 0x80)
- Fixed UTF-16/32 detection for non-ASCII-heavy text
- Fixed GB18030 ``char_len_table``
- Fixed UTF-8 state machine
- Fixed ``detect_all()`` returning inactive probers
- Fixed early cutoff bug

5.2.0 (2023-08-01)
-------------------

- Added support for running the CLI via ``python -m chardet``

5.1.0 (2022-12-01)
-------------------

- Added ``should_rename_legacy`` argument to remap legacy encoding names
  to modern equivalents
- Added MacRoman encoding prober
- Added ``--minimal`` flag to ``chardetect`` CLI
- Added type annotations and mypy CI
- Added support for Python 3.11
- Removed support for Python 3.6

5.0.0 (2022-06-25)
-------------------

- Added Johab Korean prober
- Added UTF-16/32 BE/LE probers
- Added test data for Croatian, Czech, Hungarian, Polish, Slovak,
  Slovene, Greek, Turkish
- Improved XML tag filtering
- Made ``detect_all`` return child prober confidences
- Dropped Python 2.7, 3.4, 3.5 (requires Python 3.6+)

4.0.0 (2020-12-10)
-------------------

- Added ``detect_all()`` function returning all candidate encodings
- Converted single-byte charset probers to nested dicts (performance)
- ``CharsetGroupProber`` now short-circuits on definite matches
  (performance)
- Added ``language`` field to ``detect_all`` output
- Dropped Python 2.6, 3.4, 3.5

3.0.4 (2017-06-08)
-------------------

- Fixed packaging issue with ``pytest_runner``
- Updated old URLs in README and docs

3.0.3 (2017-05-16)
-------------------

- Fixed crash when debug logging was enabled

3.0.2 (2017-04-12)
-------------------

- Fixed ``detect`` sometimes returning ``None`` instead of a result dict

3.0.1 (2017-04-11)
-------------------

- Fixed crash in EUC-TW prober with certain strings

3.0.0 (2017-04-11)
-------------------

- Added Turkish ISO-8859-9 detection
- Modernized naming conventions (``typical_positive_ratio`` instead of
  ``mTypicalPositiveRatio``)
- Added ``language`` property to probers and results
- Switched from Travis to GitHub Actions
- Fixed ``CharsetGroupProber.state`` not being set to ``FOUND_IT``

2.3.0 (2014-10-07)
-------------------

- Added CP932 detection
- Fixed UTF-8 BOM not detected as UTF-8-SIG
- Switched ``chardetect`` to use ``argparse``

2.2.1 (2013-12-18)
-------------------

- Fixed missing parenthesis in ``chardetect.py``

2.2.0 (2013-12-16)
-------------------

- First release after merger with charade (Python 3 support)

2.1.1 (2012-10-01)
-------------------

- Bumped version past Mark Pilgrim's last release
- ``chardetect`` can now read from stdin (Erik Rose)
- Fixed BOM byte strings for UCS-4-2143 and UCS-4-3412 (Toshio Kuratomi)
- Restored Mark Pilgrim's original docs and COPYING file (Toshio Kuratomi)

1.1 (2012-07-27)
-----------------

- Added ``chardetect`` CLI tool (Erik Rose)
- Fixed ``utf8prober`` crash when character is out of range (David Cramer)
- Cleaned up detection logic to fail gracefully (David Cramer)
- Fixed feed encoding errors (David Cramer)

1.0.1 (2008-04-19)
-------------------

- Packaging fix, added egg distributions for Python 2.4 and 2.5
  (Mark Pilgrim)

1.0 (2006-12-23)
-----------------

- Initial release: Python 2 port of Mozilla's universal charset detector
  (Mark Pilgrim)
