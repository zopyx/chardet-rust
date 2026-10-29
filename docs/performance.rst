Performance
===========

Benchmarked against 3,137 files from the
`chardet test suite <https://github.com/chardet/test-data>`_, scored with the
same directional equivalence rules for every detector. All detectors run in
isolated ``uv`` virtual environments, so no installed package can shadow
another. Numbers below are chardet-rust 0.2.1 on CPython 3.13.9, macOS
aarch64 (Apple Silicon).

Reproduce with::

   PYTHONPATH=scripts uv run python scripts/compare_detectors.py \
       -c 7.7.0 --cn --cchardet --no-cache

Accuracy
--------

.. list-table::
   :header-rows: 1
   :widths: 32 12 12 16 14

   * - Detector
     - Correct
     - Accuracy
     - Language
     - Speed
   * - **chardet-rust 0.2.1**
     - **3127/3137**
     - **99.7%**
     - **2769/3129 (88.5%)**
     - **3,581 files/s**
   * - chardet 7.7.0 (mypyc)
     - 3127/3137
     - 99.7%
     - 2911/3129 (93.0%)
     - 2,048 files/s
   * - charset-normalizer 3.5.2 (mypyc)
     - 2563/3137
     - 81.7%
     - not reported
     - 1,773 files/s
   * - cchardet 3.2.0 (mypyc)
     - 1885/3137
     - 60.1%
     - not reported
     - 2,943 files/s

chardet-rust detects the encoding of every file the reference chardet 7.7.0
detects: the two agree on all 3,127 correct files and on the same 10 misses.
Against the alternative detectors the margin is **+18.0pp** over
charset-normalizer and **+39.6pp** over cchardet.

Language detection trails the reference by 4.5pp (88.5% vs 93.0%). The whole
gap is a single encoding: chardet-rust reports no language for ``utf-8-sig``
files (0/151, reference 142/151). Every other encoding matches.

Speed
-----

.. list-table::
   :header-rows: 1
   :widths: 30 12 12 12 12 12 12

   * - Detector
     - Total
     - Files/s
     - Mean
     - Median
     - p90
     - p95
   * - **chardet-rust 0.2.1**
     - **876ms**
     - **3,581**
     - **0.28ms**
     - **0.07ms**
     - **0.57ms**
     - **0.96ms**
   * - cchardet 3.2.0 (mypyc)
     - 1,066ms
     - 2,943
     - 0.34ms
     - 0.06ms
     - 0.90ms
     - 1.42ms
   * - chardet 7.7.0 (mypyc)
     - 1,532ms
     - 2,048
     - 0.49ms
     - 0.19ms
     - 0.96ms
     - 1.48ms
   * - charset-normalizer 3.5.2 (mypyc)
     - 1,769ms
     - 1,773
     - 0.56ms
     - 0.38ms
     - 1.21ms
     - 1.85ms

Detection is **1.75x faster** than chardet 7.7.0 (mypyc), **2.0x faster** than
charset-normalizer (mypyc) and **1.2x faster** than cchardet. cchardet is
marginally faster on the median file (0.06ms vs 0.07ms) but about twice as
slow on the tail, which is where the accuracy-critical large files sit.

Memory
------

.. list-table::
   :header-rows: 1
   :widths: 32 14 14 14 14

   * - Detector
     - Import Time
     - Import Memory
     - Peak Memory
     - RSS after import
   * - **chardet-rust 0.2.1**
     - 0.083s
     - 5.2 MiB
     - 5.8 MiB
     - 194.0 MiB
   * - chardet 7.7.0 (mypyc)
     - 0.024s
     - 2.6 MiB
     - 29.7 MiB
     - 157.7 MiB
   * - cchardet 3.2.0 (mypyc)
     - 0.004s
     - 732 KiB
     - 81.7 MiB
     - 181.9 MiB
   * - charset-normalizer 3.5.2 (mypyc)
     - 0.019s
     - 2.3 MiB
     - 88.3 MiB
     - 248.8 MiB

The Python-side footprint is the smallest of the four (5.2 MiB imported, 5.8
MiB peak against 29.7 MiB for the reference), but ``tracemalloc`` only counts
CPython allocations: the Rust bigram tables and their buffers are invisible
there. On resident memory chardet-rust ends up **36 MiB above** chardet 7.7.0
because it decodes the model artifacts eagerly at import time — that is also
why its import takes 0.083s instead of 0.024s. Against charset-normalizer it
still uses **55 MiB less** RSS.

Language Detection
------------------

.. list-table::
   :header-rows: 1
   :widths: 32 16 14

   * - Detector
     - Correct
     - Accuracy
   * - chardet 7.7.0 (mypyc)
     - 2911/3129
     - 93.0%
   * - **chardet-rust 0.2.1**
     - **2769/3129**
     - **88.5%**
   * - charset-normalizer 3.5.2
     - not reported
     - —
   * - cchardet 3.2.0
     - not reported
     - —

chardet-rust returns a language for every detected file. charset-normalizer
and cchardet do not report one.

Thread Safety
-------------

:func:`chardet.detect` and :func:`chardet.detect_all` are thread-safe: each
call owns its state, and ``tests/test_thread_safety.py`` exercises concurrent
detection. No free-threaded (3.13t) build is published — PyO3 0.23 does not
support it — so no GIL-disabled scaling numbers are reported here.

Individual :class:`~chardet.UniversalDetector` instances are not thread-safe.
Create one instance per thread when using the streaming API.

Build
-----

chardet-rust ships platform wheels containing the compiled Rust extension
(built with ``setuptools-rust``) for CPython 3.11 and newer; there is no mypyc
build and no pure-Python fallback with the same performance characteristics.
Installing from the source distribution compiles the extension with cargo, so
that path needs a Rust toolchain.
