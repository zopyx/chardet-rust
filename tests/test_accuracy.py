# tests/test_accuracy.py
"""Accuracy evaluation against the chardet test suite.

Each test function is independently parametrized with its own xfail set.
Run with ``pytest -n auto`` for parallel execution.
"""

from __future__ import annotations

from pathlib import Path

import pytest
from utils import collect_test_files, get_data_dir, normalize_language

import chardet
from chardet import UniversalDetector
from chardet.enums import EncodingEra
from chardet.equivalences import (
    is_correct,
    is_equivalent_detection,
    is_language_equivalent,
)
from chardet.registry import REGISTRY

# ---------------------------------------------------------------------------
# Known accuracy failures — marked xfail so they don't block CI but are
# tracked for future improvement.  Kept sorted for easy diffing.
# ---------------------------------------------------------------------------

def _load_known_failures(filename: str) -> frozenset[str]:
    path = Path(__file__).with_name(filename)
    entries: set[str] = set()
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if line and not line.startswith("#"):
            entries.add(line)
    return frozenset(entries)


_KNOWN_FAILURES: frozenset[str] = _load_known_failures(
    "known_accuracy_failures.txt"
)

# Known failures when testing with era-filtered detection.
# Some overlap with _KNOWN_FAILURES (hard files that fail either way),
# some are unique (disambiguation is harder with fewer candidates),
# and many _KNOWN_FAILURES are absent (era filtering actually helps).
_KNOWN_ERA_FILTERED_FAILURES: frozenset[str] = frozenset(
    {
        "cp037-nl/culturax_mC4_107675.txt",
        "cp037-en/_ude_1.txt",
        "cp437-en/culturax_00002.txt",
        "cp500-es/culturax_mC4_87070.txt",
        "cp850-da/culturax_00002.txt",
        "cp850-nl/culturax_00000.txt",
        "cp852-ro/culturax_OSCAR-2019_78977.txt",
        "cp852-ro/culturax_mC4_78976.txt",
        "cp852-ro/culturax_mC4_78978.txt",
        "cp852-ro/culturax_mC4_78979.txt",
        "cp858-en/culturax_00000.txt",
        "cp858-fi/culturax_mC4_80362.txt",
        "cp863-fr/culturax_00002.txt",
        "cp864-ar/culturax_00000.txt",
        "cp932-ja/5554s2a-cp932.txt",
        "cp932-ja/hardsoft.at.webry.info.xml",
        "cp932-ja/y-moto.com.xml",
        "cp1006-ur/culturax_00000.txt",
        "cp1006-ur/culturax_00001.txt",
        "cp1006-ur/culturax_00002.txt",
        "gb18030-zh/_uchardet_gb18030.txt",
        "gb2312-zh/_mozilla_bug171813_text.html",
        "hp-roman8-it/culturax_00002.txt",
        "iso-8859-10-fi/culturax_00002.txt",
        "iso-8859-13-et/culturax_00002.txt",
        "iso-8859-15-ga/culturax_mC4_63469.txt",
        "iso-8859-16-hu/culturax_OSCAR-2019_82421.txt",
        "iso-8859-16-ro/_ude_1.txt",
        "macroman-da/culturax_mC4_83469.txt",
        "macroman-fi/culturax_mC4_80362.txt",
        "utf-16be-ja/culturax_mC4_5.txt",
        "utf-16be-zh/culturax_mC4_5.txt",
        "utf-16be-zh/culturax_mC4_7.txt",
        "utf-16le-ja/culturax_mC4_5.txt",
        "utf-16le-zh/culturax_mC4_5.txt",
        "utf-16le-zh/culturax_mC4_7.txt",
        "utf-8-en/finnish-utf-8-latin-1-confusion.html",
    }
)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def _encoding_era(name: str | None) -> EncodingEra:
    """Look up the encoding era for a test-data encoding name."""
    if name is None:
        return EncodingEra.ALL
    if name in REGISTRY:
        return REGISTRY[name].era
    lower = name.lower()
    for info in REGISTRY.values():
        if lower in (a.lower() for a in info.aliases):
            return info.era
    return EncodingEra.ALL


def _make_params(
    known_failures: frozenset[str],
) -> list[pytest.param]:
    """Build parametrize params from test data, marking known failures as xfail."""
    data_dir = get_data_dir()
    test_files = collect_test_files(data_dir)
    params = []
    for enc, lang, fp in test_files:
        test_id = f"{enc}-{lang}/{fp.name}"
        marks = []
        if test_id in known_failures:
            marks.append(pytest.mark.xfail(reason="known accuracy gap"))
        params.append(pytest.param(enc, lang, fp, marks=marks, id=test_id))
    return params


# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("expected_encoding", "language", "test_file_path"),
    _make_params(_KNOWN_FAILURES),
)
def test_detect(
    expected_encoding: str | None, language: str | None, test_file_path: Path
) -> None:
    """Detect encoding of a single test file and verify correctness."""
    data = test_file_path.read_bytes()
    result = chardet.detect(data, encoding_era=EncodingEra.ALL)
    detected = result["encoding"]

    # Binary files: expect encoding=None
    if expected_encoding is None:
        assert detected is None, (
            f"expected binary (None), got={detected} "
            f"(confidence={result['confidence']:.2f}, file={test_file_path.name})"
        )
        return

    assert is_correct(expected_encoding, detected) or is_equivalent_detection(
        data, expected_encoding, detected
    ), (
        f"expected={expected_encoding}, got={detected} "
        f"(confidence={result['confidence']:.2f}, "
        f"language={language}, file={test_file_path.name})"
    )

    # Language accuracy: informational only (don't fail test run)
    detected_language = normalize_language(result["language"])
    expected_language = language.lower()
    _ = detected_language is None or not is_language_equivalent(
        expected_language, detected_language
    )


@pytest.mark.parametrize(
    ("expected_encoding", "language", "test_file_path"),
    _make_params(_KNOWN_ERA_FILTERED_FAILURES),
)
def test_detect_era_filtered(
    expected_encoding: str | None, language: str | None, test_file_path: Path
) -> None:
    """Detect encoding using only the expected encoding's own era."""
    era = _encoding_era(expected_encoding)
    data = test_file_path.read_bytes()
    result = chardet.detect(data, encoding_era=era)
    detected = result["encoding"]

    # Binary files: expect encoding=None
    if expected_encoding is None:
        assert detected is None, (
            f"expected binary (None), got={detected} "
            f"(era={era!r}, confidence={result['confidence']:.2f}, "
            f"file={test_file_path.name})"
        )
        return

    assert is_correct(expected_encoding, detected) or is_equivalent_detection(
        data, expected_encoding, detected
    ), (
        f"expected={expected_encoding}, got={detected} "
        f"(era={era!r}, confidence={result['confidence']:.2f}, "
        f"language={language}, file={test_file_path.name})"
    )


@pytest.mark.parametrize(
    ("expected_encoding", "language", "test_file_path"),
    _make_params(frozenset()),
)
def test_detect_streaming_parity(
    expected_encoding: str | None, language: str | None, test_file_path: Path
) -> None:
    """UniversalDetector.feed/close must match chardet.detect (GH-296)."""
    data = test_file_path.read_bytes()
    direct = chardet.detect(data, encoding_era=EncodingEra.ALL)

    detector = UniversalDetector()
    detector.feed(data)
    streaming = detector.close()

    assert direct == streaming, (
        f"detect() != UniversalDetector for {test_file_path.name}: "
        f"detect={direct}, streaming={streaming}"
    )
