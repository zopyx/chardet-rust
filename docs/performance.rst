Performance
===========

Benchmarked against 3,137 files from the
`chardet test suite <https://github.com/chardet/test-data>`_, scored with the
same directional equivalence rules for every detector. All detectors run in
isolated ``uv`` virtual environments, so no installed package can shadow
another, and each detector is timed three times in sequence — never in parallel
with another detector, which would inflate every measurement. Numbers below are
chardet-rust 0.2.1 on CPython 3.13.9, macOS aarch64 (Apple Silicon).

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
     - **4,394 files/s**
   * - chardet 7.7.0 (mypyc)
     - 3127/3137
     - 99.7%
     - 2911/3129 (93.0%)
     - 1,907 files/s
   * - charset-normalizer 3.5.2 (mypyc)
     - 2563/3137
     - 81.7%
     - not reported
     - 1,545 files/s
   * - cchardet 3.2.0 (mypyc)
     - 1885/3137
     - 60.1%
     - not reported
     - 2,614 files/s

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
     - **714ms**
     - **4,394**
     - **0.23ms**
     - **0.07ms**
     - **0.42ms**
     - **0.66ms**
   * - cchardet 3.2.0 (mypyc)
     - 1,200ms
     - 2,614
     - 0.38ms
     - 0.07ms
     - 1.07ms
     - 1.58ms
   * - chardet 7.7.0 (mypyc)
     - 1,645ms
     - 1,907
     - 0.52ms
     - 0.23ms
     - 0.95ms
     - 1.37ms
   * - charset-normalizer 3.5.2 (mypyc)
     - 2,030ms
     - 1,545
     - 0.65ms
     - 0.45ms
     - 1.40ms
     - 2.17ms

Detection is **2.3x faster** than the reference chardet 7.7.0 (mypyc), **2.8x
faster** than charset-normalizer (mypyc) and **1.7x faster** than cchardet.
cchardet matches chardet-rust on the median file (0.07ms) but is 2.5x slower at
p90, and it is 39.6pp behind on accuracy.

Baselines
---------

A speedup means nothing without the build it was measured against, and chardet
ships two: mypyc-compiled wheels for CPython (what ``pip install chardet``
picks up) and a pure-Python build for PyPy and source installs. ``mypyc`` is
the mypy compiler — it turns annotated Python into C extension modules at build
time, so the reference's hot loops run as machine code instead of through the
interpreter.

Same corpus, same machine, one run per detector in its own virtual environment
(so the absolute times are not comparable with the three-run table above;
chardet-rust 0.2.1 measures 455ms under this protocol):

.. list-table::
   :header-rows: 1
   :widths: 46 16 16

   * - Baseline
     - Detection
     - chardet-rust is
   * - chardet 7.7.0 mypyc (default on CPython)
     - 1,099ms
     - **2.4x faster**
   * - chardet 7.7.0 pure Python (PyPy, source installs)
     - 7,258ms
     - **16x faster**
   * - chardet 6.0.0 (pre-rewrite release, CPython 3.11)
     - 135,835ms
     - **300x faster**

mypyc accounts for 6.6x of the reference's own runtime: the same version takes
7,258ms as pure Python and 1,099ms compiled. That is why the headline figure on
this page is the 2.3x against the mypyc build — comparing against pure Python
would credit chardet-rust with a handicap the reference does not have on
CPython.

Earlier performance notes in this repository quoted 21.9x, 113x and "43x faster
than chardet 6.0.0". Those came from pure-Python chardet 6.x on 4KB synthetic
buffers — including the "43x", which is the reference project's own
mypyc-vs-6.0.0 claim carried over verbatim, not a chardet-rust measurement.
The tables on this page replace them.

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
     - 0.066s
     - 5.2 MiB
     - 5.8 MiB
     - 193.5 MiB
   * - chardet 7.7.0 (mypyc)
     - 0.021s
     - 2.6 MiB
     - 29.7 MiB
     - 157.0 MiB
   * - cchardet 3.2.0 (mypyc)
     - 0.005s
     - 732 KiB
     - 81.7 MiB
     - 181.8 MiB
   * - charset-normalizer 3.5.2 (mypyc)
     - 0.014s
     - 2.3 MiB
     - 88.3 MiB
     - 248.0 MiB

The Python-side footprint is the smallest of the four (5.2 MiB imported, 5.8
MiB peak against 29.7 MiB for the reference), but ``tracemalloc`` only counts
CPython allocations: the Rust bigram tables and their buffers are invisible
there. On resident memory chardet-rust ends up **36 MiB above** chardet 7.7.0
because it decodes the model artifacts eagerly at import time — that is also
why its import takes 0.066s instead of 0.021s. Against charset-normalizer it
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
