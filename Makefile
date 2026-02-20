.PHONY: build test test-deps clean openapi help lint fmt ruff-check ruff-fmt

# On macOS, override default musl target (linker fails cross-compiling from Mac)
UNAME_S := $(shell uname -s)
ifeq ($(UNAME_S),Darwin)
  CARGO_TARGET := --target $(shell rustc -vV 2>/dev/null | grep '^host:' | cut -d' ' -f2)
else
  CARGO_TARGET :=
endif

help:
	@echo "Available targets:"
	@echo "  make build      - Build all packages and generate OpenAPI spec"
	@echo "  make openapi    - Generate OpenAPI spec only"
	@echo "  make test       - Run all tests (requires Postgres, see test-deps)"
	@echo "  make test-deps  - Start Postgres in Docker for tests"
	@echo "  make clean      - Clean build artifacts"
	@echo "  make lint       - Run cargo clippy (lint)"
	@echo "  make fmt        - Run cargo fmt (format)"
	@echo "  make ruff-check - Run ruff check on Python SDK"
	@echo "  make ruff-fmt   - Run ruff format on Python SDK"

build:
	cargo build $(CARGO_TARGET)
	cargo run $(CARGO_TARGET) -p qarax --bin generate-openapi

openapi:
	cargo run $(CARGO_TARGET) -p qarax --bin generate-openapi

# Database env vars point config to localhost (overrides local.yaml's host: postgres)
# Credentials match both the standalone start_db.sh postgres and the E2E compose postgres.
test: test-deps
	DATABASE_HOST=localhost DATABASE_PORT=5432 \
	DATABASE_USERNAME=qarax DATABASE_PASSWORD=qarax DATABASE_NAME=qarax \
	cargo test $(CARGO_TARGET)

# Start Postgres in Docker for integration tests. Run before 'make test' if needed.
# Skip with SKIP_DOCKER=1 if Postgres is already running (e.g. via Docker Compose).
test-deps:
	@if [ -n "$$SKIP_DOCKER" ]; then echo "Skipping (SKIP_DOCKER=1)"; exit 0; fi; \
	if nc -z localhost 5432 2>/dev/null; then \
		echo "Postgres appears to be running on 5432"; \
	elif command -v docker >/dev/null 2>&1; then \
		echo "Starting Postgres..."; \
		./scripts/start_db.sh || { echo "Docker failed. If Postgres is running elsewhere, use: SKIP_DOCKER=1 make test"; exit 1; }; \
	else \
		echo "Postgres required. Start Docker and run 'make test', or run: SKIP_DOCKER=1 make test (if Postgres is already running)"; exit 1; \
	fi

clean:
	cargo clean

# Linting and formatting
lint:
	cargo clippy -- -D warnings

fmt:
	cargo fmt

ruff-check:
	@cd python-sdk && ruff check .

ruff-fmt:
	@cd python-sdk && ruff format .
