# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `bamboo sync-all` subcommand for batch syncing images from a configuration list.
- Support for loading and merging multiple TOML config files via repeated `--config`.
- Image list configuration with `[[images]]` entries and optional per-image source/target registry and credential overrides.
- `continue_on_error` option in the image list config to control whether batch sync stops on the first failure.
- `--jobs` / `BAMBOO_JOBS` flag to control concurrency for `sync-all`.
- Environment variable overrides for `sync-all` global settings.
- `--target-creds` as the primary flag for target registry credentials, with `--creds` kept as a visible alias.
- CI workflow with `cargo test`, `cargo fmt --check`, and `cargo clippy -- -D warnings`.

### Changed

- Unified crate root: all modules are now exposed through `src/lib.rs`, and `src/main.rs` is a thin binary wrapper.
- Introduced `SyncSpec`, `RegistryEndpoint`, `AuthPair`, `SyncPolicy`, and `ConfigResolver` as the central domain interface for both `sync` and `sync-all`.
- Removed the environment-variable side channel in `config.rs`; configuration precedence is now resolved explicitly by `ConfigResolver`.
- `bamboo sync` and `bamboo sync-all` now read Docker config and TOML config files via `tokio::fs` instead of blocking `std::fs`.
- Refactored `registry.rs` to share single-arch and multi-arch child manifest copy logic through `copy_single_manifest`.
- `--retries` help text now clarifies it means maximum attempts (0 still runs once).

### Fixed

- `retries = 0` no longer causes a panic in the retry loop.
- `sync-all` now correctly passes `--force`, `--quiet`, and `--verbose` to each individual sync.
- `tests/sync_integration_test.rs` now checks `stdout` instead of `stderr` for the success log.
- Docker config `auth` fields that are not valid `user:pass` base64 are now rejected with an error instead of being silently ignored.

### Changed

- `--insecure-src` / `--insecure-dest` now mean "use HTTP protocol", matching the original skopeo script semantics.
- `--skip-tls-verify-src` / `--skip-tls-verify-dest` are introduced for HTTPS + skip certificate verification.
- `BAMBOO_INSECURE_SRC` / `BAMBOO_INSECURE_DEST` environment variables now map to HTTP mode.

## [0.2.0] - 2026-07-14

### Added

- Source registry authentication via `--source-creds` and `BAMBOO_SOURCE_CREDS`.
- `--version` / `-V` flag to print the CLI version.
- `--timeout` / `BAMBOO_TIMEOUT` for sync timeout (default 10m, 0 disables).
- `--quiet` / `-q` and `--verbose` / `-v` log level flags.
- Textual per-blob progress feedback during sync.
- `bamboo init` command to generate a `bamboo.toml` template.
- `--config` / `BAMBOO_CONFIG` to load settings from a TOML file.

### Changed

- `--dry-run` now prints `http://` or `https://` based on `--insecure-src` / `--insecure-dest`.
- Release workflow now uploads the raw executable binary along with a `.sha256` checksum file instead of a tarball.
- README 一键安装改为直接串联的下载/校验/安装命令，`install.sh` 作为备选方案保留。

### Fixed

- Multi-arch image sync now preserves child manifest digests, keeping index references valid.

### Removed

- Unused `--parallel-copies` CLI parameter.

## [0.1.0] - 2026-07-14

### Added

- Initial implementation of `bamboo` CLI.
- `bamboo sync <image:tag>` command for syncing a single image between registries.
- Docker image reference parsing with `library/` normalization.
- HubProxy routing support.
- Digest-based idempotency check with `--force` override.
- `--creds` and `~/.docker/config.json` authentication.
- `--insecure-src` / `--insecure-dest` TLS options.
- Fixed retry policy (3 retries, 5s delay).
- Single-arch and multi-arch (manifest list) image sync.
- GitHub Actions workflow for automated x86_64 Linux releases.
