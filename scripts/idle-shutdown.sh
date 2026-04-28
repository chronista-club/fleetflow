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
#   GRACE_MIN            shutdown 発行から実 poweroff までの猶予 (default: 2 min、journal で確認 + cancel 余地)

set -euo pipefail

IDLE_THRESHOLD_MIN="${IDLE_THRESHOLD_MIN:-15}"
MIN_UPTIME_MIN="${MIN_UPTIME_MIN:-15}"
GRACE_MIN="${GRACE_MIN:-2}"
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
# 1b. 必須サービスの is-active check
# ─────────────────────────────────────────
# unit file の `After=` は ordering のみで gate にならない。
# 必須サービスが落ちている場合は idle 判定を skip (誤 shutdown 回避)。
for svc in docker fleet-agent; do
  if ! systemctl is-active --quiet "${svc}"; then
    log "skip: ${svc} is not active (idle check requires healthy stack)"
    exit 0
  fi
done

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
if pgrep -f 'cargo|rustc' > /dev/null 2>&1; then
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
# 5. 直近 IDLE_THRESHOLD_MIN min 以内の SSH login がなかったか
# ─────────────────────────────────────────
# `last --time-format=iso` (util-linux 2.37+, Debian 12 標準) で
# ISO 8601 時刻を fixed column 0 に取得、column 位置依存の awk parse を回避。
# epoch 0 fallback による偽陽性 idle 判定 を防ぐため、parse 失敗時は skip 扱い (安全側)。
LAST_LINE=$(last --time-format=iso -n 5 2>/dev/null | grep -v '^$\|^wtmp\|^reboot' | head -1 || true)
if [ -n "${LAST_LINE}" ]; then
  # ISO 8601 timestamp は USER TTY HOST <ISO_TIME> ... の 4 列目
  # 例: "mito pts/0  100.65.119.86 2026-04-28T22:48:11+09:00 ..."
  LAST_TS=$(echo "${LAST_LINE}" | awk '{print $4}')
  if [ -n "${LAST_TS}" ]; then
    LAST_EPOCH=$(date -d "${LAST_TS}" +%s 2>/dev/null || echo "")
    if [ -z "${LAST_EPOCH}" ] || [ "${LAST_EPOCH}" -le 0 ]; then
      # parse 失敗時は安全側 (idle と判定せず skip)、再 fire 待ち
      log "skip: cannot parse last login time '${LAST_TS}' — re-check next fire"
      exit 0
    fi
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
log "idle detected (uptime=${UPTIME_MIN}m, no cargo, no ssh) — shutting down in ${GRACE_MIN}m"
log "  to cancel: shutdown -c"

# +N 分後の shutdown をスケジュール (cancel 可能)。journal に記録残す
shutdown -h "+${GRACE_MIN}" "Idle auto-shutdown ($(date -u +%FT%TZ))"
