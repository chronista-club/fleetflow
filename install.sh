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

echo -e "${BLUE}===========================================${NC}"
echo -e "${BLUE}   FleetFlow Setup Wizard (MCP Enabled)    ${NC}"
echo -e "${BLUE}===========================================${NC}"

# 1. 環境チェック
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
    darwin*)  PLATFORM="darwin" ;;
    linux*)   PLATFORM="linux" ;;
    *)        echo -e "${RED}エラー: サポートされていないOSです: $OS${NC}"; exit 1 ;;
esac
case "$ARCH" in
    x86_64)  BINARY_ARCH=\"amd64\" ;; 
    arm64|aarch64) BINARY_ARCH=\"arm64\" ;; 
    *)       echo -e "${RED}エラー: サポートされていないアーキテクチャです: $ARCH${NC}"; exit 1 ;; 
esac

INSTALL_DIR=\"$HOME/.local/bin\"
mkdir -p \"$INSTALL_DIR\"

# 2. FleetFlow インストール
echo -e "\n${CYAN}[1/3] FleetFlow 本体をインストールしています...${NC}"

# 最新バージョンの取得
LATEST_VERSION=$(curl -s https://api.github.com/repos/chronista-club/fleetflow/releases/latest | grep tag_name | cut -d'"' -f4 || echo "v0.2.14")

if [ -z "$LATEST_VERSION" ]; then
    LATEST_VERSION="v0.2.14"
fi

echo -e "  → バージョン: ${GREEN}$LATEST_VERSION${NC}"

# バイナリのダウンロード（GitHub Releasesにある前提。なければビルド案内）
URL=\"https://github.com/chronista-club/fleetflow/releases/download/${LATEST_VERSION}/fleetflow-${PLATFORM}-${BINARY_ARCH}.tar.gz\"

# 注: まだバイナリが公開されていない場合は、ローカルビルドを案内する形にフォールバック
if curl --output /dev/null --silent --head --fail "$URL"; then
    echo -e "  → ダウンロード中..."
    curl -L "$URL" | tar -xz -C "$INSTALL_DIR"
    chmod +x "$INSTALL_DIR/fleetflow"
else
    echo -e "  ${YELLOW}注意: プリビルドバイナリが見つかりませんでした。${NC}"
    if command -v cargo &> /dev/null; then
        echo -e "  → Rust (cargo) を使用してソースからビルドします..."
        cargo install --git https://github.com/chronista-club/fleetflow --force
    else
        echo -e "  ${RED}エラー: Rust がインストールされていないため、ビルドできません。${NC}"
        echo -e "  https://rustup.rs/ から Rust をインストールするか、バイナリの公開をお待ちください。"
        exit 1
    fi
fi

# 3. パスの確認
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo -e "\n${YELLOW}重要: $INSTALL_DIR にパスが通っていないようです。${NC}"
    echo -e "~/.zshrc や ~/.bashrc に以下を追記してください："
    echo -e "  ${CYAN}export PATH=\"\\$HOME/.local/bin:\\$PATH\"${NC}"
fi

# 4. MCP 連携設定
echo -e "\n${CYAN}[2/3] AI エージェント（MCP）との連携を設定しています...${NC}"

# Gemini CLI 設定
GEMINI_CONFIG_DIR=\".gemini\"
if [ -d "$GEMINI_CONFIG_DIR" ]; then
    SETTINGS_FILE="$GEMINI_CONFIG_DIR/settings.json"
    echo -e "  → Gemini CLI を検出しました。設定を更新中..."
    
    # 簡易的な JSON 生成/更新
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
    echo -e "  ${GREEN}✓ $SETTINGS_FILE を作成しました。${NC}"
else
    echo -e "  ℹ カレントディレクトリに .gemini ディレクトリがないため、設定をスキップしました。"
fi

# Claude Code 案内
if command -v claude &> /dev/null; then
    echo -e "  → Claude Code を検出しました。以下のコマンドで連携できます："
    echo -e "    ${CYAN}claude mcp add fleetflow -- fleetflow mcp${NC}"
fi

# 5. 完了
echo -e "\n${GREEN}[3/3] セットアップが完了しました！${NC}"
echo -e "${BLUE}-------------------------------------------${NC}"
echo -e "今すぐ始めるには："
echo -e "  ${CYAN}fleetflow --version${NC}"
echo -e ""
echo -e "AI にプロジェクトを解析させるには："
echo -e "  ${CYAN}# Gemini CLI や Claude Code 内で${NC}"
echo -e "  「今のプロジェクトの構成を教えて」"
${BLUE}-------------------------------------------${NC}
echo -e "Happy Flowing with ${GREEN}FleetFlow${NC}! (FとFが大文字なのを忘れずに！)\n"
