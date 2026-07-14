# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
