# AGENTS.md

## 项目概述

`bamboo` 是一个 Rust 编写的单二进制 CLI 工具，用于替代基于 `skopeo` 的 bash 脚本，将单个容器镜像从一个源 Registry 同步到目标 Docker Distribution Registry。

## 常用命令

```bash
# 编译 release 二进制
cargo build --release

# 运行测试
cargo test

# 查看版本号
bamboo --version

# 空跑（使用占位符默认地址，不会真正执行）
cargo run -- sync --dry-run nginx:1.25

# 真实同步（配置实际 Registry 地址）
export BAMBOO_SOURCE_REGISTRY=你的源镜像代理地址
export BAMBOO_TARGET_REGISTRY=你的目标私服地址
bamboo sync nginx:1.25

# 源 Registry 也需要认证时
bamboo sync --source-creds user:pass nginx:1.25

# 设置超时与日志级别
bamboo sync --timeout 30m --quiet nginx:1.25
```

## 项目结构

```
src/
├── main.rs      # 程序入口
├── cli.rs       # 命令行参数定义
├── error.rs     # 错误类型
├── image.rs     # 镜像引用解析
├── auth.rs      # 认证信息解析
├── registry.rs  # OCI Registry 客户端封装
├── sync.rs      # 同步编排（重试、幂等）
└── logging.rs   # 带时间戳的彩色日志输出
tests/
├── common/                   # 测试共享辅助代码
│   └── mock_registry.rs      # 基于 axum 的最小 OCI Registry mock
├── image_tests.rs            # 镜像解析单元测试
├── sync_integration_test.rs  # 需要真实 Registry 的集成测试骨架
└── sync_test.rs              # 基于 mock Registry 的同步集成测试
```

## 开发约定

- 使用 Rust 2021 edition，stable 工具链。
- 错误处理使用 `thiserror`。
- 异步运行时使用 `tokio`。
- OCI Registry 操作通过 `oci-distribution` 完成。
- CLI 默认地址是占位符（`hubproxy.example.com`、`registry.example.com:5000`），实际地址需通过参数或环境变量配置。
- 不依赖 skopeo 或 Docker daemon。

## 提交规范

本项目使用 [gitmoji](https://gitmoji.dev/) 作为提交信息前缀。提交信息使用中文，粒度适中，不要过细拆分。

### 常用 gitmoji

| emoji | 含义 | 使用场景 |
|---|---|---|
| ✨ `:sparkles:` | 引入新功能 | 新增命令、新增模块 |
| 🐛 `:bug:` | 修复 bug | 修复同步失败、解析错误 |
| ♻️ `:recycle:` | 重构代码 | 重构某个模块，不修改外部行为 |
| ⚡️ `:zap:` | 提升性能 | 优化并发、减少内存占用 |
| 🔥 `:fire:` | 删除代码或文件 | 移除无用模块、废弃功能 |
| 📝 `:memo:` | 文档相关 | 更新 README、CHANGELOG、AGENTS |
| 🎨 `:art:` | 改进代码结构/格式 | 调整代码风格、格式化 |
| ✅ `:white_check_mark:` | 添加/更新测试 | 新增测试用例 |
| 🔒️ `:lock:` | 修复安全问题 | 认证、TLS 相关安全修复 |
| 🔧 `:wrench:` | 配置文件 | 修改 Cargo.toml、CI workflow |
| 👷 `:construction_worker:` | CI 构建系统 | 修改 GitHub Actions |
| 📦️ `:package:` | 编译产物/打包 | release 构建相关 |
| 🚀 `:rocket:` | 部署相关 | 发布版本 |
| 🏷️ `:label:` | release / 版本标签 | 打 tag |

### 提交粒度

- 一个提交完成一个完整的、可独立 review 的改动。
- 不要把"修改代码 + 更新文档 + 调整配置"拆成三个提交，除非它们确实可以独立存在。
- 示例：
  - ✅ `✨ feat: 添加镜像引用解析模块`
  - ✅ `🐛 fix: 修复带端口 registry 被误判为 tag 的问题`
  - ✅ `🔧 chore: 添加 release 构建优化配置`
  - ❌ `修改 image.rs`（过于模糊）
  - ❌ `fix typo`、`update doc`、`adjust format` 拆成三个提交（过细）

## 发布流程

1. 更新 `CHANGELOG.md`，按需修改 `Cargo.toml` 版本号。
2. 提交变更：
   ```bash
   git add -A
   git commit -m "📝 docs: 更新 CHANGELOG 并 bump 版本到 v0.1.0"
   ```
3. 打 tag 并推送：
   ```bash
   git tag -a v0.1.0 -m "Release v0.1.0"
   git push origin main --tags
   ```
4. GitHub Actions（`.github/workflows/release.yml`）会自动构建 `x86_64-unknown-linux-gnu` release 二进制并上传到 GitHub Release。

## 重要提示

- release 二进制经过体积优化（`strip`、`lto`、`opt-level = "z"`、`panic = "abort"`、`codegen-units = 1`），目前在 macOS ARM64 上约 2.0 MB，Linux x86_64 压缩后约 1.2 MB。
- 集成测试默认标记为 `#[ignore]`，需要真实 Registry 才能运行。
- 直接 HTTP 依赖为 `http` crate，实际 Registry HTTP 调用由 `oci-distribution` 处理。

## 不纳入 git 的文件

以下文件已被 `.gitignore` 忽略，不应提交：

- `CONTEXT.md`
- `docs/`
- `.superpowers/`
- IDE 和环境文件
