#!/usr/bin/env bash
# FleetFlow Worker 初期化スクリプト
#
# アーカイブから作成されたサーバーの初回セットアップ。
# さくらクラウドのスタートアップスクリプトとしても、
# SSH 経由の手動実行としても使える。
#
# 使い方:
#   # スタートアップスクリプト経由（変数は note_vars で渡る）
#   # または手動実行:
#   HOSTNAME=fleet-worker-02 TAILSCALE_AUTHKEY=tskey-... bash worker-init.sh
#
# 冪等性:
#   何度実行しても安全。既に設定済みの項目はスキップする。

set -euo pipefail

# 変数（スタートアップスクリプト or 環境変数 or 引数）
WORKER_HOSTNAME="${hostname:-${HOSTNAME:-$(hostname)}}"
WORKER_AUTHKEY="${tailscale_authkey:-${TAILSCALE_AUTHKEY:-}}"
FLEETFLOW_REPO="https://github.com/chronista-club/fleetflow.git"
FLEETFLOW_DIR="/opt/fleetflow"

echo "=== FleetFlow Worker Init ==="
echo "  Hostname: ${WORKER_HOSTNAME}"
echo "  Date:     $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo ""

# ─────────────────────────────────────────
# 0. Self-update: provision スクリプトの最新版を取得・実行
# ─────────────────────────────────────────
echo "[0/4] Self-update..."
mkdir -p "${FLEETFLOW_DIR}"

if [ -d "${FLEETFLOW_DIR}/repo/.git" ]; then
  cd "${FLEETFLOW_DIR}/repo"
  git pull --quiet origin main 2>/dev/null || true
  echo "  repo updated."
else
  git clone --depth 1 --quiet "${FLEETFLOW_REPO}" "${FLEETFLOW_DIR}/repo" 2>/dev/null || true
  echo "  repo cloned."
fi

# provision スクリプトの最新版を実行（冪等なので安全）
if [ -f "${FLEETFLOW_DIR}/repo/scripts/provision-worker-base.sh" ]; then
  echo "  provision-worker-base.sh を実行..."
  bash "${FLEETFLOW_DIR}/repo/scripts/provision-worker-base.sh"
  echo ""
fi

# ツール群のアップデート
echo "  ツール更新..."
# mise self-update + プラグイン更新
if command -v mise &> /dev/null; then
  mise self-update --yes 2>/dev/null || true
  echo "    mise: $(mise --version 2>/dev/null)"
fi
# Homebrew
if [ -f /home/linuxbrew/.linuxbrew/bin/brew ]; then
  /home/linuxbrew/.linuxbrew/bin/brew update --quiet 2>/dev/null || true
  echo "    brew: $(/home/linuxbrew/.linuxbrew/bin/brew --version 2>/dev/null | head -1)"
fi
# FleetFlow CLI
if command -v fleet &> /dev/null; then
  fleet self-update 2>/dev/null || true
  echo "    fleet: $(fleet --version 2>/dev/null)"
fi

# ─────────────────────────────────────────
# 1. hostname 設定
# ─────────────────────────────────────────
echo "[1/4] hostname..."
CURRENT_HOSTNAME=$(hostname)
if [ "${CURRENT_HOSTNAME}" = "${WORKER_HOSTNAME}" ]; then
  echo "  skip (already ${WORKER_HOSTNAME})"
else
  hostnamectl set-hostname "${WORKER_HOSTNAME}"
  # /etc/hosts に追加（冪等: 既にあればスキップ）
  grep -q "${WORKER_HOSTNAME}" /etc/hosts 2>/dev/null || \
    echo "127.0.0.1 ${WORKER_HOSTNAME}" >> /etc/hosts
  echo "  set to ${WORKER_HOSTNAME}"
fi

# ─────────────────────────────────────────
# 2. SSH ホストキー再生成（アーカイブから作成時）
# ─────────────────────────────────────────
echo "[2/4] SSH ホストキー..."
if [ ! -f /etc/ssh/ssh_host_ed25519_key ]; then
  dpkg-reconfigure openssh-server > /dev/null 2>&1
  systemctl restart sshd 2>/dev/null || systemctl restart ssh 2>/dev/null || true
  echo "  regenerated."
else
  echo "  skip (exists)"
fi

# ─────────────────────────────────────────
# 3. Tailscale 接続
# ─────────────────────────────────────────
echo "[3/4] Tailscale..."
if command -v tailscale &> /dev/null; then
  TS_STATUS=$(tailscale status --json 2>/dev/null | jq -r '.BackendState' 2>/dev/null || echo "unknown")
  if [ "${TS_STATUS}" = "Running" ]; then
    echo "  skip (already connected: $(tailscale ip -4 2>/dev/null || echo 'N/A'))"
  elif [ -n "${WORKER_AUTHKEY}" ]; then
    systemctl start tailscaled 2>/dev/null || true
    tailscale up \
      --authkey="${WORKER_AUTHKEY}" \
      --hostname="${WORKER_HOSTNAME}" \
      --ssh
    echo "  connected: $(tailscale ip -4 2>/dev/null || echo 'pending')"
  else
    echo "  skip (no authkey provided, manual: tailscale up --hostname=${WORKER_HOSTNAME} --ssh)"
  fi
else
  echo "  skip (tailscale not installed)"
fi

# ─────────────────────────────────────────
# 4. FleetFlow メタデータ
# ─────────────────────────────────────────
echo "[4/4] メタデータ..."
cat > "${FLEETFLOW_DIR}/.worker-info" << WORKER_INFO
hostname=${WORKER_HOSTNAME}
initialized_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)
provision_version=$(cat ${FLEETFLOW_DIR}/.provision-version 2>/dev/null || echo 'unknown')
tailscale_ip=$(tailscale ip -4 2>/dev/null || echo 'N/A')
public_ip=$(curl -s -4 ifconfig.me 2>/dev/null || echo 'N/A')
WORKER_INFO
echo "  written to ${FLEETFLOW_DIR}/.worker-info"

# ─────────────────────────────────────────
# 完了
# ─────────────────────────────────────────
echo ""
echo "=== Worker Init 完了 ==="
cat "${FLEETFLOW_DIR}/.worker-info"
