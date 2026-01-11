.PHONY: build test clean openapi help

help:
	@echo "Available targets:"
	@echo "  make build      - Build all packages and generate OpenAPI spec"
	@echo "  make openapi    - Generate OpenAPI spec only"
	@echo "  make test       - Run all tests"
	@echo "  make clean      - Clean build artifacts"

build:
	cargo build
	cargo run -p qarax --bin generate-openapi

openapi:
	cargo run -p qarax --bin generate-openapi

test:
	cargo test

clean:
	cargo clean
