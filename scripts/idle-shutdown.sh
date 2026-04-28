#!/usr/bin/env bash
# Idle auto-shutdown — VM が idle 状態のとき自動で poweroff する
#
# Build Tier B1 worker (build-01 等) で常時稼働コストを削減するため、
# 一定時間 idle なら kernel shutdown を発行する。
#
# 使い方 (systemd timer 経由、手動実行不要):
#   systemctl enable --now idle-shutdown.timer
#
# 一時 disable:
#   touch /run/idle-shutdown.disable
#
# 永続 disable:
#   systemctl disable --now idle-shutdown.timer
#
# 環境変数 (override 可能):
#   IDLE_THRESHOLD_MIN   shutdown までの idle 時間 (default: 15)
#   MIN_UPTIME_MIN       boot 直後の即 shutdown 回避 (default: 15)
#   GRACE_SECONDS        shutdown まで wait 時間 (default: 60、journal で確認余地)

set -euo pipefail

IDLE_THRESHOLD_MIN="${IDLE_THRESHOLD_MIN:-15}"
MIN_UPTIME_MIN="${MIN_UPTIME_MIN:-15}"
GRACE_SECONDS="${GRACE_SECONDS:-60}"
DISABLE_FLAG="/run/idle-shutdown.disable"

log() {
  echo "[idle-shutdown] $(date -u +%Y-%m-%dT%H:%M:%SZ) $*"
}

# ─────────────────────────────────────────
# 1. Disable flag check (manual escape hatch)
# ─────────────────────────────────────────
if [ -f "${DISABLE_FLAG}" ]; then
  log "skip: ${DISABLE_FLAG} exists (manually disabled)"
  exit 0
fi

# ─────────────────────────────────────────
# 2. Boot 直後の即 shutdown を回避
# ─────────────────────────────────────────
UPTIME_MIN=$(awk '{printf "%d", $1/60}' /proc/uptime)
if [ "${UPTIME_MIN}" -lt "${MIN_UPTIME_MIN}" ]; then
  log "skip: uptime ${UPTIME_MIN}m < ${MIN_UPTIME_MIN}m (fresh boot)"
  exit 0
fi

# ─────────────────────────────────────────
# 3. Active な build process がないか
# ─────────────────────────────────────────
if pgrep -af 'cargo|rustc' > /dev/null 2>&1; then
  log "skip: active cargo/rustc process running"
  exit 0
fi

# ─────────────────────────────────────────
# 4. Active SSH session がないか
# ─────────────────────────────────────────
SSH_USERS=$(who | wc -l)
if [ "${SSH_USERS}" -gt 0 ]; then
  log "skip: ${SSH_USERS} active session(s) (who)"
  exit 0
fi

# ─────────────────────────────────────────
# 5. 直近 15 min 以内の SSH login がなかったか (last 経由)
# ─────────────────────────────────────────
# last -F は frozen format (固定幅 column)、最新 1 件
# "still logged in" あるいは現在 connection なら exit (既に上で who 確認済だが二重防御)
LAST_LINE=$(last -F -n 5 | grep -v '^$\|^wtmp\|^reboot' | head -1 || true)
if [ -n "${LAST_LINE}" ]; then
  # epoch from last login or last logout
  LAST_TS=$(echo "${LAST_LINE}" | awk '{print $5, $6, $7, $8}')
  if [ -n "${LAST_TS}" ]; then
    LAST_EPOCH=$(date -d "${LAST_TS}" +%s 2>/dev/null || echo "0")
    NOW_EPOCH=$(date +%s)
    AGE_MIN=$(( (NOW_EPOCH - LAST_EPOCH) / 60 ))
    if [ "${AGE_MIN}" -lt "${IDLE_THRESHOLD_MIN}" ]; then
      log "skip: last activity ${AGE_MIN}m ago < ${IDLE_THRESHOLD_MIN}m"
      exit 0
    fi
  fi
fi

# ─────────────────────────────────────────
# 6. 全条件満たした → shutdown
# ─────────────────────────────────────────
log "idle detected (uptime=${UPTIME_MIN}m, no cargo, no ssh) — shutting down in ${GRACE_SECONDS}s"
log "  to cancel: shutdown -c"

# +1 (1 分後) shutdown をスケジュール、journal に記録残す
shutdown -h "+$((GRACE_SECONDS / 60 + 1))" "Idle auto-shutdown ($(date -u +%FT%TZ))" 2>&1 | tee -a /var/log/idle-shutdown.log
