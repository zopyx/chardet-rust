# Makefile for chardet package builds

.PHONY: all clean build sdist wheel check upload test

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

# Check the built distributions with twine
check:
	uvx twine check dist/*

# Upload to PyPI (requires authentication)
upload: check
	uvx twine upload dist/*

# Upload to TestPyPI (requires authentication)
upload-test: check
	uvx twine upload --repository testpypi dist/*

# Run tests
test:
	uv run pytest

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
	@echo "  all         - Clean and build sdist + wheel (default)"
	@echo "  clean       - Remove build artifacts"
	@echo "  build       - Build both sdist and wheel"
	@echo "  sdist       - Build source distribution only"
	@echo "  wheel       - Build wheel only"
	@echo "  check       - Check distributions with twine"
	@echo "  upload      - Upload to PyPI"
	@echo "  upload-test - Upload to TestPyPI"
	@echo "  test        - Run test suite"
	@echo "  dev         - Install in development mode"
	@echo "  sync        - Update dependencies"
	@echo "  format      - Format code with ruff"
	@echo "  lint        - Lint code with ruff"
	@echo "  help        - Show this help message"
