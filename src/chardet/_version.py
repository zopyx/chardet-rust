"""Version of this distribution, for the vendored detection engine.

The vendored reference generates this module with hatch-vcs at build time; this
project builds with setuptools, so the module is tracked instead.  The value is
read from the installed distribution metadata, which keeps it in step with
``pyproject.toml`` without a second place to bump.
"""

from __future__ import annotations

from importlib.metadata import PackageNotFoundError, version as _dist_version

__all__ = [
    "__version__",
    "__version_tuple__",
    "__commit_id__",
    "commit_id",
    "version",
    "version_tuple",
]

try:
    version = _dist_version("chardet-rust")
except PackageNotFoundError:  # pragma: no cover - only when run uninstalled
    version = "0.0.0"

__version__ = version


def _to_version_tuple(value: str) -> tuple[int | str, ...]:
    """Split a PEP 440 version into its numeric and pre-release parts."""
    parts: list[int | str] = []
    for chunk in value.replace("-", ".").split("."):
        parts.append(int(chunk) if chunk.isdigit() else chunk)
    return tuple(parts)


__version_tuple__ = version_tuple = _to_version_tuple(version)

# Kept for the vendored code that reads them; this project has no VCS stamp.
__commit_id__ = commit_id = None
