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

PROVISION_VERSION="v4"

echo "=== FleetFlow Worker Base Image Provisioning (${PROVISION_VERSION}) ==="
echo "  Date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "  OS:   $(cat /etc/os-release | grep PRETTY_NAME | cut -d= -f2)"
echo ""

# ─────────────────────────────────────────
# 1. 基本パッケージ
# ─────────────────────────────────────────
echo "[1/9] 基本パッケージ..."
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
  protobuf-compiler \
  rsync \
  file \
  > /dev/null 2>&1
echo "  done."

# ─────────────────────────────────────────
# 2. Docker
# ─────────────────────────────────────────
echo "[2/9] Docker..."
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
echo "[3/9] Tailscale..."
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
echo "[4/9] mise..."
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
echo "[5/9] Homebrew..."
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
# 6. Firewall (ufw)
# ─────────────────────────────────────────
echo "[6/9] Firewall (ufw)..."
if command -v ufw &> /dev/null; then
  echo "  skip ($(ufw status | head -1))"
else
  apt-get install -y -qq ufw > /dev/null 2>&1
  # Tailscale (100.x.x.x) からは全許可
  ufw allow in on tailscale0 > /dev/null 2>&1 || true
  # SSH は公開IP からも許可（初回アクセス用）
  ufw allow 22/tcp > /dev/null 2>&1
  # Docker が iptables を直接操作するので、ufw の FORWARD ルールを調整
  # /etc/ufw/after.rules に Docker 用ルールは追加しない（Docker が自前で管理）
  ufw --force enable > /dev/null 2>&1
  echo "  installed & enabled"
fi

# ─────────────────────────────────────────
# 7. 自動セキュリティ更新 (unattended-upgrades)
# ─────────────────────────────────────────
echo "[7/9] Unattended upgrades..."
if dpkg -l | grep -q unattended-upgrades 2>/dev/null; then
  echo "  skip (already installed)"
else
  apt-get install -y -qq unattended-upgrades apt-listchanges > /dev/null 2>&1
  # セキュリティ更新のみ自動適用
  cat > /etc/apt/apt.conf.d/20auto-upgrades << 'AUTO_UPG'
APT::Periodic::Update-Package-Lists "1";
APT::Periodic::Unattended-Upgrade "1";
APT::Periodic::AutocleanInterval "7";
AUTO_UPG
  # 自動再起動は無効（コンテナが動いているため）
  sed -i 's|//Unattended-Upgrade::Automatic-Reboot .*|Unattended-Upgrade::Automatic-Reboot "false";|' \
    /etc/apt/apt.conf.d/50unattended-upgrades 2>/dev/null || true
  echo "  installed (security-only, no auto-reboot)"
fi

# ─────────────────────────────────────────
# 8. ログ収集 (journald 最適化 + Vector 準備)
# ─────────────────────────────────────────
echo "[8/9] ログ設定..."
# journald: ディスク使用量制限
mkdir -p /etc/systemd/journald.conf.d
cat > /etc/systemd/journald.conf.d/fleetflow.conf << 'JOURNAL_CONF'
[Journal]
SystemMaxUse=200M
MaxRetentionSec=7day
Compress=yes
JOURNAL_CONF
systemctl restart systemd-journald 2>/dev/null || true
echo "  journald: max 200MB / 7 days"

# ─────────────────────────────────────────
# 9. FleetFlow 共通設定
# ─────────────────────────────────────────
echo "[9/9] FleetFlow 共通設定..."

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
echo "  ufw:       $(ufw status 2>/dev/null | head -1 || echo 'N/A')"
echo "  Auto-upg:  $(dpkg -l unattended-upgrades 2>/dev/null | grep -q ii && echo 'enabled' || echo 'N/A')"
echo "  Swap:      $(swapon --show --bytes 2>/dev/null | tail -1 | awk '{printf "%.0fMB", $3/1024/1024}' || echo 'none')"
echo "  Deploy:    /opt/fleetflow"
echo ""
echo "次のステップ:"
echo "  1. アーカイブ化: usacloud archive create --source-disk-id <DISK_ID> --name fleet-worker-base --tags fleetflow,worker,base-image,${PROVISION_VERSION}"
echo "  2. worker-init.sh で hostname + Tailscale authkey を設定"
