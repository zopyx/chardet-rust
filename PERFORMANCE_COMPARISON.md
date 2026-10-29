# Performance Comparison

Measured comparison — chardet-rust 0.2.1 vs chardet 7.7.0, charset-normalizer
3.5.2 and cchardet 3.2.0 on the 3,137-file test corpus (CPython 3.13.9, macOS
aarch64). Full tables, method and caveats: **[docs/performance.rst](docs/performance.rst)**.

| Detector | Encoding accuracy | Language | Files/s | Median | RSS after import |
|---|---|---|---|---|---|
| **chardet-rust 0.2.1** | **99.7%** | **88.5%** | **4,394** | **0.07ms** | 193.5 MiB |
| chardet 7.7.0 (mypyc) | 99.7% | 93.0% | 1,907 | 0.23ms | 157.0 MiB |
| cchardet 3.2.0 (mypyc) | 60.1% | — | 2,614 | 0.07ms | 181.8 MiB |
| charset-normalizer 3.5.2 (mypyc) | 81.7% | — | 1,545 | 0.45ms | 248.0 MiB |

Detection is 2.3x faster than chardet 7.7.0 (mypyc) and 2.8x faster than
charset-normalizer, at the same encoding accuracy as the reference. Language
detection is 4.5pp behind the reference; the entire gap is `utf-8-sig`, where
chardet-rust reports no language at all.

Baselines matter: mypyc (the mypy compiler, which turns annotated Python into C
extensions) is what `pip install chardet` gives you on CPython. Against the
pure-Python build of the same reference, used on PyPy and source installs, the
margin is 16x instead of 2.4x; against chardet 6.0.0 it is ~300x. Details and
protocol in [docs/performance.rst](docs/performance.rst#baselines).

Reproduce:

```bash
PYTHONPATH=scripts uv run python scripts/compare_detectors.py \
    -c 7.7.0 --cn --cchardet --no-cache
```

The previous content of this file was a set of estimates from an early
prototype (4KB synthetic inputs, "~50 calls/sec" placeholders, derived
"real-world impact" projections). It has been replaced by the measured run.
