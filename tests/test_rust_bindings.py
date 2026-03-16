from __future__ import annotations

import pytest

pytest.importorskip("chardet_rs")
pytest.importorskip("chardet_rs._chardet_rs")

import chardet_rs
from chardet_rs import LanguageFilter


def test_rust_language_filter_matches_bitflag_values() -> None:
    assert LanguageFilter.CHINESE_SIMPLIFIED.value == 0x01
    assert LanguageFilter.CHINESE_TRADITIONAL.value == 0x02
    assert LanguageFilter.JAPANESE.value == 0x04
    assert LanguageFilter.KOREAN.value == 0x08
    assert LanguageFilter.NON_CJK.value == 0x10
    assert LanguageFilter.ALL.value == 0x1F
    assert LanguageFilter.CHINESE.value == 0x03
    assert LanguageFilter.CJK.value == 0x0F


def test_rust_language_filter_bitwise_or_matches_composites() -> None:
    assert (
        LanguageFilter.CHINESE_SIMPLIFIED | LanguageFilter.CHINESE_TRADITIONAL
        == LanguageFilter.CHINESE.value
    )
    assert (
        LanguageFilter.CHINESE | LanguageFilter.JAPANESE | LanguageFilter.KOREAN
        == LanguageFilter.CJK.value
    )


def test_rust_universal_detector_respects_max_bytes() -> None:
    data = b"1234567890+ZeVnLIqe-"
    detector = chardet_rs.UniversalDetector(max_bytes=10)
    detector.feed(data)

    result = detector.close()
    expected = chardet_rs.detect(data[:10], max_bytes=10)

    assert result == expected
