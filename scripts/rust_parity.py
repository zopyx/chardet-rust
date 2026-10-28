#!/usr/bin/env python3
"""Compare the Rust engine against the vendored Python reference.

The Python tree under ``src/chardet`` is the reference implementation
(chardet 7.7.0+); ``chardet_rs._chardet_rs`` is the Rust port. This script
runs both over the accuracy corpus and reports agreement, so the port can be
driven by numbers instead of impressions.

Usage:
    PYTHONPATH=rust:src:scripts uv run python scripts/rust_parity.py [--limit N]
"""

from __future__ import annotations

import argparse
import collections
import sys
from pathlib import Path

from chardet.enums import EncodingEra
from chardet.pipeline.orchestrator import run_pipeline
from chardet_rs import _chardet_rs as rust


def iter_files(data_dir: Path) -> list[Path]:
    """Yield every test file in the corpus, sorted for reproducibility."""
    return sorted(p for p in data_dir.rglob("*") if p.is_file() and not p.name.startswith("."))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--limit", type=int, default=0, help="only the first N files")
    parser.add_argument("--max-bytes", type=int, default=200_000)
    parser.add_argument("--show", type=int, default=15, help="how many mismatches to print")
    args = parser.parse_args()

    data_dir = Path("tests/data")
    files = iter_files(data_dir)
    if args.limit:
        files = files[: args.limit]

    total = 0
    enc_match = 0
    full_match = 0
    mismatches: list[tuple[Path, str, str, float, float]] = []
    by_encoding: collections.Counter[str] = collections.Counter()

    era = EncodingEra.ALL
    for path in files:
        try:
            data = path.read_bytes()
        except OSError:
            continue
        if not data:
            continue

        ref = run_pipeline(data, era, max_bytes=args.max_bytes)
        rust_out = rust.detect(
            data,
            should_rename_legacy=False,
            encoding_era=rust.EncodingEra.ALL,
            max_bytes=args.max_bytes,
        )
        if not ref:
            continue

        total += 1
        ref_enc = ref[0].encoding
        rust_enc = rust_out.get("encoding")
        same_enc = ref_enc == rust_enc
        if same_enc:
            enc_match += 1
        else:
            by_encoding[f"{ref_enc} -> {rust_enc}"] += 1
            if len(mismatches) < args.show:
                mismatches.append((path, ref_enc, rust_enc, ref[0].confidence, rust_out.get("confidence", 0.0)))

        same_conf = abs(ref[0].confidence - rust_out.get("confidence", 0.0)) < 1e-9
        same_lang = ref[0].language == rust_out.get("language")
        if same_enc and same_conf and same_lang:
            full_match += 1

    print(f"files compared : {total}")
    print(f"encoding match : {enc_match} ({100.0 * enc_match / max(total, 1):.2f}%)")
    print(f"full match     : {full_match} ({100.0 * full_match / max(total, 1):.2f}%)")
    if mismatches:
        print("\nfirst mismatches (file: python -> rust, conf):")
        for path, ref_enc, rust_enc, ref_conf, rust_conf in mismatches:
            print(f"  {path.relative_to(data_dir)}: {ref_enc} -> {rust_enc} ({ref_conf:.3f} vs {rust_conf:.3f})")
    if by_encoding:
        print("\ntop divergence pairs:")
        for pair, count in by_encoding.most_common(15):
            print(f"  {count:4d}  {pair}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
