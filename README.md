# bamboo

一个用 Rust 编写的单二进制 CLI 工具，用于替代 `skopeo` 脚本，将容器镜像从一个源 Registry 同步到目标 Docker Distribution Registry。

## 安装

### 方式一：直接下载并安装（Linux x86_64）

从 [GitHub Releases](https://github.com/ofyann/bamboo/releases) 下载对应版本的原始二进制和 `.sha256` 校验文件，校验通过后安装到 `/usr/local/bin`：

```bash
VERSION="v0.2.0"
BINARY="bamboo-${VERSION}-x86_64-unknown-linux-gnu"

curl -fsSL -O "https://github.com/ofyann/bamboo/releases/download/${VERSION}/${BINARY}" \
  -O "https://github.com/ofyann/bamboo/releases/download/${VERSION}/${BINARY}.sha256" \
  && sha256sum -c "${BINARY}.sha256" \
  && chmod +x "${BINARY}" \
  && sudo mv "${BINARY}" /usr/local/bin/bamboo
```

将 `v0.2.0` 替换为实际要安装的 release 版本号。

### 方式二：使用 install.sh 脚本

如果希望自动查询 latest release 并安装，可以使用仓库中的 `install.sh`：

```bash
curl -fsSL https://raw.githubusercontent.com/ofyann/bamboo/main/install.sh | bash
```

安装目录默认为 `/usr/local/bin`，可通过 `INSTALL_DIR` 修改；默认查询最新 release，也可通过 `VERSION` 指定版本：

```bash
# 安装到自定义目录
INSTALL_DIR=~/.local/bin curl -fsSL https://raw.githubusercontent.com/ofyann/bamboo/main/install.sh | bash

# 安装指定版本
VERSION=v0.2.0 curl -fsSL https://raw.githubusercontent.com/ofyann/bamboo/main/install.sh | bash
```

脚本会校验下载文件的 sha256、备份旧版本、并在安装后执行 `bamboo --version` 验证。

### 方式三：从源码编译

```bash
git clone https://github.com/ofyann/bamboo.git
cd bamboo
cargo build --release
# 产物位于 target/release/bamboo
```

## 快速开始

### 1. 配置 Registry 地址

通过环境变量配置：

```bash
export BAMBOO_SOURCE_REGISTRY=your-source-registry.example.com
export BAMBOO_TARGET_REGISTRY=your-target-registry.example.com:5000
```

或者每次通过命令行参数指定：

```bash
bamboo sync \
  --source-registry your-source-registry.example.com \
  --target-registry your-target-registry.example.com:5000 \
  nginx:1.25
```

### 2. 空跑检查

```bash
bamboo sync --dry-run nginx:1.25
```

会打印解析后的源地址和目标地址，不执行实际同步。

### 3. 执行同步

```bash
bamboo sync nginx:1.25
```

如果目标 Registry 已存在相同 digest 的镜像，会自动跳过。

## 常用命令

```bash
# 查看版本号
bamboo --version

# 生成配置文件模板
bamboo init

# 使用配置文件同步
bamboo sync --config bamboo.toml nginx:1.25

# 查看帮助
bamboo sync --help

# 强制覆盖（即使 digest 一致）
bamboo sync --force nginx:1.25

# 使用账号密码认证目标 Registry
bamboo sync --creds username:password nginx:1.25

# 源 Registry 也需要认证
bamboo sync --source-creds username:password nginx:1.25

# 跳过目标 Registry 的 TLS 验证（自建私服常用）
bamboo sync --insecure-dest nginx:1.25

# 指定 docker config 认证文件
bamboo sync --authfile /path/to/config.json nginx:1.25

# 设置同步超时为 30 分钟
bamboo sync --timeout 30m nginx:1.25

# 只输出警告和错误（减少日志）
bamboo sync --quiet nginx:1.25

# 输出调试日志
bamboo sync --verbose nginx:1.25
```

## 批量同步（sync-all）

除了同步单个镜像，`bamboo` 还支持按配置文件里的列表批量同步。适合用 cron / systemd timer 做定时同步。

### 基础配置（`base.toml`）

放全局的源/目标 Registry 和鉴权信息，使用现有的 `bamboo.toml` 格式：

```toml
source_registry = "hubproxy.example.com"
target_registry = "registry.example.com:5000"
creds = "user:pass"
retries = 3
retry_delay = "5s"
timeout = "10m"
```

### 镜像列表配置（`images.toml`）

只放要同步的镜像列表，以及需要覆盖的参数：

```toml
continue_on_error = false

[[images]]
image = "nginx:1.25"

[[images]]
image = "redis:7"
# 这个镜像单独指定目标 Registry
source_registry = "mirror-a.example.com"
target_registry = "local-redis.example.com:5000"
```

每个 `[[images]]` 支持的字段：

| 字段 | 说明 |
|---|---|
| `image` | 必填，镜像引用，例如 `nginx:1.25` |
| `source_registry` | 可选，覆盖全局源 Registry |
| `target_registry` | 可选，覆盖全局目标 Registry |
| `source_creds` | 可选，覆盖源 Registry 认证 |
| `creds` | 可选，覆盖目标 Registry 认证 |
| `authfile` | 可选，覆盖 Docker 认证文件路径 |
| `insecure_src` | 可选，覆盖源 TLS 设置 |
| `insecure_dest` | 可选，覆盖目标 TLS 设置 |

### 执行批量同步

```bash
bamboo sync-all --config base.toml --config images.toml
```

空跑检查：

```bash
bamboo sync-all --config base.toml --config images.toml --dry-run
```

多个 `--config` 会按顺序合并：全局字段后者覆盖前者，`images` 列表会追加。

### 结合 cron 定时同步

例如每天凌晨 2 点执行：

```cron
0 2 * * * /usr/local/bin/bamboo sync-all --config /etc/bamboo/base.toml --config /etc/bamboo/images.toml
```

`continue_on_error = true` 时，单个镜像失败会继续同步剩余镜像，最后统一输出失败列表；默认 `false`，失败即终止。

## 环境变量

所有命令行参数都支持对应的环境变量：

| 环境变量 | 说明 |
|---|---|
| `BAMBOO_CONFIG` | TOML 配置文件路径 |
| `BAMBOO_SOURCE_REGISTRY` | 源 Registry 地址 |
| `BAMBOO_TARGET_REGISTRY` | 目标 Registry 地址 |
| `BAMBOO_SOURCE_CREDS` | 源 Registry 认证，格式 `user:pass` |
| `BAMBOO_CREDS` | 目标 Registry 认证，格式 `user:pass` |
| `BAMBOO_AUTHFILE` | Docker 认证文件路径（同时用于源和目标） |
| `BAMBOO_INSECURE_SRC` | 跳过源 Registry TLS 验证 |
| `BAMBOO_INSECURE_DEST` | 跳过目标 Registry TLS 验证 |
| `BAMBOO_RETRIES` | 失败重试次数，默认 3 |
| `BAMBOO_RETRY_DELAY` | 重试间隔，默认 5s |
| `BAMBOO_TIMEOUT` | 同步超时时间，默认 10m，0 表示不超时 |
| `BAMBOO_QUIET` | 只输出 WARN 及以上日志 |
| `BAMBOO_VERBOSE` | 输出 DEBUG 日志 |

优先级：**命令行参数 > 环境变量 > 默认值**。

## 认证方式

目标 Registry 支持三种认证方式，优先级如下：

1. `--creds user:pass` 命令行参数
2. `--authfile` 指定的 Docker config 文件（默认 `~/.docker/config.json`）
3. 匿名访问

源 Registry 默认匿名访问；如需认证，使用 `--source-creds user:pass`，同样也会读取 `--authfile` 中对应源 Registry 地址的凭据。

## 功能特性

- 单二进制，无需安装 skopeo 或 Docker daemon
- 支持单架构和多架构（manifest list）镜像同步
- 基于 digest 的幂等同步，默认跳过已一致镜像
- 支持 `--dry-run` 空跑模式
- 自动处理 `docker.io/library/` 前缀和 HubProxy 路由
- 失败自动重试，支持自定义超时
- 同步过程中逐层显示进度（config / layer 开始与完成）
- 支持 `--quiet` / `--verbose` 调整日志级别
- 支持 `bamboo init` 生成 TOML 配置文件模板，并通过 `--config` 读取

## 与 skopeo 脚本的对应关系

| 原脚本行为 | bamboo 命令 |
|---|---|
| `sync.sh nginx:1.25` | `bamboo sync nginx:1.25` |
| `--dry-run` | `bamboo sync --dry-run nginx:1.25` |
| `INSECURE_DEST=true` | `bamboo sync --insecure-dest nginx:1.25` |
| `DEST_CREDS=user:pass` | `bamboo sync --creds user:pass nginx:1.25` |
| `MAX_RETRIES=3` | 默认 3 次，可通过 `--retries` 修改 |

## 发布记录

查看 [CHANGELOG.md](./CHANGELOG.md) 或 [GitHub Releases](https://github.com/ofyann/bamboo/releases)。

## 注意事项

- 默认地址是占位符，使用前必须通过参数或环境变量配置真实 Registry。
- 目标 Registry 如果是自建私服且使用自签名证书，通常需要加 `--insecure-dest`。
- 集成测试需要真实 Registry，默认被忽略。
