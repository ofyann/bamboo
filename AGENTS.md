# AGENTS.md

## Project Overview

`bamboo` is a Rust CLI tool that replaces a skopeo-based bash script for syncing a single container image from a source registry to a target Docker Distribution registry.

## Quick Commands

```bash
# Build release binary
cargo build --release

# Run tests
cargo test

# Run with placeholders (will fail without real registries)
cargo run -- sync --dry-run nginx:1.25

# Run against real registries
export BAMBOO_SOURCE_REGISTRY=your-source-registry
export BAMBOO_TARGET_REGISTRY=your-target-registry
bamboo sync nginx:1.25
```

## Project Structure

```
src/
├── main.rs      # CLI entry point
├── cli.rs       # clap command definitions
├── error.rs     # Error types
├── image.rs     # Docker image reference parsing
├── auth.rs      # Docker config and --creds auth resolution
├── registry.rs  # OCI registry client wrapper
├── sync.rs      # Sync orchestration with retry and idempotency
└── logging.rs   # Timestamped colored log output
tests/
├── image_tests.rs            # Unit tests for image reference parsing
└── sync_integration_test.rs  # Ignored integration test skeleton
```

## Conventions

- Rust edition 2021, stable toolchain.
- Use `thiserror` for error types.
- Async runtime is `tokio`.
- OCI registry operations go through `oci-distribution`.
- CLI defaults are placeholders (`hubproxy.example.com`, `registry.example.com:5000`). Real addresses must be configured via CLI flags or environment variables.
- No runtime dependency on skopeo or Docker daemon.

## Important Notes

- The release binary is optimized for size (`strip`, `lto`, `opt-level = "z"`, `panic = "abort"`, `codegen-units = 1`). Current size is about 2.0 MB on macOS ARM64.
- Integration tests are marked `#[ignore]` and require real registries.
- Direct HTTP client dependency is `http`; actual registry HTTP calls go through `oci-distribution`.

## Files Not Tracked in Git

The following files are intentionally ignored by git and should not be committed:

- `CONTEXT.md`
- `docs/`
- `.superpowers/`
- IDE and environment files (see `.gitignore`)
