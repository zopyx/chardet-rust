#!/usr/bin/env python3
"""Regenerate the Rust codec-alias table from Python's own alias table.

`lookup_encoding` resolves labels the way `codecs.lookup` does, but the alias
targets are Python codec names (`gb2312`), which are often not registry names
in their own right — so the target has to be resolved to the registry's
canonical name before it is written out.

Usage:
    PYTHONPATH=rust:src:scripts uv run python scripts/gen_codec_aliases.py
"""

from __future__ import annotations

import encodings.aliases
import re
from pathlib import Path

from chardet.registry import lookup_encoding

TARGET = Path("rust/src/registry.rs")


def main() -> int:
    pairs: dict[str, str] = {}
    for alias, target in encodings.aliases.aliases.items():
        canonical = lookup_encoding(target) or lookup_encoding(alias)
        if canonical is not None:
            pairs[alias.lower()] = canonical
    lines = ["static CODEC_ALIASES: &[(&str, &str)] = &["]
    lines += [f'    ("{alias}", "{canonical}"),' for alias, canonical in sorted(pairs.items())]
    lines.append("];")
    block = "\n".join(lines)

    text = TARGET.read_text()
    pattern = re.compile(r"static CODEC_ALIASES: &\[\(&str, &str\)\] = &\[.*?\n\];", re.DOTALL)
    if not pattern.search(text):
        msg = "CODEC_ALIASES array not found"
        raise SystemExit(msg)
    TARGET.write_text(pattern.sub(lambda _: block, text, count=1))
    print(f"wrote {len(pairs)} codec aliases into {TARGET}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
