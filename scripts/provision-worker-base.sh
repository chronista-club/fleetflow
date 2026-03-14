#!/usr/bin/env bash
# FleetFlow Worker ベースイメージ プロビジョニングスクリプト
#
# 素の Debian サーバーを FleetFlow Worker として使える状態にする。
# 実行後にさくらクラウドのアーカイブとして保存すれば、
# 以降のサーバー作成はこのアーカイブから一発で起動できる。
#
# 使い方:
#   ssh root@<server-ip> 'bash -s' < scripts/provision-worker-base.sh
#
# 前提:
#   - Debian 12 (bookworm)
#   - root ユーザーで実行
#   - インターネット接続あり
#
# 冪等性:
#   このスクリプトは冪等（何度実行しても同じ結果）に設計。
#   各ステップは「既にインストール済みならスキップ」する。

set -euo pipefail

PROVISION_VERSION="v2"

echo "=== FleetFlow Worker Base Image Provisioning (${PROVISION_VERSION}) ==="
echo "  Date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "  OS:   $(cat /etc/os-release | grep PRETTY_NAME | cut -d= -f2)"
echo ""

# ─────────────────────────────────────────
# 1. 基本パッケージ
# ─────────────────────────────────────────
echo "[1/6] 基本パッケージ..."
apt-get update -qq
apt-get install -y -qq \
  curl \
  ca-certificates \
  gnupg \
  lsb-release \
  git \
  jq \
  unzip \
  htop \
  build-essential \
  > /dev/null 2>&1
echo "  done."

# ─────────────────────────────────────────
# 2. Docker
# ─────────────────────────────────────────
echo "[2/6] Docker..."
if command -v docker &> /dev/null; then
  echo "  skip ($(docker --version))"
else
  curl -fsSL https://get.docker.com | sh > /dev/null 2>&1
  systemctl enable docker
  systemctl start docker
  echo "  installed: $(docker --version)"
fi

# ─────────────────────────────────────────
# 3. Tailscale
# ─────────────────────────────────────────
echo "[3/6] Tailscale..."
if command -v tailscale &> /dev/null; then
  echo "  skip ($(tailscale version | head -1))"
else
  curl -fsSL https://tailscale.com/install.sh | sh > /dev/null 2>&1
  systemctl enable tailscaled
  echo "  installed: $(tailscale version | head -1)"
fi

# ─────────────────────────────────────────
# 4. mise（ツールバージョン管理）
# ─────────────────────────────────────────
echo "[4/6] mise..."
if command -v mise &> /dev/null || [ -f /usr/local/bin/mise ]; then
  echo "  skip ($(mise --version 2>/dev/null || echo 'installed'))"
else
  curl -fsSL https://mise.run | sh > /dev/null 2>&1
  # mise を /usr/local/bin にシンボリックリンク（全ユーザーから使えるように）
  if [ -f /root/.local/bin/mise ] && [ ! -f /usr/local/bin/mise ]; then
    ln -s /root/.local/bin/mise /usr/local/bin/mise
  fi
  # bashrc に activate 追加
  grep -q 'mise activate' /etc/skel/.bashrc 2>/dev/null || \
    echo 'eval "$(mise activate bash)"' >> /etc/skel/.bashrc
  grep -q 'mise activate' /root/.bashrc 2>/dev/null || \
    echo 'eval "$(mise activate bash)"' >> /root/.bashrc
  echo "  installed: $(mise --version 2>/dev/null || echo 'done')"
fi

# ─────────────────────────────────────────
# 5. Homebrew (Linuxbrew)
# ─────────────────────────────────────────
echo "[5/6] Homebrew..."
if command -v brew &> /dev/null || [ -f /home/linuxbrew/.linuxbrew/bin/brew ]; then
  echo "  skip ($(brew --version 2>/dev/null | head -1 || echo 'installed'))"
else
  # Homebrew は非 root で実行する必要がある
  # linuxbrew 用ユーザーを作成
  if ! id "linuxbrew" &>/dev/null; then
    useradd -m -s /bin/bash linuxbrew 2>/dev/null || true
  fi
  # non-interactive インストール
  NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)" > /dev/null 2>&1 || true
  # パスを通す
  if [ -f /home/linuxbrew/.linuxbrew/bin/brew ]; then
    grep -q 'linuxbrew' /etc/environment 2>/dev/null || \
      echo 'PATH="/home/linuxbrew/.linuxbrew/bin:/home/linuxbrew/.linuxbrew/sbin:$PATH"' >> /etc/environment
    grep -q 'linuxbrew' /root/.bashrc 2>/dev/null || \
      echo 'eval "$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)"' >> /root/.bashrc
    echo "  installed: $(/home/linuxbrew/.linuxbrew/bin/brew --version | head -1)"
  else
    echo "  warning: Homebrew インストールに失敗（続行）"
  fi
fi

# ─────────────────────────────────────────
# 6. FleetFlow 共通設定
# ─────────────────────────────────────────
echo "[6/6] FleetFlow 共通設定..."

# デプロイ用ディレクトリ
mkdir -p /opt/fleetflow

# Docker log rotation（ディスク節約）
cat > /etc/docker/daemon.json << 'DOCKER_CONF'
{
  "log-driver": "json-file",
  "log-opts": {
    "max-size": "10m",
    "max-file": "3"
  }
}
DOCKER_CONF
systemctl restart docker 2>/dev/null || true

# swap（冪等: 既にあればスキップ）
if [ ! -f /swapfile ]; then
  fallocate -l 1G /swapfile
  chmod 600 /swapfile
  mkswap /swapfile > /dev/null
  swapon /swapfile
  grep -q '/swapfile' /etc/fstab || echo '/swapfile none swap sw 0 0' >> /etc/fstab
  echo "  swap 1GB 追加"
fi

# プロビジョニングバージョンを記録
echo "${PROVISION_VERSION}" > /opt/fleetflow/.provision-version

echo "  done."

# ─────────────────────────────────────────
# 完了
# ─────────────────────────────────────────
echo ""
echo "=== プロビジョニング完了 (${PROVISION_VERSION}) ==="
echo "  Docker:    $(docker --version 2>/dev/null || echo 'N/A')"
echo "  Tailscale: $(tailscale version 2>/dev/null | head -1 || echo 'N/A')"
echo "  mise:      $(mise --version 2>/dev/null || echo 'N/A')"
echo "  Homebrew:  $(/home/linuxbrew/.linuxbrew/bin/brew --version 2>/dev/null | head -1 || echo 'N/A')"
echo "  Swap:      $(swapon --show --bytes 2>/dev/null | tail -1 | awk '{printf "%.0fMB", $3/1024/1024}' || echo 'none')"
echo "  Deploy:    /opt/fleetflow"
echo ""
echo "次のステップ:"
echo "  1. アーカイブ化: usacloud archive create --source-disk-id <DISK_ID> --name fleet-worker-base --tags fleetflow,worker,base-image,${PROVISION_VERSION}"
echo "  2. worker-init.sh で hostname + Tailscale authkey を設定"
