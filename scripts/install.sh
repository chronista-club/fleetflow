#!/bin/sh
# FleetFlow インストールスクリプト
# Usage: curl -fsSL https://fleetflow.run/install | sh
set -e

REPO="chronista-club/fleetflow"
BINARY_NAME="fleet"
INSTALL_DIR="/usr/local/bin"

# カラー出力
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

info() { printf "${BLUE}${BOLD}%s${NC}\n" "$1"; }
success() { printf "${GREEN}${BOLD}%s${NC}\n" "$1"; }
warn() { printf "${YELLOW}%s${NC}\n" "$1"; }
error() { printf "${RED}${BOLD}%s${NC}\n" "$1" >&2; exit 1; }

# OS/Arch 検出
detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$OS" in
        linux)  OS="linux" ;;
        darwin) OS="darwin" ;;
        *)      error "未対応のOS: $OS" ;;
    esac

    case "$ARCH" in
        x86_64|amd64)   ARCH="amd64" ;;
        aarch64|arm64)  ARCH="arm64" ;;
        *)              error "未対応のアーキテクチャ: $ARCH" ;;
    esac

    ASSET_NAME="fleetflow-${OS}-${ARCH}.tar.gz"
}

# 最新バージョン取得
get_latest_version() {
    if command -v curl > /dev/null 2>&1; then
        VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": *"//;s/".*//')
    elif command -v wget > /dev/null 2>&1; then
        VERSION=$(wget -qO- "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": *"//;s/".*//')
    else
        error "curl または wget が必要です"
    fi

    [ -z "$VERSION" ] && error "最新バージョンの取得に失敗しました"
    VERSION_NUM=$(echo "$VERSION" | sed 's/^v//')
}

# ダウンロード & インストール
install() {
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET_NAME}"
    TMP_DIR=$(mktemp -d)

    info "FleetFlow ${VERSION} をインストールします"
    printf "  プラットフォーム: ${CYAN}${OS}-${ARCH}${NC}\n"
    printf "  インストール先:  ${CYAN}${INSTALL_DIR}/${BINARY_NAME}${NC}\n"
    echo ""

    # ダウンロード
    info "ダウンロード中..."
    if command -v curl > /dev/null 2>&1; then
        curl -fsSL "$DOWNLOAD_URL" -o "${TMP_DIR}/${ASSET_NAME}" || error "ダウンロード失敗: ${DOWNLOAD_URL}"
    else
        wget -q "$DOWNLOAD_URL" -O "${TMP_DIR}/${ASSET_NAME}" || error "ダウンロード失敗: ${DOWNLOAD_URL}"
    fi

    # 展開
    tar -xzf "${TMP_DIR}/${ASSET_NAME}" -C "$TMP_DIR" || error "展開に失敗しました"

    # インストール
    if [ -w "$INSTALL_DIR" ]; then
        mv "${TMP_DIR}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
        chmod +x "${INSTALL_DIR}/${BINARY_NAME}"
    else
        info "sudo が必要です"
        sudo mv "${TMP_DIR}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
        sudo chmod +x "${INSTALL_DIR}/${BINARY_NAME}"
    fi

    # クリーンアップ
    rm -rf "$TMP_DIR"

    echo ""
    success "✓ FleetFlow ${VERSION_NUM} をインストールしました！"
    echo ""

    # PATH チェック
    if ! echo "$PATH" | tr ':' '\n' | grep -q "^${INSTALL_DIR}$"; then
        warn "⚠ ${INSTALL_DIR} が PATH に含まれていません。以下を追加してください:"
        echo ""
        echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
        echo ""
    else
        printf "  実行: ${CYAN}fleet --help${NC}\n"
    fi
}

# メイン
main() {
    info "FleetFlow インストーラー"
    echo ""

    detect_platform
    get_latest_version
    install
}

main
