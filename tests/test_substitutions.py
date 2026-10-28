"""Tests for scripts/substitutions.py encoding-aware filtering."""

from __future__ import annotations

import sys

import pytest

sys.path.insert(0, "scripts")
from substitutions import get_substitutions


class TestEncodingAwareFiltering:
    """get_substitutions only substitutes unencodable characters."""

    def test_smart_quotes_preserved_for_windows_1252(self):
        """Windows-1252 encodes smart quotes — they should NOT be substituted."""
        subs = get_substitutions("windows-1252", ["en"])
        # U+201C LEFT DOUBLE QUOTATION MARK is byte 0x93 in Windows-1252
        assert "\u201c" not in subs
        # U+201D RIGHT DOUBLE QUOTATION MARK is byte 0x94 in Windows-1252
        assert "\u201d" not in subs

    def test_smart_quotes_substituted_for_iso_8859_1(self):
        """ISO-8859-1 cannot encode smart quotes — they SHOULD be substituted."""
        subs = get_substitutions("iso-8859-1", ["en"])
        assert "\u201c" in subs
        assert "\u201d" in subs

    def test_em_dash_preserved_for_windows_1252(self):
        """Windows-1252 encodes em dash (0x97) — should NOT be substituted."""
        subs = get_substitutions("windows-1252", ["en"])
        assert "\u2014" not in subs

    def test_em_dash_substituted_for_iso_8859_1(self):
        """ISO-8859-1 cannot encode em dash — should be substituted."""
        subs = get_substitutions("iso-8859-1", ["en"])
        assert "\u2014" in subs

    def test_nbsp_preserved_for_iso_8859_1(self):
        """ISO-8859-1 encodes NBSP (0xA0) — should NOT be substituted."""
        subs = get_substitutions("iso-8859-1", ["en"])
        assert "\u00a0" not in subs

    def test_zero_width_chars_substituted_for_all(self):
        """Zero-width characters are unencodable in single-byte encodings."""
        for enc in ("windows-1252", "iso-8859-1", "koi8-r"):
            subs = get_substitutions(enc, ["en"])
            assert "\u200b" in subs, f"ZWSP should be substituted for {enc}"

    def test_arabic_comma_preserved_for_cp864(self):
        """CP864 encodes Arabic comma (U+060C) — should NOT be substituted."""
        subs = get_substitutions("cp864", ["ar"])
        assert "\u060c" not in subs

    def test_arabic_comma_substituted_for_cp720(self):
        """CP720 cannot encode Arabic comma — should be substituted."""
        subs = get_substitutions("cp720", ["ar"])
        assert "\u060c" in subs

    def test_cp866_cyrillic_subs_kept(self):
        """CP866 Cyrillic substitutions survive — source chars are not in CP866."""
        subs = get_substitutions("cp866", ["ru"])
        # Ukrainian U+0456 -> U+0438 -- U+0456 is NOT in CP866
        assert "\u0456" in subs

    def test_invalid_codec_raises(self):
        """Invalid charset_name should raise LookupError, not silently degrade."""
        with pytest.raises(LookupError):
            get_substitutions("not-a-real-encoding", ["en"])

    def test_all_encoding_specific_pairs_succeed(self):
        """Verify get_substitutions runs without error for all encoding/lang pairs."""
        encoding_lang_pairs = [
            ("cp720", ["ar"]),
            ("cp864", ["ar"]),
            ("iso-8859-6", ["ar"]),
            ("cp720", ["fa"]),
            ("iso-8859-6", ["fa"]),
            ("cp1256", ["fa"]),
            ("cp1006", ["ar"]),
            ("cp866", ["ru"]),
            ("iso-8859-2", ["ro"]),
            ("windows-1250", ["ro"]),
        ]
        for enc, langs in encoding_lang_pairs:
            get_substitutions(enc, langs)  # should not raise
