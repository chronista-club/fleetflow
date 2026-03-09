# FleetFlow Platform ビジョン

## ステータス: Draft

## 概要

FleetFlow を単一プロジェクトの CLI ツールから、複数サービスを横断管理する中央管理基盤（Control Plane）へ進化させる。

**ビジョン**: 「伝えれば、動く」を、1サービスから事業全体へ。

## 背景

ANYCREATIVE Inc は複数のサービスを開発・運用していく。現在 FleetFlow は creo-memories の単一プロジェクト管理で実績があり、この基盤を拡張して全サービスを統一管理する。将来的には外部顧客への SaaS 提供も視野に入れる。

## データモデル

```
Tenant
 └─ Project
     └─ Stage
         └─ Service
             └─ Container
```

### Tenant

- サービス群を所有する組織単位
- 最初のテナント: ANYCREATIVE Inc（自社ドッグフーディング）
- 将来: マルチテナント対応で外部顧客を受入

### Project

- 1つのサービス/プロダクトに対応（例: creo-memories, vantage-point）
- 各プロジェクトは独自の `fleet.kdl` を持つ

### Stage

- プロジェクトごとに自由定義（例: local / dev / staging / prod）
- 全プロジェクトで統一する必要はない
- Control Plane が横断クエリ可能（`--stage prod` で全プロジェクトの prod を一覧）

### Service / Container

- 既存の FleetFlow の概念をそのまま継承

## サービスポートフォリオ（ANYCREATIVE Inc）

| サービス | ジャンル | 概要 |
|---------|---------|------|
| Creo Memories | AI × 記憶管理 | AI エージェントの永続記憶基盤 |
| Vantage Point | 開発ツール | リッチコンテンツビューア / Canvas |
| CPLP Sound System | 音楽 × リアルタイム | オンラインギグプラットフォーム |
| FleetFlow | インフラ基盤 | 全サービスの管理基盤（これ自体も製品） |

## アーキテクチャ

### Control Plane

クラウド上に常駐するプロセス。全サーバー・全サービスの状態を常時把握し、3つのインターフェースからの操作を受け付ける。

```
        ┌──────┐   ┌──────┐   ┌──────────┐
        │ CLI  │   │ MCP  │   │  WebUI   │
        │(fleet)│  │Server│   │Dashboard │
        └──┬───┘   └──┬───┘   └────┬─────┘
           └──────────┼────────────┘
                      ▼
         ┌────────────────────────┐
         │  FleetFlow Core API   │
         │  ──────────────────   │
         │  • テナント管理        │
         │  • プロジェクト管理     │
         │  • ステージ管理        │
         │  • サービス管理        │
         │  • サーバー管理        │
         │  • デプロイ制御        │
         │  • ヘルスモニタリング   │
         │  • コスト追跡          │
         │  • DNS/TLS 管理       │
         │  • 認証基盤            │
         └───────────┬────────────┘
                     │
     ┌───────────────┼───────────────┐
     ▼               ▼               ▼
  VPS/Cloud       VPS/Cloud       VPS/Cloud
   (creo)          (vp)           (cplp)
```

### 3つのインターフェース

#### 1. CLI (`fleet`)

従来の CLI に加え、プロジェクト横断の管理コマンドを追加。

```bash
# 横断クエリ
fleet ps --stage prod              # 全プロジェクトの prod を横断表示
fleet ps --project creo-memories   # creo の全 stage を表示
fleet ps --all                     # 全部

# テナント・コスト
fleet tenant status                # テナント状態
fleet cost summary                 # 月次コスト（プロジェクト×stage別）

# デプロイ
fleet deploy creo-memories prod    # 個別デプロイ
```

#### 2. MCP Server

AI エージェントから管理操作を実行。

```
AI: 「prod 環境の全サービス状態を教えて」
AI: 「creo-memories の SurrealDB をバックアップして」
AI: 「今月のコスト内訳は？」
```

#### 3. WebUI Dashboard

- テナント × プロジェクト × ステージのマトリクス表示
- リアルタイムヘルス / メトリクス
- デプロイ履歴 / ログビューア
- コストダッシュボード（プロジェクト × stage 別損益）

## 横断基盤

### 認証基盤

- ANYCREATIVE 社内の統一認証
- 各サービスのエンドユーザー認証（サービスごとに分離）
- 将来: 外部テナントの認証分離
- 技術選定は未決（Auth0 継続 vs 自前 vs ハイブリッド）

### コスト管理

- さくらクラウド API → VPS 費用
- Cloudflare API → DNS/CDN 費用
- Auth0 → 認証費用
- Stripe 手数料 → 決済コスト
- プロジェクト × stage 別の月次損益を可視化

### DNS / ドメイン管理

- 開発用ドメイン（objectrecords.io）
- 各サービスの本番ドメイン
- Cloudflare API 経由の統合管理

## 進化ロードマップ

| Phase | テーマ | 内容 |
|-------|--------|------|
| 0 | Single Project CLI | 1プロジェクトの管理（完了） |
| 1 | Control Plane | 常駐デーモン + Core API + テナント/プロジェクト/stage モデル |
| 2 | Multi-Project | Fleet Registry で ANYCREATIVE 全サービス統合 |
| 3 | Interfaces | MCP Server v2 + WebUI Dashboard |
| 4 | 横断基盤 | 認証統合 + コスト管理 + DNS 統合 |
| 5 | SaaS 化 | マルチテナント対応 → 外部顧客受入 |

## 未決事項

1. **コントロールプレーンの配置** — 専用サーバー vs 既存VPS同居（規模に応じて後決め）
2. **認証基盤の技術選定** — Auth0 継続 vs 自前構築 vs ハイブリッド
3. **SaaS で売るもの** — FleetFlow 基盤？ 各サービス？ 両方？
4. **VP / CPLP のインフラ要件** — 特に CPLP の低レイテンシ要件
5. **技術スタック** — WebUI: SolidJS? / Control Plane: Rust?

## 文書化の責務分担

| 置き場 | 役割 | 内容の例 |
|--------|------|---------|
| creo-memories | 脳（記憶・意思決定の経緯） | 「なぜこうなったか」の議論ログ |
| spec/ (リポジトリ内) | 設計図（確定した仕様） | このドキュメント |
| GitHub Issues/Project | 現場（やること・進捗） | タスク分解、マイルストーン |
