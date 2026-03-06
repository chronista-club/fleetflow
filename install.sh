#!/bin/bash

# FleetFlow 一括セットアップスクリプト
# 「宣言だけで、開発も本番も」

set -e

# 色定義
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# 表示用関数
info() { printf "${CYAN}%b${NC}\n" "$1"; }
success() { printf "${GREEN}%b${NC}\n" "$1"; }
warn() { printf "${YELLOW}%b${NC}\n" "$1"; }
error() { printf "${RED}%b${NC}\n" "$1"; }

# クリーンアップ
cleanup() {
    rm -f "$TMPDIR/fleetflow-download.tar.gz" "$TMPDIR/fleetflow-download.tar.gz.sha256"
}
trap cleanup EXIT

printf "${BLUE}===========================================${NC}\n"
printf "${BLUE}   FleetFlow Setup Wizard (MCP Enabled)    ${NC}\n"
printf "${BLUE}===========================================${NC}\n"

# 1. 環境チェック
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
    darwin*)  PLATFORM="darwin" ;;
    linux*)   PLATFORM="linux" ;;
    *)        error "エラー: サポートされていないOSです: $OS"; exit 1 ;;
esac

case "$ARCH" in
    x86_64)  BINARY_ARCH="amd64" ;;
    arm64|aarch64) BINARY_ARCH="arm64" ;;
    *)       error "エラー: サポートされていないアーキテクチャです: $ARCH"; exit 1 ;;
esac

INSTALL_DIR="$HOME/.local/bin"
mkdir -p "$INSTALL_DIR"

# 2. FleetFlow インストール
printf "\n"
info "[1/3] FleetFlow 本体をインストールしています..."

# 最新バージョンの取得
LATEST_VERSION=$(curl -sf https://api.github.com/repos/chronista-club/fleetflow/releases/latest | grep tag_name | cut -d'"' -f4) || true

if [ -z "$LATEST_VERSION" ] || [ "$LATEST_VERSION" = "null" ]; then
    error "エラー: 最新バージョンの取得に失敗しました。ネットワーク接続を確認してください。"
    exit 1
fi

printf "  → バージョン: ${GREEN}%s${NC}\n" "$LATEST_VERSION"

# バイナリのダウンロード
URL="https://github.com/chronista-club/fleetflow/releases/download/${LATEST_VERSION}/fleetflow-${PLATFORM}-${BINARY_ARCH}.tar.gz"

if curl --output /dev/null --silent --head --fail "$URL"; then
    info "  → ダウンロード中..."
    TMPFILE="${TMPDIR:-/tmp}/fleetflow-download.tar.gz"
    curl -sfL "$URL" -o "$TMPFILE"

    # SHA256 チェックサム検証
    SHA256_URL="${URL}.sha256"
    SHA256FILE="${TMPFILE}.sha256"
    if curl -sfL "$SHA256_URL" -o "$SHA256FILE" 2>/dev/null; then
        info "  → チェックサム検証中..."
        EXPECTED_HASH=$(awk '{print $1}' "$SHA256FILE")
        if command -v sha256sum &> /dev/null; then
            ACTUAL_HASH=$(sha256sum "$TMPFILE" | awk '{print $1}')
        elif command -v shasum &> /dev/null; then
            ACTUAL_HASH=$(shasum -a 256 "$TMPFILE" | awk '{print $1}')
        else
            warn "  警告: sha256sum/shasum が見つかりません。チェックサム検証をスキップします。"
            ACTUAL_HASH="$EXPECTED_HASH"
        fi
        if [ "$ACTUAL_HASH" != "$EXPECTED_HASH" ]; then
            error "エラー: チェックサムが一致しません。ダウンロードが破損している可能性があります。"
            error "  期待値: $EXPECTED_HASH"
            error "  実際値: $ACTUAL_HASH"
            exit 1
        fi
        success "  ✓ チェックサム検証に成功しました。"
    else
        warn "  警告: .sha256 ファイルが見つかりません。チェックサム検証をスキップします。"
    fi

    tar -xzf "$TMPFILE" -C "$INSTALL_DIR"
    chmod +x "$INSTALL_DIR/fleetflow"
else
    warn "  注意: プリビルドバイナリが見つかりませんでした ($URL)"
    if command -v cargo &> /dev/null; then
        info "  → Rust (cargo) を使用してソースからビルドします..."
        cargo install --git https://github.com/chronista-club/fleetflow --force
    else
        error "  エラー: Rust がインストールされていないため、ビルドできません。"
        error "  https://rustup.rs/ から Rust をインストールするか、バイナリの公開をお待ちください。"
        exit 1
    fi
fi

# 3. パスの確認
if [[ ":$PATH:" != ":$INSTALL_DIR:" ]]; then
    printf "\n"
    warn "重要: $INSTALL_DIR にパスが通っていないようです。"
    printf "  ~/.zshrc や ~/.bashrc に以下を追記してください：\n"
    printf "  ${CYAN}export PATH=\"$HOME/.local/bin:$PATH\"${NC}\n"
fi

# 4. MCP 連携設定
printf "\n"
info "[2/3] AI エージェント（MCP）との連携を設定しています..."

# Gemini CLI 設定
GEMINI_CONFIG_DIR="./.gemini"
if [ -d "$GEMINI_CONFIG_DIR" ]; then
    SETTINGS_FILE="$GEMINI_CONFIG_DIR/settings.json"
    info "  → Gemini CLI を検出しました。設定を更新中..."

    cat > "$SETTINGS_FILE" <<EOF
{
  "mcpServers": {
    "fleetflow": {
      "displayName": "FleetFlow",
      "command": "fleetflow",
      "args": ["mcp"],
      "type": "stdio"
    }
  }
}
EOF
    success "  ✓ $SETTINGS_FILE を作成しました。"
else
    printf "  ℹ カレントディレクトリに .gemini ディレクトリがないため、設定をスキップしました。\n"
fi

# Claude Code 案内
if command -v claude &> /dev/null; then
    info "  → Claude Code を検出しました。以下のコマンドで連携できます："
    printf "    ${CYAN}claude mcp add fleetflow -- fleetfleet mcp${NC}\n"
fi

# 5. 完了
printf "\n"
success "[3/3] セットアップが完了しました！"
printf "${BLUE}-------------------------------------------${NC}\n"
printf "今すぐ始めるには：\n"
printf "  ${CYAN}fleetfleet version${NC}\n"
printf "\n"
printf "AI にプロジェクトを解析させるには：\n"
printf "  ${CYAN}# Gemini CLI や Claude Code 内で${NC}\n"
printf "  「今のプロジェクトの構成を教えて」\n"
printf "${BLUE}-------------------------------------------${NC}\n"
printf "Happy Flowing with ${GREEN}FleetFlow${NC}! (FとFが大文字なのを忘れずに！)\n\n"