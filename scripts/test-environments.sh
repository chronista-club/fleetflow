#!/usr/bin/env bash
set -euo pipefail

# FleetFlow環境テストスクリプト
# dev/prod環境の起動・接続確認・停止を自動実行

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "🧪 FleetFlow環境テスト開始"
echo "プロジェクトルート: $PROJECT_ROOT"
echo ""

# クリーンアップ関数
cleanup() {
    echo ""
    echo "🧹 クリーンアップ中..."
    fleetflow down dev --remove 2>/dev/null || true
    fleetflow down prod --remove 2>/dev/null || true
}

# スクリプト終了時にクリーンアップ
trap cleanup EXIT

# 1. Dev環境テスト
echo "📦 [1/4] Dev環境を起動中..."
cd "$PROJECT_ROOT"
fleetflow up dev

echo ""
echo "🔍 [2/4] Dev環境に接続確認中（ポート: 40001）..."
sleep 3  # SurrealDBの起動を待つ

# ヘルスチェック（HTTPエンドポイント）
if curl -f -s http://localhost:40001/health > /dev/null 2>&1; then
    echo "✅ Dev環境 (40001) への接続成功"
else
    echo "❌ Dev環境 (40001) への接続失敗"
    exit 1
fi

echo ""
echo "🛑 Dev環境を停止中..."
fleetflow down dev

# 2. Prod環境テスト
echo ""
echo "📦 [3/4] Prod環境を起動中..."
fleetflow up prod

echo ""
echo "🔍 [4/4] Prod環境に接続確認中（ポート: 40002）..."
sleep 3  # SurrealDBの起動を待つ

# ヘルスチェック（HTTPエンドポイント）
if curl -f -s http://localhost:40002/health > /dev/null 2>&1; then
    echo "✅ Prod環境 (40002) への接続成功"
else
    echo "❌ Prod環境 (40002) への接続失敗"
    exit 1
fi

echo ""
echo "🛑 Prod環境を停止中..."
fleetflow down prod --remove

echo ""
echo "✅ すべての環境テストが完了しました！"
echo ""
echo "サマリー:"
echo "  ✓ Dev環境 (40001) - 起動・接続・停止"
echo "  ✓ Prod環境 (40002) - 起動・接続・停止"
