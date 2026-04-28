#!/usr/bin/env bash
# install-idle-shutdown.sh ジェネレータ
#
# scripts/idle-shutdown.{sh,service,timer} を inline 埋め込みで合成し、
# scripts/install-idle-shutdown.sh として書き出す。
#
# 必須: idle-shutdown.{sh,service,timer} の編集後にこれを実行して
#       install-idle-shutdown.sh を再生成、3 ファイル間の DRY を保証。
#
# 使い方:
#   bash scripts/build-install-idle-shutdown.sh
#
# CI で sync 確認するには:
#   bash scripts/build-install-idle-shutdown.sh && git diff --exit-code scripts/install-idle-shutdown.sh

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "${ROOT}"

OUT="scripts/install-idle-shutdown.sh"

cat > "${OUT}" <<'EOF_HEAD'
#!/usr/bin/env bash
# Install idle-shutdown timer on a worker host (self-contained)
#
# 使い方:
#   ssh root@<host> 'bash -s' < scripts/install-idle-shutdown.sh
#
# 冪等性: 既存 install を上書き + restart。
# Disable: touch /run/idle-shutdown.disable (一時)
#          systemctl disable --now idle-shutdown.timer (永続)
#
# !!! このファイルは生成物 !!!
# scripts/build-install-idle-shutdown.sh から生成。
# 直接編集せず、source ファイル (idle-shutdown.{sh,service,timer}) を編集してから
# bash scripts/build-install-idle-shutdown.sh で再生成してください。

set -euo pipefail

# Root が必須 (systemd unit / /usr/local/bin への書き込み)
if [ "${EUID:-$(id -u)}" -ne 0 ]; then
  echo "error: must run as root (sudo or root login)" >&2
  exit 1
fi

echo "=== idle-shutdown install ==="
echo "  host: $(hostname)"
echo "  date: $(date -u +%Y-%m-%dT%H:%M:%SZ)"

# 1. /usr/local/bin/idle-shutdown.sh
cat > /usr/local/bin/idle-shutdown.sh <<'SCRIPT_INNER'
EOF_HEAD

cat scripts/idle-shutdown.sh >> "${OUT}"

cat >> "${OUT}" <<'EOF_MID1'
SCRIPT_INNER
chmod +x /usr/local/bin/idle-shutdown.sh
echo "[1/4] /usr/local/bin/idle-shutdown.sh installed"

# 2. /etc/systemd/system/idle-shutdown.service
cat > /etc/systemd/system/idle-shutdown.service <<'SERVICE_INNER'
EOF_MID1

cat scripts/idle-shutdown.service >> "${OUT}"

cat >> "${OUT}" <<'EOF_MID2'
SERVICE_INNER
echo "[2/4] /etc/systemd/system/idle-shutdown.service installed"

# 3. /etc/systemd/system/idle-shutdown.timer
cat > /etc/systemd/system/idle-shutdown.timer <<'TIMER_INNER'
EOF_MID2

cat scripts/idle-shutdown.timer >> "${OUT}"

cat >> "${OUT}" <<'EOF_TAIL'
TIMER_INNER
echo "[3/4] /etc/systemd/system/idle-shutdown.timer installed"

# 4. systemd daemon-reload + enable + start
systemctl daemon-reload
systemctl enable --now idle-shutdown.timer
echo "[4/4] timer enabled & started"

echo ""
echo "=== status ==="
systemctl list-timers idle-shutdown.timer --no-pager 2>&1 | head -5
echo ""
echo "=== controls ==="
echo "  一時 disable: touch /run/idle-shutdown.disable"
echo "  永続 disable: systemctl disable --now idle-shutdown.timer"
echo "  shutdown cancel: shutdown -c"
echo "  log: journalctl -u idle-shutdown -n 20"
EOF_TAIL

chmod +x "${OUT}"
echo "✓ Generated ${OUT} ($(wc -l < ${OUT}) lines)"
