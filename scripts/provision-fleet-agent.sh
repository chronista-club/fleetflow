#!/usr/bin/env bash
# fleet-agent provisioning script
#
# 既に provision-worker-base.sh が完了済みの host で、fleet-agent binary を
# 配置 + systemd unit を起こして CP に接続させる。
#
# 使い方:
#   ssh root@<host> 'bash -s -- --slug <slug> --endpoint <cp-endpoint>' \
#     < scripts/provision-fleet-agent.sh
#
#   # または環境変数経由:
#   FLEET_SLUG=build-01 FLEET_CP_ENDPOINT=cp.fleetstage.cloud:4510 \
#     bash provision-fleet-agent.sh
#
# 前提:
#   - root 権限
#   - /usr/local/bin/fleet-agent はすでに別途 scp 済み (Mac で zigbuild → scp)
#   - Tailscale 接続済み (cp.fleetstage.cloud に到達できる)
#
# 冪等性: 既存 unit があれば内容更新 + restart。何度実行しても同じ結果。

set -euo pipefail

# ─────────────────────────────────────────
# 引数 / 環境変数
# ─────────────────────────────────────────
FLEET_SLUG="${FLEET_SLUG:-}"
FLEET_CP_ENDPOINT="${FLEET_CP_ENDPOINT:-cp.fleetstage.cloud:4510}"
FLEET_HEARTBEAT_INTERVAL="${FLEET_HEARTBEAT_INTERVAL:-30}"
FLEET_MONITOR_INTERVAL="${FLEET_MONITOR_INTERVAL:-30}"
FLEET_RESTART_THRESHOLD="${FLEET_RESTART_THRESHOLD:-3}"
FLEET_DEPLOY_BASE="${FLEET_DEPLOY_BASE:-/opt/apps}"

while [[ $# -gt 0 ]]; do
  case $1 in
    --slug) FLEET_SLUG="$2"; shift 2;;
    --endpoint) FLEET_CP_ENDPOINT="$2"; shift 2;;
    --heartbeat) FLEET_HEARTBEAT_INTERVAL="$2"; shift 2;;
    --monitor) FLEET_MONITOR_INTERVAL="$2"; shift 2;;
    --threshold) FLEET_RESTART_THRESHOLD="$2"; shift 2;;
    --deploy-base) FLEET_DEPLOY_BASE="$2"; shift 2;;
    *) echo "Unknown arg: $1" >&2; exit 1;;
  esac
done

if [ -z "${FLEET_SLUG}" ]; then
  echo "ERROR: --slug <name> または FLEET_SLUG 環境変数が必須" >&2
  exit 1
fi

if [ ! -x /usr/local/bin/fleet-agent ]; then
  echo "ERROR: /usr/local/bin/fleet-agent が見当たりません。" >&2
  echo "       Mac で zigbuild → scp 済みか確認してください。" >&2
  exit 1
fi

echo "=== fleet-agent provisioning ==="
echo "  slug:      ${FLEET_SLUG}"
echo "  endpoint:  ${FLEET_CP_ENDPOINT}"
echo "  heartbeat: ${FLEET_HEARTBEAT_INTERVAL}s"
echo "  monitor:   ${FLEET_MONITOR_INTERVAL}s"
echo "  deploy:    ${FLEET_DEPLOY_BASE}"
echo ""

# ─────────────────────────────────────────
# 1. deploy_base 用ディレクトリ
# ─────────────────────────────────────────
echo "[1/3] deploy_base..."
mkdir -p "${FLEET_DEPLOY_BASE}"
echo "  ${FLEET_DEPLOY_BASE} ready."

# ─────────────────────────────────────────
# 2. systemd unit
# ─────────────────────────────────────────
echo "[2/3] systemd unit..."
cat > /etc/systemd/system/fleet-agent.service <<UNIT
[Unit]
Description=FleetFlow Agent
After=network-online.target docker.service
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/fleet-agent
Environment=FLEET_AGENT_CP_ENDPOINT=${FLEET_CP_ENDPOINT}
Environment=FLEET_AGENT_SERVER_SLUG=${FLEET_SLUG}
Environment=FLEET_AGENT_HEARTBEAT_INTERVAL=${FLEET_HEARTBEAT_INTERVAL}
Environment=FLEET_AGENT_MONITOR_INTERVAL=${FLEET_MONITOR_INTERVAL}
Environment=FLEET_AGENT_RESTART_THRESHOLD=${FLEET_RESTART_THRESHOLD}
Environment=FLEET_AGENT_DEPLOY_BASE=${FLEET_DEPLOY_BASE}
Environment=RUST_LOG=fleet_agent=info,unison=info
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
echo "  unit installed."

# ─────────────────────────────────────────
# 3. enable + start
# ─────────────────────────────────────────
echo "[3/3] enable + start..."
systemctl enable --now fleet-agent
sleep 3
echo ""
echo "=== status ==="
systemctl status fleet-agent --no-pager -l | head -10
echo ""
echo "=== recent logs ==="
journalctl -u fleet-agent -n 6 --no-pager
echo ""
echo "次のステップ (手元 Mac から):"
echo "  fleet cp server register --slug ${FLEET_SLUG} --provider sakura --ssh-host \$(tailscale ip -4)"
echo "  fleet cp server list   # status: online を確認"
