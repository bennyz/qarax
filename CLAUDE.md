# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Qarax is a two-tier VM management platform for Cloud Hypervisor:
- **qarax** (control plane): REST API server (Axum) managing VM/host lifecycle, backed by PostgreSQL
- **qarax-node** (data plane): gRPC service running on hypervisor hosts, managing VMs via Cloud Hypervisor SDK

## Workspace Crates

- `qarax` — Control plane REST API (Axum + SQLx + utoipa for OpenAPI)
- `qarax-node` — Data plane gRPC service (tonic + Cloud Hypervisor SDK)
- `common` — Shared logging/telemetry helpers
- `cli` — Command-line client (clap)
- `qarax-init` — PID 1 init process for OCI-booted VMs

## Build & Development Commands

```bash
make build          # Build all + regenerate OpenAPI spec
make test           # Run tests (auto-starts Postgres via Docker)
make test-deps      # Start Postgres in Docker for tests
make lint           # cargo clippy --workspace -- -D warnings
make fmt            # cargo fmt + shfmt
make openapi        # Regenerate openapi.yaml
make sdk            # Regenerate Python SDK from OpenAPI
make run-local      # Start full Docker stack (qarax + qarax-node + Postgres)
make run-local VM=1 # Same, but run qarax-node in a libvirt VM
make stop-local     # Stop and remove the local Docker stack
```

Single crate: `cargo build -p qarax-node` or `cargo test -p qarax test_name`

**macOS note:** The default build target is `x86_64-unknown-linux-musl` (in `.cargo/config.toml`), but the Makefile auto-detects macOS and overrides to the host target. To build manually on macOS, pass `--target` explicitly or use `make build`.

**CI uses nightly for fmt:** `cargo +nightly fmt --all -- --check`

**Build dependencies:** `protobuf-compiler` (protoc) is required for gRPC code generation.

## Database

PostgreSQL 16+ with SQLx (compile-time verified queries).

- Migrations in `migrations/`, run automatically on startup
- Start DB: `make test-deps` or `./scripts/start_db.sh`
- Default connection: `qarax:qarax@127.0.0.1:5432/qarax`
- After modifying any SQL query: `cargo sqlx prepare --workspace`
- Offline/CI builds: `SQLX_OFFLINE=true cargo build`

## Architecture

### Control Plane (qarax)

- `qarax/src/handlers/` — Axum HTTP handlers by resource (hosts, vms, storage_pools, storage_objects, boot_sources, transfers, jobs)
- `qarax/src/model/` — Database models with inline SQLx queries
- `qarax/src/grpc_client.rs` — Tonic client for communicating with qarax-node instances
- `qarax/src/vm_monitor.rs` — Background task that periodically reconciles VM status with nodes
- `qarax/src/resource_monitor.rs` — Background task polling host resource metrics
- `qarax/src/transfer_executor.rs` — Async file transfer handling
- `qarax/src/host_deployer.rs` — Host deployment via SSH + bootc
- `qarax/src/configuration.rs` — YAML config with env var overrides

### Data Plane (qarax-node)

- `qarax-node/src/services/vm/` — VM lifecycle gRPC service (create, start, stop, pause, resume, delete)
- `qarax-node/src/services/file_transfer/` — File download/copy gRPC service
- `qarax-node/src/cloud_hypervisor/manager.rs` — Cloud Hypervisor process management
- `qarax-node/src/image_store/manager.rs` — OCI image handling via virtiofsd
- `qarax-node/src/overlaybd/manager.rs` — OverlayBD TCMU block device management

### Communication Flow

Control plane → gRPC (proto/node.proto) → Data plane → Cloud Hypervisor API. VMs are scheduled onto hosts in "up" state; all subsequent VM operations route to the scheduled host.

### OpenAPI

Auto-generated via utoipa derive macros → `openapi.yaml`. Swagger UI available at `/swagger-ui`. Python SDK in `python-sdk/` is regenerated from the spec via `make sdk`.

## Configuration

YAML files in `configuration/` (base.yaml, local.yaml, production.yaml), selected by `APP_ENVIRONMENT` env var (default: local). Key env var overrides: `DATABASE_HOST`, `DATABASE_PORT`, `DATABASE_USERNAME`, `DATABASE_PASSWORD`, `DATABASE_NAME`.

## CI

GitHub Actions (`rust-ci.yml`): fmt check (nightly) → clippy → build (musl) → unit tests → E2E tests. E2E tests run on push to master or PRs with `run-e2e` label. E2E uses pytest against a Docker Compose stack with KVM passthrough (`e2e/`).
