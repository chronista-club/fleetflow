#!/usr/bin/env bash
# FleetFlow Build Worker Spawn Script
#
# さくらクラウドに Build Tier worker を 1 コマンドで spawn し、
# OS provision + Tailscale connect + idle-shutdown timer まで連鎖実行する。
#
# 使い方:
#   ./spawn-build-worker.sh --name build-03
#   ./spawn-build-worker.sh --name build-04 --cpu 8 --memory 32 --disk-gb 100
#   ./spawn-build-worker.sh --name build-05 --no-tailscale --no-idle-shutdown
#   ./spawn-build-worker.sh --name build-06 --fleet-agent-binary path/to/fleet-agent
#
# 前提:
#   - usacloud CLI 認証済 (env var or ~/.config/usacloud/、 `server list` で実 auth 検証)
#     注: usacloud v1.20.1 の `config show` は nil panic で死ぬので使わない
#   - 1Password CLI (`op`) で OP_SERVICE_ACCOUNT_TOKEN export 済
#   - Mac から実行 (~/.ssh/config 追記、~/.ssh/id_ed25519 利用)
#   - Tailscale authkey が ${TS_AUTHKEY_OP} に保存されている (default の固定 path)
#   - fleet-agent (linux-gnu) binary が手元にある:
#       cargo zigbuild --target x86_64-unknown-linux-gnu.2.36 -p fleet-agent --release
#     auto-detect 場所 = ${REPO}/target/x86_64-unknown-linux-gnu*/release/fleet-agent
#     不要なら --no-fleet-agent (ただし idle-shutdown が auto-trigger しなくなる)
#
# 実行フロー:
#   0. Pre-flight: usacloud / op / 引数検証 + fleet-agent binary 検出
#   1. Root password 自動生成 → 1Password に保存 (item name = "<hostname> root (Sakura Cloud)")
#   2. usacloud server create (debian-bookworm + SSD + SSH key only)
#   3. SSH reachable まで poll
#   4. provision-worker-base.sh を SSH 経由で実行
#   5. worker-init.sh (hostname + Tailscale connect)
#   5.5. scp fleet-agent binary + provision-fleet-agent.sh (CP に slug=<hostname> で登録)
#   6. install-idle-shutdown.sh (10 min idle で auto-poweroff、 --no-idle-shutdown で skip)
#   7. ~/.ssh/config に Host エントリ idempotent 追記
#   8. サマリ表示 (公開IP / Tailscale IP / 月コスト目安 / 削除コマンド)
#
# 失敗時の手動 cleanup:
#   usacloud server delete -y --with-disks --zone <zone> <SERVER_ID>
#   op item delete <password-item-id> --vault FleetFlowVault
#   ~/.ssh/config から Host <hostname> ブロック削除
#   (CP 側の server レコードは別途 `fleet cp server delete --slug <slug>`)

set -euo pipefail

# ─────────────────────────────────────────
# Config (環境変数で override 可能)
# ─────────────────────────────────────────

# 1Password 内の Tailscale authkey 保存先 (固定 item ID、 reusable key 推奨)
# user が事前に reusable authkey を発行 → credential field に保存しておく:
#   op item edit aeezqqcjops36p5jszo2agfc3q --vault FleetFlowVault \
#     credential='tskey-auth-...' \
#     --title 'Tailscale reusable authkey (build workers)'
TS_AUTHKEY_OP="${TS_AUTHKEY_OP:-op://FleetFlowVault/aeezqqcjops36p5jszo2agfc3q/credential}"

# さくらクラウドに登録済の SSH 公開鍵 ID (~/.ssh/id_ed25519.pub と紐付き)
SAKURA_SSH_KEY_ID="${SAKURA_SSH_KEY_ID:-113702829263}"

# Sakura shared archive ID (Debian 12 cloudimg)
SAKURA_ARCHIVE_ID="${SAKURA_ARCHIVE_ID:-113601947266}"

# 1Password の保存先 vault
OP_VAULT="${OP_VAULT:-FleetFlowVault}"

# fleet-agent が接続する CP の host:port (Phase 5.5)
FLEET_CP_ENDPOINT="${FLEET_CP_ENDPOINT:-cp.fleetstage.cloud:4510}"

# Default spec (build-02 benchmark で確定: 8C/32G で -j 6 が sweet spot)
DEFAULT_CPU=8
DEFAULT_MEMORY=32
DEFAULT_DISK_GB=100
DEFAULT_ZONE=tk1a
DEFAULT_IDLE_MIN=10

# Tag (cost 集計時に identify しやすく)
SAKURA_TAGS="${SAKURA_TAGS:-fleetflow,build-worker}"

# ─────────────────────────────────────────
# CLI 引数 parse
# ─────────────────────────────────────────

usage() {
  cat <<USAGE
Usage: $(basename "$0") --name <hostname> [options]

Required:
  --name <hostname>           例: build-03

Options:
  --cpu N                       default: ${DEFAULT_CPU}
  --memory G                    default: ${DEFAULT_MEMORY} (GB)
  --disk-gb N                   default: ${DEFAULT_DISK_GB}
  --zone <zone>                 default: ${DEFAULT_ZONE}
  --idle-min N                  default: ${DEFAULT_IDLE_MIN}
  --fleet-agent-binary <path>   default: auto-detect (\${REPO}/target/x86_64-unknown-linux-gnu*/release/fleet-agent)
  --fleet-cp-endpoint <h:p>     default: ${FLEET_CP_ENDPOINT}
  --fleet-slug <slug>           default: <hostname>
  --no-tailscale                Tailscale connect を skip
  --no-fleet-agent              fleet-agent install を skip (idle-shutdown auto-trigger 無効になる)
  --no-idle-shutdown            idle-shutdown timer install を skip
  --dry-run                     server create までで止める (provision skip)
  --help                        この help を表示

Examples:
  $(basename "$0") --name build-03
  $(basename "$0") --name build-04 --cpu 4 --memory 16
USAGE
}

NAME=""
CPU="${DEFAULT_CPU}"
MEMORY="${DEFAULT_MEMORY}"
DISK_GB="${DEFAULT_DISK_GB}"
ZONE="${DEFAULT_ZONE}"
IDLE_MIN="${DEFAULT_IDLE_MIN}"
FLEET_AGENT_BINARY=""
FLEET_SLUG=""
USE_TAILSCALE=1
USE_FLEET_AGENT=1
USE_IDLE_SHUTDOWN=1
DRY_RUN=0

while [ $# -gt 0 ]; do
  case "$1" in
    --name)                 NAME="$2";    shift 2 ;;
    --cpu)                  CPU="$2";     shift 2 ;;
    --memory)               MEMORY="$2";  shift 2 ;;
    --disk-gb)              DISK_GB="$2"; shift 2 ;;
    --zone)                 ZONE="$2";    shift 2 ;;
    --idle-min)             IDLE_MIN="$2"; shift 2 ;;
    --fleet-agent-binary)   FLEET_AGENT_BINARY="$2"; shift 2 ;;
    --fleet-cp-endpoint)    FLEET_CP_ENDPOINT="$2"; shift 2 ;;
    --fleet-slug)           FLEET_SLUG="$2"; shift 2 ;;
    --no-tailscale)         USE_TAILSCALE=0; shift ;;
    --no-fleet-agent)       USE_FLEET_AGENT=0; shift ;;
    --no-idle-shutdown)     USE_IDLE_SHUTDOWN=0; shift ;;
    --dry-run)              DRY_RUN=1; shift ;;
    --help|-h)              usage; exit 0 ;;
    *) echo "error: unknown arg: $1" >&2; usage >&2; exit 2 ;;
  esac
done

if [ -z "$NAME" ]; then
  echo "error: --name is required" >&2
  usage >&2
  exit 2
fi
FLEET_SLUG="${FLEET_SLUG:-${NAME}}"

# scripts/ ディレクトリ (このスクリプトと同じ場所に各 helper script がある前提)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# ─────────────────────────────────────────
# Phase 0. Pre-flight
# ─────────────────────────────────────────

echo "=== Phase 0: pre-flight ==="

# 1Password
if ! op whoami > /dev/null 2>&1; then
  echo "error: 1Password CLI not authenticated. set OP_SERVICE_ACCOUNT_TOKEN" >&2
  exit 1
fi

# helper script 群が同階層にあるか
HELPERS=(provision-worker-base.sh worker-init.sh install-idle-shutdown.sh)
if [ "${USE_FLEET_AGENT}" -eq 1 ]; then
  HELPERS+=(provision-fleet-agent.sh)
fi
for s in "${HELPERS[@]}"; do
  if [ ! -f "${SCRIPT_DIR}/${s}" ]; then
    echo "error: helper script missing: ${SCRIPT_DIR}/${s}" >&2
    exit 1
  fi
done

# fleet-agent binary 検出 (USE_FLEET_AGENT 有効時のみ)
if [ "${USE_FLEET_AGENT}" -eq 1 ]; then
  if [ -z "${FLEET_AGENT_BINARY}" ]; then
    REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
    for cand in \
      "${REPO_ROOT}/target/x86_64-unknown-linux-gnu.2.36/release/fleet-agent" \
      "${REPO_ROOT}/target/x86_64-unknown-linux-gnu/release/fleet-agent"; do
      if [ -f "${cand}" ]; then
        FLEET_AGENT_BINARY="${cand}"
        break
      fi
    done
  fi
  if [ ! -f "${FLEET_AGENT_BINARY}" ]; then
    echo "error: fleet-agent binary が見つからない: ${FLEET_AGENT_BINARY:-<auto-detect 失敗>}" >&2
    echo "  zigbuild で linux-gnu binary を作成:" >&2
    echo "    cd ${REPO_ROOT:-~/repos/fleetflow}" >&2
    echo "    cargo zigbuild --target x86_64-unknown-linux-gnu.2.36 -p fleet-agent --release" >&2
    echo "  または --fleet-agent-binary <path> を明示、 --no-fleet-agent で skip" >&2
    exit 1
  fi
fi

# usacloud auth + zone reachable + 同名 server dedup を 1 回の呼び出しで兼ねる
# (v1.20.1 の `config show` は nil panic で使えない、 stdout=JSON / stderr=NOTICE で分離)
LIST_OUT=$(usacloud server list --zone "${ZONE}" --output-type json 2>/dev/null || true)
if ! echo "${LIST_OUT}" | jq -e 'type == "array"' > /dev/null 2>&1; then
  echo "error: usacloud auth/config 失敗 — zone=${ZONE}" >&2
  echo "  確認: ls ~/.config/usacloud/ もしくは env | grep SAKURA" >&2
  exit 1
fi
if echo "${LIST_OUT}" | jq -e --arg n "$NAME" '.[] | select(.Name == $n)' > /dev/null 2>&1; then
  EXISTING_ID=$(echo "${LIST_OUT}" | jq -r --arg n "$NAME" '.[] | select(.Name == $n) | .ID' | head -1)
  echo "error: server '${NAME}' already exists in zone ${ZONE} (ID=${EXISTING_ID})" >&2
  echo "  cleanup: usacloud server delete -y --with-disks --zone ${ZONE} ${EXISTING_ID}" >&2
  exit 1
fi
if grep -q "^Host ${NAME}\$" "${HOME}/.ssh/config" 2>/dev/null; then
  echo "warning: ~/.ssh/config に既に 'Host ${NAME}' がある (後で更新する)" >&2
fi

# Tailscale authkey の取得確認 (--no-tailscale でない限り)
if [ "${USE_TAILSCALE}" -eq 1 ]; then
  if ! op read "${TS_AUTHKEY_OP}" > /dev/null 2>&1; then
    echo "error: Tailscale authkey が読めない: ${TS_AUTHKEY_OP}" >&2
    echo "  reusable key を Tailscale admin で発行 → 1Password に保存:" >&2
    echo "    op item edit aeezqqcjops36p5jszo2agfc3q --vault ${OP_VAULT} \\" >&2
    echo "      credential='tskey-auth-...' \\" >&2
    echo "      --title 'Tailscale reusable authkey (build workers)'" >&2
    echo "  または --no-tailscale で Tailscale を skip して spawn 後に手動 'tailscale up --ssh'" >&2
    exit 1
  fi
fi

echo "  hostname:           ${NAME}"
echo "  spec:               ${CPU}C / ${MEMORY}G / ${DISK_GB}GB SSD (${ZONE})"
echo "  tailscale:          $([ ${USE_TAILSCALE} -eq 1 ] && echo "yes" || echo "skip")"
echo "  fleet-agent:        $([ ${USE_FLEET_AGENT} -eq 1 ] && echo "yes (slug=${FLEET_SLUG}, endpoint=${FLEET_CP_ENDPOINT})" || echo "skip — idle-shutdown auto-trigger 無効")"
echo "  idle-shutdown:      $([ ${USE_IDLE_SHUTDOWN} -eq 1 ] && echo "yes (${IDLE_MIN} min)" || echo "skip")"
echo "  dry-run:            $([ ${DRY_RUN} -eq 1 ] && echo "yes" || echo "no")"
if [ "${USE_FLEET_AGENT}" -eq 1 ]; then
  echo "  fleet-agent bin:    ${FLEET_AGENT_BINARY}"
fi
echo ""

# ─────────────────────────────────────────
# Phase 1. Root password 生成 → 1Password に保存
# ─────────────────────────────────────────

echo "=== Phase 1: root password generate + save to 1Password ==="

ROOT_PW=$(openssl rand -base64 24 | tr -d '/+=' | head -c 24)
PW_ITEM_TITLE="${NAME} root (Sakura Cloud)"

PW_ITEM_ID=$(op item create \
  --category=password \
  --vault="${OP_VAULT}" \
  --title="${PW_ITEM_TITLE}" \
  password="${ROOT_PW}" \
  --tags=fleetflow,build-worker \
  --format=json 2>/dev/null | jq -r '.id')

if [ -z "${PW_ITEM_ID}" ] || [ "${PW_ITEM_ID}" = "null" ]; then
  echo "error: 1Password item create failed" >&2
  exit 1
fi
echo "  saved: op://${OP_VAULT}/${PW_ITEM_ID}/password ('${PW_ITEM_TITLE}')"
echo ""

# ─────────────────────────────────────────
# Phase 2. usacloud server create
# ─────────────────────────────────────────

echo "=== Phase 2: usacloud server create ==="

# stdout=JSON / stderr=NOTICE 分離。 set -e と組み合わせるため if-! で rc を捕まえる
CREATE_ERR_LOG="$(mktemp -t spawn-build-worker.create-err.XXXXXX)"
trap 'rm -f "${CREATE_ERR_LOG}"' EXIT
if ! CREATE_OUT=$(usacloud server create -y \
    --zone "${ZONE}" \
    --name "${NAME}" \
    --cpu "${CPU}" \
    --memory "${MEMORY}" \
    --tags "${SAKURA_TAGS}" \
    --boot-after-create \
    --interface-driver virtio \
    --network-interface-upstream shared \
    --disk-edit-password "${ROOT_PW}" \
    --disk-edit-ssh-key-ids "${SAKURA_SSH_KEY_ID}" \
    --disk-os-type debian \
    --disk-disk-plan ssd \
    --disk-source-archive-id "${SAKURA_ARCHIVE_ID}" \
    --disk-size "${DISK_GB}" \
    --disk-edit-disable-pw-auth \
    --output-type json 2> "${CREATE_ERR_LOG}"); then
  echo "error: server create failed (usacloud rc=$?):" >&2
  cat "${CREATE_ERR_LOG}" >&2 || true
  echo "  cleanup 1Password: op item delete ${PW_ITEM_ID} --vault ${OP_VAULT}" >&2
  exit 1
fi

if ! echo "${CREATE_OUT}" | jq -e '.[0].ID' > /dev/null 2>&1; then
  echo "error: server create stdout は JSON array でない:" >&2
  echo "${CREATE_OUT}" >&2
  echo "stderr:" >&2
  cat "${CREATE_ERR_LOG}" >&2 || true
  echo "  cleanup 1Password: op item delete ${PW_ITEM_ID} --vault ${OP_VAULT}" >&2
  exit 1
fi

SERVER_ID=$(echo "${CREATE_OUT}" | jq -r '.[0].ID')
PUBLIC_IP=$(echo "${CREATE_OUT}" | jq -r '.[0].Interfaces[0].IPAddress // .[0].Interfaces[0].UserIPAddress // empty')

if [ -z "${PUBLIC_IP}" ]; then
  sleep 3
  PUBLIC_IP=$(usacloud server read --zone "${ZONE}" --output-type json "${SERVER_ID}" 2>/dev/null \
    | jq -r '.[0].Interfaces[0].IPAddress // empty')
fi

if [ -z "${PUBLIC_IP}" ]; then
  echo "error: 公開 IP の取得に失敗 (server ID=${SERVER_ID})" >&2
  exit 1
fi

echo "  server_id:  ${SERVER_ID}"
echo "  public_ip:  ${PUBLIC_IP}"
echo ""

if [ "${DRY_RUN}" -eq 1 ]; then
  echo "=== DRY-RUN 完了 ==="
  echo "  provision/init/idle-shutdown は skip"
  echo "  ssh root@${PUBLIC_IP} で手動接続可"
  exit 0
fi

# ─────────────────────────────────────────
# Phase 3. SSH reachable 待ち (max 5 min)
# ─────────────────────────────────────────

echo "=== Phase 3: wait SSH ready ==="

SSH_OPTS="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 -o BatchMode=yes"
TRIES=30
for i in $(seq 1 ${TRIES}); do
  if ssh ${SSH_OPTS} "root@${PUBLIC_IP}" 'true' > /dev/null 2>&1; then
    echo "  SSH ready (try ${i}/${TRIES})"
    break
  fi
  if [ "${i}" -eq "${TRIES}" ]; then
    echo "error: SSH not reachable after ${TRIES} tries" >&2
    exit 1
  fi
  sleep 10
done
echo ""

# ─────────────────────────────────────────
# Phase 4. provision-worker-base.sh
# ─────────────────────────────────────────

echo "=== Phase 4: provision-worker-base.sh ==="
ssh ${SSH_OPTS} "root@${PUBLIC_IP}" 'bash -s' < "${SCRIPT_DIR}/provision-worker-base.sh"
echo ""

# ─────────────────────────────────────────
# Phase 5. worker-init.sh (hostname + Tailscale)
# ─────────────────────────────────────────

echo "=== Phase 5: worker-init.sh (hostname + Tailscale) ==="

if [ "${USE_TAILSCALE}" -eq 1 ]; then
  TS_AUTHKEY=$(op read "${TS_AUTHKEY_OP}")
else
  TS_AUTHKEY=""
fi

ssh ${SSH_OPTS} "root@${PUBLIC_IP}" \
  "HOSTNAME='${NAME}' TAILSCALE_AUTHKEY='${TS_AUTHKEY}' bash -s" \
  < "${SCRIPT_DIR}/worker-init.sh"

TS_IP=""
if [ "${USE_TAILSCALE}" -eq 1 ]; then
  TS_IP=$(ssh ${SSH_OPTS} "root@${PUBLIC_IP}" 'tailscale ip -4 2>/dev/null || true' | head -1)
fi
echo ""

# ─────────────────────────────────────────
# Phase 5.5. fleet-agent install (idle-shutdown 必須前提)
# ─────────────────────────────────────────
# idle-shutdown.sh は `for svc in docker fleet-agent` で fleet-agent active を要求する。
# fleet-agent が未 install だと idle-shutdown は永続「skip」状態で auto-trigger しない。
# --no-fleet-agent で skip 可、 ただし idle-shutdown も実質無効化される点に注意。

if [ "${USE_FLEET_AGENT}" -eq 1 ]; then
  echo "=== Phase 5.5: scp fleet-agent + provision-fleet-agent.sh (slug=${FLEET_SLUG}) ==="
  # scp: SSH_OPTS は `-o k=v ...` のスペース区切り、 zsh で word-split されないので
  # ここでは個別 -o を直書き (scp は array 展開不可)
  scp -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=10 \
    "${FLEET_AGENT_BINARY}" "root@${PUBLIC_IP}:/usr/local/bin/fleet-agent"
  ssh ${SSH_OPTS} "root@${PUBLIC_IP}" 'chmod +x /usr/local/bin/fleet-agent'
  ssh ${SSH_OPTS} "root@${PUBLIC_IP}" \
    "bash -s -- --slug '${FLEET_SLUG}' --endpoint '${FLEET_CP_ENDPOINT}'" \
    < "${SCRIPT_DIR}/provision-fleet-agent.sh"
  echo ""
else
  echo "=== Phase 5.5: fleet-agent SKIP (--no-fleet-agent) ==="
  echo "  warning: idle-shutdown は fleet-agent active を要求するため、 auto-shutdown が動かない" >&2
  echo ""
fi

# ─────────────────────────────────────────
# Phase 6. install-idle-shutdown.sh
# ─────────────────────────────────────────

if [ "${USE_IDLE_SHUTDOWN}" -eq 1 ]; then
  echo "=== Phase 6: install-idle-shutdown.sh (idle ${IDLE_MIN} min) ==="
  ssh ${SSH_OPTS} "root@${PUBLIC_IP}" "IDLE_THRESHOLD_MIN=${IDLE_MIN} bash -s" \
    < "${SCRIPT_DIR}/install-idle-shutdown.sh"
  echo ""
else
  echo "=== Phase 6: idle-shutdown SKIP (--no-idle-shutdown) ==="
  echo ""
fi

# ─────────────────────────────────────────
# Phase 7. ~/.ssh/config 追記 (idempotent)
# ─────────────────────────────────────────

echo "=== Phase 7: update ~/.ssh/config ==="

SSH_CFG="${HOME}/.ssh/config"
SPAWN_DATE=$(date -u +%Y-%m-%d)

# 既存の Host ${NAME} ブロックを除去 (Host ${NAME}-ts は別 host として残す)
if grep -q "^Host ${NAME}\$" "${SSH_CFG}" 2>/dev/null; then
  awk -v target="Host ${NAME}" '
    BEGIN { skip = 0 }
    /^Host / { skip = ($0 == target) ? 1 : 0 }
    !skip { print }
  ' "${SSH_CFG}" > "${SSH_CFG}.tmp"
  mv "${SSH_CFG}.tmp" "${SSH_CFG}"
  echo "  既存 'Host ${NAME}' ブロックを削除"
fi
if grep -q "^Host ${NAME}-ts\$" "${SSH_CFG}" 2>/dev/null; then
  awk -v target="Host ${NAME}-ts" '
    BEGIN { skip = 0 }
    /^Host / { skip = ($0 == target) ? 1 : 0 }
    !skip { print }
  ' "${SSH_CFG}" > "${SSH_CFG}.tmp"
  mv "${SSH_CFG}.tmp" "${SSH_CFG}"
  echo "  既存 'Host ${NAME}-ts' ブロックを削除"
fi

cat >> "${SSH_CFG}" <<SSH_ENTRY

# Build worker (Sakura ${ZONE}) — ${SPAWN_DATE} spawn (${CPU}C/${MEMORY}G/${DISK_GB}GB SSD)
Host ${NAME}
    HostName ${PUBLIC_IP}
    User root
    IdentityFile ~/.ssh/id_ed25519
    ServerAliveInterval 30
    ServerAliveCountMax 6
    ConnectTimeout 15
SSH_ENTRY

if [ -n "${TS_IP}" ]; then
  cat >> "${SSH_CFG}" <<SSH_ENTRY_TS

Host ${NAME}-ts
    HostName ${TS_IP}
    User root
    IdentityFile ~/.ssh/id_ed25519
    ServerAliveInterval 30
    ServerAliveCountMax 6
    ConnectTimeout 15
SSH_ENTRY_TS
  echo "  written: Host ${NAME} (${PUBLIC_IP}) + Host ${NAME}-ts (${TS_IP})"
else
  echo "  written: Host ${NAME} (${PUBLIC_IP})"
fi
echo ""

# ─────────────────────────────────────────
# Phase 8. サマリ
# ─────────────────────────────────────────

# Sakura tk1a の従量料金概算 (実勢値ではなく目安、 SSD 固定費別)
HOURLY=$(awk -v c="${CPU}" -v m="${MEMORY}" 'BEGIN { printf "%.0f", (c*15 + m*3) }')
MONTHLY_24H=$(awk -v h="${HOURLY}" 'BEGIN { printf "%.0f", h*24*30 }')

cat <<SUMMARY
=== ✅ build worker '${NAME}' spawned ===
  spec:           ${CPU}C / ${MEMORY}G / ${DISK_GB}GB SSD (zone ${ZONE})
  server_id:      ${SERVER_ID}
  public_ip:      ${PUBLIC_IP}
  tailscale_ip:   ${TS_IP:-N/A}
  hostname:       ${NAME}
  fleet-agent:    $([ ${USE_FLEET_AGENT} -eq 1 ] && echo "registered (slug=${FLEET_SLUG}, endpoint=${FLEET_CP_ENDPOINT})" || echo "skip")

  cost (rough):   ~¥${HOURLY}/h compute + SSD 固定費
                  常時稼働なら ~¥${MONTHLY_24H}/月 — idle-shutdown ${IDLE_MIN}min で大幅圧縮

  password:       op://${OP_VAULT}/${PW_ITEM_ID}/password
  ssh:            ssh ${NAME}      # 公開 IP 経由
$([ -n "${TS_IP}" ] && echo "                  ssh ${NAME}-ts   # Tailscale 経由")

  cleanup:        usacloud server delete -y --with-disks --zone ${ZONE} ${SERVER_ID}
                  op item delete ${PW_ITEM_ID} --vault ${OP_VAULT}
$([ ${USE_FLEET_AGENT} -eq 1 ] && echo "                  fleet cp server delete --slug ${FLEET_SLUG}   # CP-side")
SUMMARY
