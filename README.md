# bamboo

一个用 Rust 编写的单二进制 CLI 工具，用于替代 `skopeo` 脚本，将容器镜像从一个源 Registry 同步到目标 Docker Distribution Registry。

## 安装

### 方式一：下载预编译二进制

从 [GitHub Releases](https://github.com/ofyann/bamboo/releases) 下载对应平台的二进制文件，解压后放入 `PATH` 即可。

```bash
# Linux x86_64 示例
wget https://github.com/ofyann/bamboo/releases/download/v0.1.0/bamboo-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
tar xzf bamboo-v0.1.0-x86_64-unknown-linux-gnu.tar.gz
mv bamboo /usr/local/bin/
```

### 方式二：从源码编译

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
# 查看帮助
bamboo sync --help

# 强制覆盖（即使 digest 一致）
bamboo sync --force nginx:1.25

# 使用账号密码认证目标 Registry
bamboo sync --creds username:password nginx:1.25

# 跳过目标 Registry 的 TLS 验证（自建私服常用）
bamboo sync --insecure-dest nginx:1.25

# 指定 docker config 认证文件
bamboo sync --authfile /path/to/config.json nginx:1.25
```

## 环境变量

所有命令行参数都支持对应的环境变量：

| 环境变量 | 说明 |
|---|---|
| `BAMBOO_SOURCE_REGISTRY` | 源 Registry 地址 |
| `BAMBOO_TARGET_REGISTRY` | 目标 Registry 地址 |
| `BAMBOO_CREDS` | 目标 Registry 认证，格式 `user:pass` |
| `BAMBOO_AUTHFILE` | Docker 认证文件路径 |
| `BAMBOO_INSECURE_SRC` | 跳过源 Registry TLS 验证 |
| `BAMBOO_INSECURE_DEST` | 跳过目标 Registry TLS 验证 |
| `BAMBOO_RETRIES` | 失败重试次数，默认 3 |
| `BAMBOO_RETRY_DELAY` | 重试间隔，默认 5s |
| `BAMBOO_PARALLEL_COPIES` | 并发 blob 复制数，默认 5 |

优先级：**命令行参数 > 环境变量 > 默认值**。

## 认证方式

工具支持两种认证方式，优先级如下：

1. `--creds user:pass` 命令行参数
2. `--authfile` 指定的 Docker config 文件（默认 `~/.docker/config.json`）
3. 匿名访问

## 功能特性

- 单二进制，无需安装 skopeo 或 Docker daemon
- 支持单架构和多架构（manifest list）镜像同步
- 基于 digest 的幂等同步，默认跳过已一致镜像
- 支持 `--dry-run` 空跑模式
- 自动处理 `docker.io/library/` 前缀和 HubProxy 路由
- 失败自动重试

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
