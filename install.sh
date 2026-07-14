#!/bin/bash
#
# 安装脚本：从 GitHub Releases 下载 bamboo 并安装到指定目录。
#
# 用法：
#   curl -fsSL https://raw.githubusercontent.com/ofyann/bamboo/main/install.sh | bash
#   VERSION=v0.2.0 INSTALL_DIR=~/.local/bin curl ... | bash
#
set -euo pipefail

# -----------------------------------------------------------------------------
# 配置与常量
# -----------------------------------------------------------------------------
REPO="ofyann/bamboo"
DEFAULT_INSTALL_DIR="/usr/local/bin"
DEFAULT_VERSION="latest"

INSTALL_DIR="${INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"
VERSION="${VERSION:-$DEFAULT_VERSION}"

# 平台支持矩阵（当前仅提供 Linux x86_64 构建）
SUPPORTED_OS="linux"
SUPPORTED_ARCH="x86_64"

# -----------------------------------------------------------------------------
# 日志工具
# -----------------------------------------------------------------------------
log_info()  { printf '\033[32m[INFO]\033[0m %s\n' "$*"; }
log_warn()  { printf '\033[33m[WARN]\033[0m %s\n' "$*" >&2; }
log_error() { printf '\033[31m[ERROR]\033[0m %s\n' "$*" >&2; }

# -----------------------------------------------------------------------------
# 依赖检查
# -----------------------------------------------------------------------------
require_command() {
    local cmd="$1"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        log_error "缺少必要命令: $cmd"
        exit 1
    fi
}

require_command curl
require_command uname
require_command mktemp
require_command chmod
require_command mv

# 优先使用 sha256sum，macOS 等环境可回退到 shasum
if command -v sha256sum >/dev/null 2>&1; then
    SHASUM_CMD="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
    SHASUM_CMD="shasum -a 256"
else
    log_error "缺少 sha256 校验工具（需要 sha256sum 或 shasum）"
    exit 1
fi

# -----------------------------------------------------------------------------
# 平台检测
# -----------------------------------------------------------------------------
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

if [ "$OS" != "$SUPPORTED_OS" ] || [ "$ARCH" != "$SUPPORTED_ARCH" ]; then
    log_error "当前平台 ${OS}/${ARCH} 不受支持，本脚本仅支持 ${SUPPORTED_OS}/${SUPPORTED_ARCH}"
    exit 1
fi

# -----------------------------------------------------------------------------
# 版本解析
# -----------------------------------------------------------------------------
resolve_latest_version() {
    log_info "正在查询最新版本..."

    local api_url="https://api.github.com/repos/${REPO}/releases/latest"
    local response
    local http_code

    # 先尝试 HTTP 请求并捕获状态码，便于给出更友好的错误信息
    response=$(curl -fsSL --retry 3 --connect-timeout 10 --max-time 30 "$api_url" 2>&1) || {
        log_error "无法从 GitHub API 获取最新版本"
        log_error "$response"
        exit 1
    }

    # 若安装了 jq 则使用 jq 解析，否则使用更鲁棒的 sed/grep 组合
    if command -v jq >/dev/null 2>&1; then
        VERSION=$(printf '%s\n' "$response" | jq -r '.tag_name // empty')
    else
        VERSION=$(printf '%s\n' "$response" \
            | grep -m1 '"tag_name":' \
            | sed -E 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')
    fi

    if [ -z "$VERSION" ] || [ "$VERSION" = "null" ]; then
        log_error "无法从 GitHub API 响应中解析最新版本号"
        exit 1
    fi

    log_info "最新版本: $VERSION"
}

if [ "$VERSION" = "latest" ]; then
    resolve_latest_version
fi

# 简单校验版本号格式：以 v 开头，后跟数字
if ! printf '%s\n' "$VERSION" | grep -qE '^v[0-9]+\.[0-9]+'; then
    log_warn "版本号 $VERSION 看起来不是标准 release tag，安装可能失败"
fi

# -----------------------------------------------------------------------------
# 下载路径与临时目录
# -----------------------------------------------------------------------------
BINARY="bamboo-${VERSION}-${SUPPORTED_ARCH}-unknown-${SUPPORTED_OS}-gnu"
CHECKSUM="${BINARY}.sha256"
DOWNLOAD_BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "${TMP_DIR}"' EXIT INT TERM HUP

TMP_BINARY="${TMP_DIR}/${BINARY}"
TMP_CHECKSUM="${TMP_DIR}/${CHECKSUM}"

# -----------------------------------------------------------------------------
# 文件下载
# -----------------------------------------------------------------------------
download() {
    local url="$1"
    local out="$2"
    local desc="$3"

    log_info "下载 ${desc}..."
    if ! curl -fsSL --retry 3 --connect-timeout 10 --max-time 300 \
        -o "$out" "$url"; then
        log_error "下载失败: $url"
        exit 1
    fi

    if [ ! -s "$out" ]; then
        log_error "下载结果为空: $out"
        exit 1
    fi
}

download "${DOWNLOAD_BASE_URL}/${BINARY}" "$TMP_BINARY" "$BINARY"
download "${DOWNLOAD_BASE_URL}/${CHECKSUM}" "$TMP_CHECKSUM" "$CHECKSUM"

# -----------------------------------------------------------------------------
# 校验和检查
# -----------------------------------------------------------------------------
log_info "校验 sha256..."

# 校验文件内容必须包含目标二进制文件名
if ! grep -qF "$BINARY" "$TMP_CHECKSUM"; then
    log_error "校验文件中未找到 ${BINARY} 的校验值"
    exit 1
fi

# 在临时目录中执行校验，避免路径干扰
if ! (cd "$TMP_DIR" && $SHASUM_CMD -c "$CHECKSUM" >/dev/null); then
    log_error "sha256 校验失败，下载文件可能被篡改或损坏"
    exit 1
fi

chmod +x "$TMP_BINARY"

# 简单校验是否为 ELF 可执行文件
if command -v file >/dev/null 2>&1; then
    if ! file "$TMP_BINARY" | grep -qE 'ELF.*executable'; then
        log_warn "下载文件看起来不是 ELF 可执行文件，继续安装但可能需要检查"
    fi
fi

# -----------------------------------------------------------------------------
# 安装目录准备
# -----------------------------------------------------------------------------
TARGET_BINARY="${INSTALL_DIR}/bamboo"
BACKUP_SUFFIX=".bak.$(date +%Y%m%d%H%M%S)"

if [ ! -d "$INSTALL_DIR" ]; then
    log_info "安装目录不存在，尝试创建: $INSTALL_DIR"
    if [ -w "$(dirname "$INSTALL_DIR")" ]; then
        mkdir -p "$INSTALL_DIR"
    elif command -v sudo >/dev/null 2>&1; then
        sudo mkdir -p "$INSTALL_DIR"
    else
        log_error "无法创建安装目录: $INSTALL_DIR"
        exit 1
    fi
fi

# -----------------------------------------------------------------------------
# 备份已有二进制
# -----------------------------------------------------------------------------
if [ -e "$TARGET_BINARY" ]; then
    log_info "备份已有二进制: ${TARGET_BINARY}${BACKUP_SUFFIX}"
    if [ -w "$INSTALL_DIR" ]; then
        cp -a "$TARGET_BINARY" "${TARGET_BINARY}${BACKUP_SUFFIX}" || true
    elif command -v sudo >/dev/null 2>&1; then
        sudo cp -a "$TARGET_BINARY" "${TARGET_BINARY}${BACKUP_SUFFIX}" || true
    fi
fi

# -----------------------------------------------------------------------------
# 原子安装
# -----------------------------------------------------------------------------
log_info "安装到 ${TARGET_BINARY}..."

TARGET_TMP="${INSTALL_DIR}/.bamboo.tmp.$$"

install_file() {
    local src="$1"
    local dst="$2"

    if [ -w "$INSTALL_DIR" ]; then
        cp "$src" "$dst" && chmod +x "$dst" && mv "$dst" "$TARGET_BINARY"
    elif command -v sudo >/dev/null 2>&1; then
        sudo cp "$src" "$dst" && sudo chmod +x "$dst" && sudo mv "$dst" "$TARGET_BINARY"
    else
        log_error "没有 ${INSTALL_DIR} 的写入权限，且未找到 sudo"
        exit 1
    fi
}

if ! install_file "$TMP_BINARY" "$TARGET_TMP"; then
    log_error "安装失败"
    # 尝试恢复备份
    if [ -e "${TARGET_BINARY}${BACKUP_SUFFIX}" ]; then
        log_info "尝试恢复备份..."
        if [ -w "$INSTALL_DIR" ]; then
            mv "${TARGET_BINARY}${BACKUP_SUFFIX}" "$TARGET_BINARY" || true
        elif command -v sudo >/dev/null 2>&1; then
            sudo mv "${TARGET_BINARY}${BACKUP_SUFFIX}" "$TARGET_BINARY" || true
        fi
    fi
    exit 1
fi

# 清理备份（安装成功则无需保留）
if [ -e "${TARGET_BINARY}${BACKUP_SUFFIX}" ]; then
    if [ -w "$INSTALL_DIR" ]; then
        rm -f "${TARGET_BINARY}${BACKUP_SUFFIX}" || true
    elif command -v sudo >/dev/null 2>&1; then
        sudo rm -f "${TARGET_BINARY}${BACKUP_SUFFIX}" || true
    fi
fi

# -----------------------------------------------------------------------------
# 安装后验证
# -----------------------------------------------------------------------------
log_info "验证安装..."
if ! "$TARGET_BINARY" --version >/dev/null 2>&1; then
    log_error "安装后无法执行: $TARGET_BINARY"
    exit 1
fi

log_info "安装完成: $("$TARGET_BINARY" --version)"
