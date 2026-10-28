#!/usr/bin/env python3
"""Generate the Rust encoding tables from Python's codecs and unicodedata.

The confusion stage reasons about *how* each encoding reads a byte: its
Unicode general category (letter / punctuation / symbol preference), whether
it is a letter and in which case, and whether two encodings read a byte
differently at all. Python answers those from `codecs` and `unicodedata`;
Rust has neither for EBCDIC and DOS code pages, so the answers are generated
here once and shipped as a compact binary the Rust side loads at startup.

Output: ``rust/src/data/encoding_tables.bin`` (``CET1``).

Layout:
    magic           4 bytes   b"CET1"
    num_encodings   u32 BE
    per encoding:
        name_len    u8
        name        name_len bytes, UTF-8
        256 x (case: u8, category: u8, char_id: u16 BE)

``case``: 0 = not a letter, 1 = uppercase letter, 2 = other letter
(combining marks count as letters, as in the reference).
``category``: index into the reference's category table (0 = Lu ... 29 = Cn).
``char_id``: stable id of the decoded character, 0xFFFF when the byte does not
decode; two encodings read a byte differently iff their ids differ.

Usage:
    PYTHONPATH=src uv run python scripts/gen_encoding_tables.py
"""

from __future__ import annotations

import struct
import unicodedata
from pathlib import Path

from chardet.registry import REGISTRY

CATEGORIES = [
    "Lu", "Ll", "Lt", "Lm", "Lo", "Mn", "Mc", "Me", "Nd", "Nl",
    "No", "Pc", "Pd", "Ps", "Pe", "Pi", "Pf", "Po", "Sm", "Sc",
    "Sk", "So", "Zs", "Zl", "Zp", "Cc", "Cf", "Cs", "Co", "Cn",
]
CATEGORY_TO_INT = {name: i for i, name in enumerate(CATEGORIES)}
UNDECODABLE_CHAR_ID = 0xFFFF


def main() -> int:
    char_ids: dict[str, int] = {}
    blocks: list[bytes] = []
    names: list[str] = []

    for encoding in sorted(REGISTRY):
        name = encoding.encode("utf-8")
        rows = bytearray()
        for byte in range(256):
            try:
                text = bytes((byte,)).decode(encoding)
            except (UnicodeDecodeError, LookupError):
                rows += struct.pack("!BBH", 0, CATEGORY_TO_INT["Cn"], UNDECODABLE_CHAR_ID)
                continue

            if len(text) != 1:
                # Stateful codecs can consume a byte without yielding a
                # character; the reference treats that as no evidence.
                rows += struct.pack("!BBH", 0, CATEGORY_TO_INT["Cn"], UNDECODABLE_CHAR_ID)
                continue

            category = unicodedata.category(text)
            if category == "Lu":
                case = 1
            elif category[0] == "L" or category in ("Mn", "Mc"):
                case = 2
            else:
                case = 0

            char_id = char_ids.get(text)
            if char_id is None:
                char_id = len(char_ids)
                char_ids[text] = char_id

            rows += struct.pack("!BBH", case, CATEGORY_TO_INT.get(category, CATEGORY_TO_INT["Cn"]), char_id)

        if len(rows) != 256 * 4:
            msg = "row block must hold 256 four-byte rows"
            raise ValueError(msg)
        blocks.append(bytes(rows))
        names.append(name.decode())

    out = bytearray(b"CET1")
    out += struct.pack("!I", len(blocks))
    for name, rows in zip(names, blocks, strict=True):
        encoded = name.encode("utf-8")
        out += struct.pack("!B", len(encoded)) + encoded + rows

    target = Path("rust/src/data/encoding_tables.bin")
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(bytes(out))
    print(f"wrote {target} ({len(out)} bytes, {len(blocks)} encodings, {len(char_ids)} distinct characters)")

    # cp037 is the page the reference decodes EBCDIC markup heads through: its
    # letter, digit and tag-anchor positions are shared by every supported
    # EBCDIC variant, so one table answers the markup scan for all of them.
    rows = bytearray()
    for byte in range(256):
        try:
            text = bytes((byte,)).decode("cp037")
            code_point = ord(text) if len(text) == 1 else UNDECODABLE_CHAR_ID
        except (UnicodeDecodeError, LookupError):
            code_point = UNDECODABLE_CHAR_ID
        rows += struct.pack("!I", code_point)
    cp037_target = Path("rust/src/data/cp037.bin")
    cp037_target.write_bytes(bytes(rows))
    print(f"wrote {cp037_target} ({len(rows)} bytes)")
    write_pair_tables()
    return 0


def write_pair_tables() -> None:
    """Write per-encoding byte-sequence validity tables (``NPB1``).

    Rust validates candidates by decoding them, but its only multi-byte codecs
    are `encoding_rs`'s, which follow the WHATWG mappings: `shift_jis` there is
    CP932, so bytes the reference's narrower codec rejects decode fine.  The
    reference's answer is the Python codec's, so the accepted byte sequences are
    generated here for every multi-byte encoding the registry lists.

    Layout::

        magic       4 bytes   b"NPB1"
        num_tables  u32 BE
        per table:
            name_len u8
            name     name_len bytes, UTF-8
            single   32 bytes    bit set = the byte decodes on its own
            pairs    8192 bytes  bit set = (lead << 8 | trail) decodes as a pair
    """
    # Registry multi-byte encodings, plus the narrower codecs their display
    # names resolve to: the decode checks ask about those names too, and they
    # are not registry entries (`shift_jis` is a codec, `shift_jis_2004` is the
    # registry name).
    candidates = {
        name
        for name, info in REGISTRY.items()
        if info.is_multibyte_cjk and not name.startswith(("iso2022", "hz", "utf"))
    }
    candidates |= {"shift_jis", "euc_jp", "big5", "gb2312", "gbk", "cp950", "cp936"}
    names = []
    tables = []
    for encoding in sorted(candidates):
        if encoding.startswith(("iso2022", "hz", "utf")):
            # Stateful codecs are not a context-free byte grammar.
            continue
        single = bytearray(32)
        pairs = bytearray(8192)
        for byte in range(0x80, 0x100):
            try:
                bytes((byte,)).decode(encoding)
            except (UnicodeDecodeError, LookupError):
                continue
            single[byte // 8] |= 1 << (byte % 8)
        for lead in range(0x80, 0x100):
            for trail in range(0x100):
                try:
                    bytes((lead, trail)).decode(encoding)
                except (UnicodeDecodeError, LookupError):
                    continue
                index = (lead << 8) | trail
                pairs[index // 8] |= 1 << (index % 8)
        tables.append((encoding, bytes(single), bytes(pairs)))
        names.append(encoding)

    out = bytearray(b"NPB1")
    out += struct.pack("!I", len(tables))
    for name, single, pairs in tables:
        encoded = name.encode("utf-8")
        out += struct.pack("!B", len(encoded)) + encoded + single + pairs
    target = Path("rust/src/data/byte_pairs.bin")
    target.write_bytes(bytes(out))
    print(f"wrote {target} ({len(out)} bytes, {len(tables)} encodings: {', '.join(names)})")


if __name__ == "__main__":
    raise SystemExit(main())
