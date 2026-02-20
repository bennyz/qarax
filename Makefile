.PHONY: build test test-deps clean openapi help

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

build:
	cargo build $(CARGO_TARGET)
	cargo run $(CARGO_TARGET) -p qarax --bin generate-openapi

openapi:
	cargo run $(CARGO_TARGET) -p qarax --bin generate-openapi

# Database env vars point config to localhost (overrides local.yaml's host: postgres)
test: test-deps
	DATABASE_HOST=localhost DATABASE_PORT=5432 \
	DATABASE_USERNAME=postgres DATABASE_PASSWORD=password DATABASE_NAME=qarax \
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
