"""Enumerations for chardet.

These provide Python IntFlag compatibility while wrapping the Rust implementation.
"""

from __future__ import annotations

import enum

# Import the underlying Rust enum for use in detection
from chardet_rs._chardet_rs import (
    EncodingEra as _EncodingEra,
)
from chardet_rs._chardet_rs import (
    LanguageFilter as _LanguageFilter,
)


class EncodingEra(enum.IntFlag):
    """Bit flags representing encoding eras for filtering detection candidates."""

    MODERN_WEB = 1
    LEGACY_ISO = 2
    LEGACY_MAC = 4
    LEGACY_REGIONAL = 8
    DOS = 16
    MAINFRAME = 32
    ALL = 63


class LanguageFilter(enum.IntFlag):
    """Language filter flags for UniversalDetector (chardet 6.x API compat)."""

    CHINESE_SIMPLIFIED = 0x01
    CHINESE_TRADITIONAL = 0x02
    JAPANESE = 0x04
    KOREAN = 0x08
    NON_CJK = 0x10
    ALL = 0x1F
    CHINESE = 0x03
    CJK = 0x0F


# Mapping from Python enum to Rust enum for internal use
_ENCODING_ERA_MAP: dict[EncodingEra, _EncodingEra] = {
    EncodingEra.MODERN_WEB: _EncodingEra.MODERN_WEB,
    EncodingEra.LEGACY_ISO: _EncodingEra.LEGACY_ISO,
    EncodingEra.LEGACY_MAC: _EncodingEra.LEGACY_MAC,
    EncodingEra.LEGACY_REGIONAL: _EncodingEra.LEGACY_REGIONAL,
    EncodingEra.DOS: _EncodingEra.DOS,
    EncodingEra.MAINFRAME: _EncodingEra.MAINFRAME,
    EncodingEra.ALL: _EncodingEra.ALL,
}

if hasattr(_LanguageFilter, "CJK"):
    _RUST_LANGUAGE_FILTER_CJK = _LanguageFilter.CJK
else:
    _RUST_LANGUAGE_FILTER_CJK = _LanguageFilter.ALL_CJK

_LANGUAGE_FILTER_MAP: dict[LanguageFilter, _LanguageFilter] = {
    LanguageFilter.CHINESE_SIMPLIFIED: _LanguageFilter.CHINESE_SIMPLIFIED,
    LanguageFilter.CHINESE_TRADITIONAL: _LanguageFilter.CHINESE_TRADITIONAL,
    LanguageFilter.JAPANESE: _LanguageFilter.JAPANESE,
    LanguageFilter.KOREAN: _LanguageFilter.KOREAN,
    LanguageFilter.NON_CJK: _LanguageFilter.NON_CJK,
    LanguageFilter.ALL: _LanguageFilter.ALL,
    LanguageFilter.CHINESE: _LanguageFilter.CHINESE,
    LanguageFilter.CJK: _RUST_LANGUAGE_FILTER_CJK,
}


def _to_rust_encoding_era(era: EncodingEra) -> _EncodingEra:
    """Convert Python EncodingEra to Rust _EncodingEra."""
    return _ENCODING_ERA_MAP.get(era, _EncodingEra.ALL)


def _to_rust_language_filter(filter: LanguageFilter) -> _LanguageFilter:
    """Convert Python LanguageFilter to Rust _LanguageFilter."""
    return _LANGUAGE_FILTER_MAP.get(filter, _LanguageFilter.ALL)


__all__ = [
    "EncodingEra",
    "LanguageFilter",
    "_to_rust_encoding_era",
    "_to_rust_language_filter",
]
