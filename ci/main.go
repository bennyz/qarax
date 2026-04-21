// Package main is a Dagger module for the Qarax CI pipeline.
// Call individual steps with: dagger call --mod ./ci <function> --src=.
// Run all checks in parallel:  dagger call --mod ./ci all --src=.
package main

import (
	"context"
	"fmt"

	"dagger/ci/internal/dagger"

	"golang.org/x/sync/errgroup"
)

const (
	otelFeatures = "qarax/otel,qarax-node/otel"
	dbUser       = "postgres"
	dbPassword   = "password"
	dbName       = "qarax"
	dbURL        = "postgres://postgres:password@postgres:5432/qarax"
)

// Ci is the Qarax CI pipeline Dagger module.
type Ci struct{}

func New() *Ci { return &Ci{} }

// postgres returns a Postgres 16 service ready to accept connections.
func (c *Ci) postgres() *dagger.Service {
	return dag.Container().
		From("postgres:16").
		WithEnvVariable("POSTGRES_USER", dbUser).
		WithEnvVariable("POSTGRES_PASSWORD", dbPassword).
		WithEnvVariable("POSTGRES_DB", dbName).
		WithExposedPort(5432).
		AsService()
}

// rustPreSource sets up the rust:1 image with system tools and cargo caches,
// but does not mount source. Layers up to this point are stable across source
// changes and are reused by both rustBase and Fmt.
func (c *Ci) rustPreSource() *dagger.Container {
	return dag.Container().
		From("rust:1").
		WithExec([]string{"sh", "-c",
			"apt-get update && apt-get install -y --no-install-recommends musl-tools protobuf-compiler libprotobuf-dev"}).
		WithExec([]string{"rustup", "target", "add", "x86_64-unknown-linux-musl"}).
		WithMountedCache("/usr/local/cargo/registry", dag.CacheVolume("cargo-registry")).
		WithMountedCache("/usr/local/cargo/git", dag.CacheVolume("cargo-git"))
}

// rustBase returns a ready-to-use container with source and build cache mounted.
func (c *Ci) rustBase(src *dagger.Directory) *dagger.Container {
	return c.rustPreSource().
		WithMountedCache("/src/target", dag.CacheVolume("cargo-target")).
		WithDirectory("/src", src).
		WithWorkdir("/src")
}

// Fmt checks formatting with cargo +nightly fmt --all -- --check.
// Nightly is installed on top of rustPreSource, before the source is mounted,
// so source changes do not evict the toolchain from Dagger's layer cache.
func (c *Ci) Fmt(ctx context.Context, src *dagger.Directory) (string, error) {
	return c.rustPreSource().
		WithExec([]string{"rustup", "install", "nightly", "--profile", "minimal"}).
		WithExec([]string{"rustup", "component", "add", "rustfmt", "--toolchain", "nightly"}).
		WithMountedCache("/src/target", dag.CacheVolume("cargo-target")).
		WithDirectory("/src", src).
		WithWorkdir("/src").
		WithExec([]string{"cargo", "+nightly", "fmt", "--all", "--", "--check"}).
		Stdout(ctx)
}

// Lint runs cargo clippy -D warnings using the committed .sqlx offline query cache.
// No database required.
func (c *Ci) Lint(ctx context.Context, src *dagger.Directory) (string, error) {
	return c.rustBase(src).
		WithExec([]string{"rustup", "component", "add", "clippy"}).
		WithEnvVariable("SQLX_OFFLINE", "true").
		WithExec([]string{"cargo", "clippy", "--workspace",
			"--features", otelFeatures, "--", "-D", "warnings"}).
		Stdout(ctx)
}

// Build compiles the workspace in release mode and returns a directory containing
// the four service binaries: qarax-server, qarax-node, qarax-init, qarax.
func (c *Ci) Build(ctx context.Context, src *dagger.Directory) (*dagger.Directory, error) {
	// /src/target is a cache volume so cannot be snapshotted directly.
	// Copy the four binaries to /out (a plain layer) before returning.
	return c.rustBase(src).
		WithEnvVariable("SQLX_OFFLINE", "true").
		WithExec([]string{"cargo", "build", "--workspace", "--release",
			"--features", otelFeatures}).
		WithExec([]string{"bash", "-c",
			"mkdir /out && cp /src/target/x86_64-unknown-linux-musl/release/{qarax-server,qarax-node,qarax-init,qarax} /out/"}).
		Directory("/out"), nil
}

// Test runs the nextest suite against a live Postgres 16 service.
func (c *Ci) Test(ctx context.Context, src *dagger.Directory) (string, error) {
	pg := c.postgres()

	return c.rustBase(src).
		// Cache the nextest binary so it is only downloaded on first use.
		WithMountedCache("/root/.cargo-tools", dag.CacheVolume("nextest-bin")).
		WithExec([]string{"sh", "-c",
			"[ -f /root/.cargo-tools/cargo-nextest ] || " +
				"curl -LsSf https://get.nexte.st/latest/linux | tar zxf - -C /root/.cargo-tools"}).
		WithEnvVariable("PATH", "/root/.cargo-tools:$PATH",
			dagger.ContainerWithEnvVariableOpts{Expand: true}).
		WithServiceBinding("postgres", pg).
		WithEnvVariable("SQLX_OFFLINE", "true").
		WithEnvVariable("DATABASE_URL", dbURL).
		WithEnvVariable("TEST_DATABASE_URL", dbURL).
		WithEnvVariable("DATABASE_HOST", "postgres").
		WithEnvVariable("DATABASE_PORT", "5432").
		WithEnvVariable("DATABASE_USERNAME", dbUser).
		WithEnvVariable("DATABASE_PASSWORD", dbPassword).
		WithEnvVariable("DATABASE_NAME", dbName).
		WithExec([]string{"cargo", "nextest", "run", "--workspace",
			"--features", otelFeatures}).
		Stdout(ctx)
}

// All runs Fmt, Lint, and Test in parallel.
// Use Build separately when you need release binaries.
func (c *Ci) All(ctx context.Context, src *dagger.Directory) (string, error) {
	g, ctx := errgroup.WithContext(ctx)

	g.Go(func() error {
		if _, err := c.Fmt(ctx, src); err != nil {
			return fmt.Errorf("fmt: %w", err)
		}
		return nil
	})
	g.Go(func() error {
		if _, err := c.Lint(ctx, src); err != nil {
			return fmt.Errorf("lint: %w", err)
		}
		return nil
	})
	g.Go(func() error {
		if _, err := c.Test(ctx, src); err != nil {
			return fmt.Errorf("test: %w", err)
		}
		return nil
	})

	if err := g.Wait(); err != nil {
		return "", err
	}
	return "All CI checks passed", nil
}
