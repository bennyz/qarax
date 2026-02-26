# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Qarax is a two-tier VM management platform for Cloud Hypervisor:
- **qarax** (control plane): REST API server (Axum) managing VM/host lifecycle
- **qarax-node** (data plane): gRPC service running on hypervisor hosts, managing VMs via Cloud Hypervisor

## Workspace Crates

- `qarax` — Control plane REST API (Axum + SQLx + utoipa)
- `qarax-node` — Data plane gRPC service (tonic + Cloud Hypervisor SDK)
- `common` — Shared logging/telemetry helpers
- `cli` — Command-line client (clap)
- `qarax-init` — PID 1 init process for OCI-booted VMs

## Build & Development Commands

```bash
make build                # Build all + regenerate OpenAPI spec
make test                 # Run all unit tests (needs Postgres)
make test-deps            # Start Postgres in Docker for tests
make lint                 # cargo clippy --workspace -- -D warnings
make fmt                  # cargo fmt + shfmt
make openapi              # Regenerate openapi.yaml
make sdk                  # Regenerate Python SDK from OpenAPI
make run-local            # Start full Docker stack (qarax + qarax-node + Postgres)
make stop-local           # Cleanup Docker stack
```

Single crate build: `cargo build -p qarax-node`

Single test: `cargo test -p qarax test_name`

Format requires nightly: `cargo +nightly fmt --all`

Default build target is `x86_64-unknown-linux-musl` (set in `.cargo/config.toml`).

## Database

PostgreSQL with SQLx (compile-time verified queries).

After modifying any SQL query: `cargo sqlx prepare --workspace`

Offline builds (CI): `SQLX_OFFLINE=true cargo build`

Migrations live in `migrations/`. Database started via `scripts/start_db.sh` or `make test-deps`.

Default connection: `qarax:qarax@127.0.0.1:5432/qarax`

## Architecture

**Control Plane (qarax):**
- `qarax/src/handlers/` — Axum HTTP handlers organized by resource (hosts, vms, storage_objects, storage_pools, boot_sources, transfers, jobs)
- `qarax/src/model/` — Database models with SQLx queries
- `qarax/src/grpc_client.rs` — tonic client for communicating with qarax-node
- `qarax/src/vm_monitor.rs` — Background task reconciling VM status
- `qarax/src/transfer_executor.rs` — Async file transfer handling
- `qarax/src/host_deployer.rs` — Host deployment via SSH (russh)
- `qarax/src/configuration.rs` — YAML config with env var overrides (`configuration/base.yaml`)

**Data Plane (qarax-node):**
- `qarax-node/src/services/` — gRPC service implementations (vm, file_transfer)
- `qarax-node/src/cloud_hypervisor/manager.rs` — Cloud Hypervisor SDK integration
- `qarax-node/src/image_store/manager.rs` — OCI image handling via virtiofsd
- `qarax-node/src/overlaybd/manager.rs` — OverlayBD TCMU device management

**Protocol:** gRPC defined in `proto/node.proto`, compiled via tonic-build in build.rs.

**OpenAPI:** Auto-generated via utoipa derive macros → `openapi.yaml`. Swagger UI at `/swagger-ui`.

## Configuration

YAML files in `configuration/` (base.yaml, local.yaml, production.yaml). Selected by `APP_ENVIRONMENT` env var. Environment variables override YAML values (e.g., `DATABASE_HOST`, `DATABASE_PORT`, `VM_KERNEL`).

## CI

GitHub Actions runs: fmt check (nightly), clippy, build (musl), unit tests, and E2E tests (pytest against Docker Compose stack). E2E tests require the `run-e2e` label on PRs.

## Python SDK

Generated from OpenAPI in `python-sdk/`. Lint with `make ruff-check`, format with `make ruff-fmt`.
