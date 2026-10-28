# tests/conftest.py
"""Shared test configuration."""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

# Add scripts/ to sys.path so tests can import utils
sys.path.insert(0, str(Path(__file__).parent.parent / "scripts"))


@pytest.fixture(autouse=True, scope="session")
def _report_gil_status() -> None:
    """Print GIL status once per test session for CI visibility."""
    if hasattr(sys, "_is_gil_enabled"):
        enabled = sys._is_gil_enabled()
        status = "DISABLED (free-threaded)" if not enabled else "enabled"
        print(f"\nPython {sys.version.split()[0]} — GIL {status}")
    else:
        print(f"\nPython {sys.version.split()[0]} — GIL always enabled (< 3.13)")
