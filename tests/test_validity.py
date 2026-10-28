# tests/test_validity.py
from __future__ import annotations

from chardet.enums import EncodingEra
from chardet.pipeline.validity import filter_by_validity
from chardet.registry import get_candidates


def test_utf8_text_valid_under_utf8():
    data = "Héllo wörld".encode()
    candidates = get_candidates(EncodingEra.ALL)
    valid = filter_by_validity(data, candidates)
    valid_names = {e.name for e in valid}
    assert "utf-8" in valid_names


def test_latin1_text_invalid_under_strict_multibyte():
    data = "Héllo".encode("latin-1")
    candidates = get_candidates(EncodingEra.ALL)
    valid = filter_by_validity(data, candidates)
    valid_names = {e.name for e in valid}
    assert "iso8859-1" in valid_names


def test_shift_jis_text_valid_under_shift_jis():
    data = "こんにちは".encode("shift_jis")
    candidates = get_candidates(EncodingEra.ALL)
    valid = filter_by_validity(data, candidates)
    valid_names = {e.name for e in valid}
    assert "shift_jis_2004" in valid_names


def test_eliminates_impossible_encodings():
    data = "Привет".encode("windows-1251")
    candidates = get_candidates(EncodingEra.ALL)
    valid = filter_by_validity(data, candidates)
    assert len(valid) < len(candidates)


def test_empty_input_returns_all():
    candidates = get_candidates(EncodingEra.MODERN_WEB)
    valid = filter_by_validity(b"", candidates)
    assert len(valid) == len(candidates)


def test_eliminates_specific_encoding():
    """Invalid UTF-8 bytes should eliminate UTF-8 from the candidate list."""
    data = b"\xc3\x28"  # invalid UTF-8 (bad continuation byte)
    candidates = get_candidates(EncodingEra.ALL)
    valid = filter_by_validity(data, candidates)
    valid_names = {e.name for e in valid}
    assert "utf-8" not in valid_names


def test_all_eliminated_returns_empty():
    """When no candidate can decode the data, return an empty tuple."""
    data = b"\xff\xfe"  # invalid under UTF-8
    utf8_only = tuple(e for e in get_candidates(EncodingEra.ALL) if e.name == "utf-8")
    assert len(utf8_only) == 1
    valid = filter_by_validity(data, utf8_only)
    assert valid == ()


def test_returns_tuple():
    candidates = get_candidates(EncodingEra.MODERN_WEB)
    valid = filter_by_validity(b"Hello", candidates)
    assert isinstance(valid, tuple)
