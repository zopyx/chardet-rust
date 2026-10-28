# Makefile for chardet package builds

.PHONY: all clean build-ext build sdist wheel wheels wheels-linux wheels-all check upload upload-all upload-test test parity

# Default target
all: clean build

# Clean build artifacts
clean:
	rm -rf dist/ build/ src/*.egg-info .eggs/
	find . -type d -name __pycache__ -exec rm -rf {} + 2>/dev/null || true
	find . -type f -name "*.pyc" -delete 2>/dev/null || true

# Build both sdist and wheel
build: clean
	uv build

# Build only source distribution
sdist: clean
	uv build --sdist

# Build only wheel
wheel: clean
	uv build --wheel

# Build wheels for multiple platforms using cibuildwheel
# Note: Linux builds require Docker (colima/docker-desktop)
wheels:
	@echo "Building wheels for macOS..."
	uv run cibuildwheel --platform macos

wheels-linux:
	@echo "Building Linux wheels (requires Docker)..."
	uv run cibuildwheel --platform linux

wheels-windows:
	@echo "Windows wheels can only be built on Windows or using cross-compilation"
	@echo "Use GitHub Actions for Windows builds"

# Build all wheels (macOS local + Linux via Docker)
wheels-all: wheels wheels-linux
	@echo "All platform wheels built in wheelhouse/"

# Check the built distributions with twine
check:
	uvx twine check dist/*

# Upload to PyPI (requires authentication)
upload: check
	@set -e; \
	files=$$(ls dist/* | grep -Ev -- '-linux_[^/]*\.whl$$' || true); \
	if [ -z "$$files" ]; then \
		echo "No uploadable distributions found in dist/"; \
		exit 1; \
	fi; \
	echo "Uploading filtered distributions:"; \
	printf '%s\n' $$files; \
	uvx twine upload $$files

# Upload all distributions as-is (may fail on unsupported local platform wheels)
upload-all: check
	uvx twine upload dist/*

# Upload to TestPyPI (requires authentication)
upload-test: check
	uvx twine upload --repository testpypi dist/*

# Compile the Rust extension in place (always reflects the current sources).
#
# `uv pip install -e rust` caches the built wheel by package metadata, not by
# Rust source content, so after editing rust/src/* it can silently keep an old
# _chardet_rs.so.  Building with cargo and copying the cdylib makes the
# extension the tests import match the working tree.
CARGO ?= $(HOME)/.cargo/bin/cargo
PYO3_PYTHON ?= $(CURDIR)/.venv/bin/python
EXT_SO = rust/chardet_rs/_chardet_rs.abi3.so

build-ext:
	PATH="$(HOME)/.cargo/bin:$$PATH" \
	PYO3_PYTHON="$(PYO3_PYTHON)" \
	PYO3_BUILD_EXTENSION_MODULE=1 \
	$(CARGO) rustc --manifest-path rust/Cargo.toml --release \
		--features extension-module --lib --crate-type cdylib -- \
		-C link-arg=-undefined -C link-arg=dynamic_lookup \
		-C link-arg=-Wl,-install_name,@rpath/chardet_rs._chardet_rs.abi3.so
	cp rust/target/release/lib_chardet_rs.dylib $(EXT_SO)

# Run tests
test: build-ext
	PYTHONPATH=rust:src:scripts uv run pytest

# Rust-vs-pytest parity report
parity:
	PYTHONPATH=scripts uv run python scripts/parity_report.py

# Install package in development mode
dev:
	uv pip install -e ".[dev]"

# Update dependencies
sync:
	uv sync

# Format code
format:
	uv run ruff format .

# Lint code
lint:
	uv run ruff check .

# Show help
help:
	@echo "Available targets:"
	@echo "  all          - Clean and build sdist + wheel (default)"
	@echo "  clean        - Remove build artifacts"
	@echo "  build        - Build both sdist and wheel (current platform only)"
	@echo "  sdist        - Build source distribution only"
	@echo "  wheel        - Build wheel only (current platform only)"
	@echo "  wheels       - Build wheels for macOS (arm64 only)"
	@echo "  wheels-linux - Build Linux wheels via Docker (requires colima/docker)"
	@echo "  wheels-all   - Build wheels for macOS + Linux"

	@echo "  check        - Check distributions with twine"
	@echo "  upload       - Upload to PyPI (skips unsupported local linux_* wheels)"
	@echo "  upload-all   - Upload all files in dist/ as-is"
	@echo "  upload-test  - Upload to TestPyPI"
	@echo "  test         - Run test suite"
	@echo "  parity       - Run Rust-vs-pytest accuracy parity report"
	@echo "  dev          - Install in development mode"
	@echo "  sync         - Update dependencies"
	@echo "  format       - Format code with ruff"
	@echo "  lint         - Lint code with ruff"
	@echo "  help         - Show this help message"
