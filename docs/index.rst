chardet documentation
=====================

**chardet** is a universal character encoding detector for Python. It analyzes
byte strings and returns the detected encoding, confidence score, and language.

.. code-block:: python

   import chardet

   result = chardet.detect("It\u2019s a lovely day \u2014 let\u2019s grab coffee.".encode("utf-8"))
   print(result)
   # {'encoding': 'utf-8', 'confidence': 0.99, 'language': 'es'}

chardet 7.0 is a ground-up, LGPL-licensed rewrite — same package name, same
public API, drop-in replacement for chardet 5.x/6.x. The detector core is
implemented in Rust and exposed to Python via PyO3. Python 3.10+.

.. warning::

   This Rust reimplementation is an AI experiment and is not an official
   upstream replacement.

- **98.1% accuracy** on 2,510 test files
- **High-performance Rust core** with Python bindings
- **Language detection** for every result (95.1% accuracy)
- **99 encodings** across six encoding eras
- **Thread-safe** ``detect()`` and ``detect_all()``

.. toctree::
   :maxdepth: 2
   :caption: Contents
   :hidden:

   usage
   supported-encodings
   how-it-works
   performance
   faq
   api/index
   contributing
   changelog
