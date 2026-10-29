# Performance Comparison

Measured comparison — chardet-rust 0.2.1 vs chardet 7.7.0, charset-normalizer
3.5.2 and cchardet 3.2.0 on the 3,137-file test corpus (CPython 3.13.9, macOS
aarch64). Full tables, method and caveats: **[docs/performance.rst](docs/performance.rst)**.

| Detector | Encoding accuracy | Language | Files/s | Median | RSS after import |
|---|---|---|---|---|---|
| **chardet-rust 0.2.1** | **99.7%** | **88.5%** | **3,581** | **0.07ms** | 194.0 MiB |
| chardet 7.7.0 (mypyc) | 99.7% | 93.0% | 2,048 | 0.19ms | 157.7 MiB |
| charset-normalizer 3.5.2 (mypyc) | 81.7% | — | 1,773 | 0.38ms | 248.8 MiB |
| cchardet 3.2.0 (mypyc) | 60.1% | — | 2,943 | 0.06ms | 181.9 MiB |

Detection is 1.75x faster than chardet 7.7.0 (mypyc) and 2.0x faster than
charset-normalizer, at the same encoding accuracy as the reference. Language
detection is 4.5pp behind the reference; the entire gap is `utf-8-sig`, where
chardet-rust reports no language at all.

Reproduce:

```bash
PYTHONPATH=scripts uv run python scripts/compare_detectors.py \
    -c 7.7.0 --cn --cchardet --no-cache
```

The previous content of this file was a set of estimates from an early
prototype (4KB synthetic inputs, "~50 calls/sec" placeholders, derived
"real-world impact" projections). It has been replaced by the measured run.
