# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.6] - 2026-07-17

### Fixed

- 彻底修复多架构镜像在指定 `--platform` 时目标 Registry 报 `BLOB_UNKNOWN` 的问题：平台过滤后只同步选中的子 manifest，并重写 index 只保留已同步的平台条目，不再推送引用未同步子 manifest 的原 index。

## [0.3.5] - 2026-07-17

### Fixed

- 修复同步多 tag/多架构镜像时目标 Registry 报 `BLOB_UNKNOWN` 的问题：关闭 `OciRegistry::blob_exists` 中基于 `pull_blob_stream` 的存在性探测（部分 Registry 会返回假阳性），改为始终让 `push_blob` 自行用标准 `HEAD /v2/{repo}/blobs/{digest}` 流程处理去重。

## [0.3.4] - 2026-07-17

### Changed

- **破坏性变更**：目标 Registry 认证统一重命名为 `dest_creds`。
  - 命令行：`--dest-creds`（移除 `--target-creds` / `--creds` 别名）。
  - 环境变量：`BAMBOO_DEST_CREDS`（移除 `BAMBOO_CREDS`）。
  - 配置文件：`dest_creds = "user:pass"`（移除 `creds`）。
- **破坏性变更**：目标 Registry 地址统一重命名为 `dest_registry`。
  - 命令行：`--dest-registry`（移除 `--target-registry`）。
  - 环境变量：`BAMBOO_DEST_REGISTRY`（移除 `BAMBOO_TARGET_REGISTRY`）。
  - 配置文件：`dest_registry = "registry.example.com:5000"`（移除 `target_registry`）。

### Fixed

- 重试日志现在会打印具体错误原因，方便排查同步失败根因。

## [0.3.3] - 2026-07-17

### Added

- 按镜像/平台汇总 blob 同步进度：`TerminalProgressSink` 现在会在每个单架构镜像或每个多架构平台内聚合 config + layers 的总进度，每 10% 里程碑输出一行汇总（例如 `[redis:8 (linux/amd64)] 总进度 2/5 blobs，120 MiB / 1.5 GiB (8%)`）。
- 配置文件严格校验：`ConfigFile` 现在拒绝未知字段，并对 `platform`、`timeout`、`retry_delay`、`jobs`、`images` 等字段格式进行校验，错误信息统一为中文。
- 多架构镜像平台过滤：`--platform` / 配置 `platform` 可指定只同步某个 OS/ARCH（或 OS/ARCH/VARIANT），例如 `linux/amd64`。
- 目标仓库 blob 存在性检查：同步前检测目标 Registry 是否已有对应 blob，有则跳过拉取和推送，提升幂等同步效率。
- 中文错误翻译层：新增 `error_reporter.rs`，将 Registry 网络、TLS、manifest 不存在等底层错误翻译成更直接的中文提示。

### Changed

- 单镜像 `bamboo sync` 与批量 `bamboo sync-all` 统一使用 `SyncEngine`，集中处理 retry、timeout、并发控制与失败聚合。
- `progress.rs` 进度接口增加 `init_manifest`，`NoopProgressSink` 与 `TerminalProgressSink` 同步更新。

## [0.3.2] - 2026-07-16

### Added

- 实时 blob 同步进度输出：`bamboo sync` / `bamboo sync-all` 现在会在终端显示每个镜像的 blob 拉取/推送进度（按 ~10% 里程碑），多架构镜像会标注平台信息。

### Fixed

- 绕过 `oci-distribution` 0.11 的 panic：向不返回 `Location` header 的 Registry 推送多架构子 manifest 时，改用临时 `_bamboo_child_<digest>` tag 推送，index 仍按 digest 引用。

## [0.3.1] - 2026-07-16

### Fixed

- `bamboo sync-all` now correctly inherits `insecure_src` / `insecure_dest` from the global config or CLI args when a per-image entry does not override them.

## [0.3.0] - 2026-07-15

### Added

- `bamboo sync-all` subcommand for batch syncing images from a configuration list.
- Support for loading and merging multiple TOML config files via repeated `--config`.
- Image list configuration with `[[images]]` entries and optional per-image source/target registry and credential overrides.
- `continue_on_error` option in the image list config to control whether batch sync stops on the first failure.
- `--jobs` / `BAMBOO_JOBS` flag to control concurrency for `sync-all`.
- Environment variable overrides for `sync-all` global settings.
- `--target-creds` as the primary flag for target registry credentials, with `--creds` kept as a visible alias.
- `--skip-tls-verify-src` / `--skip-tls-verify-dest` flags for HTTPS + skip certificate verification.
- `Registry` trait, `OciRegistry` adapter, `InMemoryRegistry` test fake, and `ManifestCopier` for copy logic.
- `SyncEngine` module to centralize retry, timeout, and batch concurrency control.
- `tracing` and `tracing-subscriber` dependencies; per-image and per-platform log spans.
- CI workflow with `cargo test`, `cargo fmt --check`, and `cargo clippy -- -D warnings`.

### Changed

- Unified crate root: all modules are now exposed through `src/lib.rs`, and `src/main.rs` is a thin binary wrapper.
- Introduced `SyncSpec`, `RegistryEndpoint`, `AuthPair`, `SyncPolicy`, and `ConfigResolver` as the central domain interface for both `sync` and `sync-all`.
- Removed the environment-variable side channel in `config.rs`; configuration precedence is now resolved explicitly by `ConfigResolver`.
- `bamboo sync` and `bamboo sync-all` now read Docker config and TOML config files via `tokio::fs` instead of blocking `std::fs`.
- Refactored `registry.rs` to share single-arch and multi-arch child manifest copy logic through `copy_single_manifest`.
- `--retries` help text now clarifies it means maximum attempts (0 still runs once).
- `--insecure-src` / `--insecure-dest` now mean "use HTTP protocol", matching the original skopeo script semantics.
- `BAMBOO_INSECURE_SRC` / `BAMBOO_INSECURE_DEST` environment variables now map to HTTP mode.
- `sync.rs` and `sync_all.rs` are now thin adapters that delegate to `SyncEngine`.
- Replaced the global `LOG_LEVEL` static with `tracing`; `logging.rs` now initializes a subscriber.

### Fixed

- `retries = 0` no longer causes a panic in the retry loop.
- `sync-all` now correctly passes `--force`, `--quiet`, and `--verbose` to each individual sync.
- `tests/sync_integration_test.rs` now checks `stdout` instead of `stderr` for the success log.
- Docker config `auth` fields that are not valid `user:pass` base64 are now rejected with an error instead of being silently ignored.

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
