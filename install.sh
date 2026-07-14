#!/bin/bash
set -euo pipefail

REPO="ofyann/bamboo"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
VERSION="${VERSION:-latest}"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

if [ "$OS" != "linux" ] || [ "$ARCH" != "x86_64" ]; then
    echo "当前 install.sh 只支持 Linux x86_64" >&2
    exit 1
fi

if [ "$VERSION" = "latest" ]; then
    echo "正在查询最新版本..."
    VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name":' \
        | head -n 1 \
        | sed -E 's/.*"([^"]+)".*/\1/')
    if [ -z "$VERSION" ]; then
        echo "无法获取最新版本" >&2
        exit 1
    fi
    echo "最新版本: ${VERSION}"
fi

BINARY="bamboo-${VERSION}-x86_64-unknown-linux-gnu"
CHECKSUM="${BINARY}.sha256"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}"

TMP_DIR=$(mktemp -d)
trap 'rm -rf "${TMP_DIR}"' EXIT

echo "下载 ${BINARY}..."
curl -fsSL -o "${TMP_DIR}/${BINARY}" "${DOWNLOAD_URL}/${BINARY}"
echo "下载 ${CHECKSUM}..."
curl -fsSL -o "${TMP_DIR}/${CHECKSUM}" "${DOWNLOAD_URL}/${CHECKSUM}"

echo "校验 sha256..."
cd "${TMP_DIR}"
sha256sum -c "${CHECKSUM}"

chmod +x "${TMP_DIR}/${BINARY}"

echo "安装到 ${INSTALL_DIR}/bamboo ..."
if [ -w "$INSTALL_DIR" ]; then
    mv "${TMP_DIR}/${BINARY}" "${INSTALL_DIR}/bamboo"
else
    sudo mv "${TMP_DIR}/${BINARY}" "${INSTALL_DIR}/bamboo"
fi

echo "安装完成: $(${INSTALL_DIR}/bamboo --version)"
