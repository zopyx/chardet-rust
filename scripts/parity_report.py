#!/usr/bin/env python3
"""Generate a Rust-vs-pytest accuracy parity report."""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from utils import collect_test_files  # noqa: E402


def load_known_failures(path: Path) -> set[str]:
    entries: set[str] = set()
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if line and not line.startswith("#"):
            entries.add(line)
    return entries


def parse_metric(output: str, label: str) -> int:
    pattern = rf"{re.escape(label)}:\s+(\d+)"
    match = re.search(pattern, output)
    if not match:
        raise RuntimeError(f"Missing metric in Rust output: {label}")
    return int(match.group(1))


def run_rust_accuracy(repo_root: Path) -> dict[str, int]:
    cmd = [
        "cargo",
        "test",
        "--test",
        "test_accury",
        "test_accuracy_all_files",
        "--",
        "--nocapture",
    ]
    proc = subprocess.run(
        cmd,
        cwd=repo_root / "rust",
        capture_output=True,
        text=True,
        check=True,
    )
    out = proc.stdout + "\n" + proc.stderr
    return {
        "discovered": parse_metric(out, "Total test data cases discovered"),
        "processed": parse_metric(out, "Processed test data cases"),
        "skipped": parse_metric(out, "Skipped known failures"),
        "passed": parse_metric(out, "Passed"),
        "failed": parse_metric(out, "Failed"),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--strict",
        action="store_true",
        help="exit with non-zero status when parity target is not reached",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parent.parent
    known_path = repo_root / "tests" / "known_accuracy_failures.txt"
    data_dir = repo_root / "tests" / "data"

    known_failures = load_known_failures(known_path)
    total_cases = len(collect_test_files(data_dir))
    expected_processed = total_cases - len(known_failures)

    rust = run_rust_accuracy(repo_root)
    parity_gap = rust["failed"]
    processing_ok = (
        rust["discovered"] == total_cases
        and rust["processed"] == expected_processed
        and rust["skipped"] == len(known_failures)
    )

    print("Accuracy Parity Report")
    print(f"  pytest total cases: {total_cases}")
    print(f"  pytest known failures (xfail): {len(known_failures)}")
    print(f"  pytest expected processed cases: {expected_processed}")
    print(f"  rust discovered cases: {rust['discovered']}")
    print(f"  rust processed cases: {rust['processed']}")
    print(f"  rust skipped known failures: {rust['skipped']}")
    print(f"  rust pass/fail: {rust['passed']}/{rust['failed']}")
    print(f"  parity gap (unexpected Rust failures): {parity_gap}")

    if not processing_ok:
        print("  status: FAIL (test corpus or known-failure baseline mismatch)")
        raise SystemExit(2)

    if parity_gap == 0:
        print("  status: PASS (Rust is at parity target)")
        return

    print("  status: FAIL (Rust not yet at parity target)")
    if args.strict:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
