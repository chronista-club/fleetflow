# Spec 20: Dashboard v2 — ピン留めステージビュー

## ステータス

Draft

## 背景

Dashboard v1 は CP のデータを一覧表示するだけのフラットな構造だった。
実運用では「自分が管理しているステージの状態を一目で把握したい」というニーズがある。

## ゴール

テナントユーザーが、関心のあるプロジェクト×ステージを **ピン留め** し、
1stビューで運用状態を即座に確認できるダッシュボードを提供する。

## 非ゴール

- ログ集約・閲覧（別 spec で設計）
- コスト管理ダッシュボード（既存の別機能）
- マルチテナント管理画面（管理者向け）

## アーキテクチャ

### 認証

- **Auth0** を使用（FleetFlow 専用テナント、creo とは分離）
  - creo: `creo.auth0.com` — アプリユーザー向け（既存）
  - fleetflow: `fleetstage.auth0.com` — インフラ管理者向け（新規）
- Auth0 Free プラン（7,500 MAU — 管理者のみなので十分）
- フロントエンド: Auth0 SPA SDK（`@auth0/auth0-spa-js`）
- バックエンド: JWT 検証（`jsonwebtoken` crate）
- テナント紐付け: Auth0 `app_metadata` にテナント slug を格納

### データモデル

```surql
-- ピン留め設定（テナントユーザーごと）
DEFINE TABLE pinned_stage SCHEMAFULL;
DEFINE FIELD user_id ON pinned_stage TYPE string;       -- Auth0 user ID
DEFINE FIELD tenant ON pinned_stage TYPE record<tenant>;
DEFINE FIELD project ON pinned_stage TYPE record<project>;
DEFINE FIELD stage ON pinned_stage TYPE record<stage>;
DEFINE FIELD sort_order ON pinned_stage TYPE int DEFAULT 0;
DEFINE FIELD created_at ON pinned_stage TYPE datetime DEFAULT time::now();
DEFINE INDEX idx_user ON pinned_stage FIELDS user_id;
```

### API エンドポイント

| メソッド | パス | 説明 | 認証 |
|---------|------|------|------|
| GET | `/api/auth/config` | Auth0 設定（domain, clientId） | 不要 |
| GET | `/api/me` | 認証ユーザー情報 + テナント | 要 |
| GET | `/api/pins` | ピン留めステージ一覧 | 要 |
| POST | `/api/pins` | ピン留め追加 | 要 |
| DELETE | `/api/pins/:id` | ピン留め解除 | 要 |
| PATCH | `/api/pins/reorder` | ピン留め並び替え | 要 |
| GET | `/api/stages/:project/:stage` | ステージ詳細（展開ビュー） | 要 |
| GET | `/api/stages/:project/:stage/services` | サービス一覧 | 要 |
| GET | `/api/stages/:project/:stage/deployments` | デプロイ履歴 | 要 |

### 画面構成

#### 1st ビュー（ピン留めステージ一覧）

```
┌─────────────────────────────────────────────────────┐
│ FleetFlow Dashboard           [user@email] [Logout] │
├─────────────────────────────────────────────────────┤
│                                                     │
│ 📌 creo-memories / live                             │
│    creo-prod · ● online · Last deploy: 2h ago       │
│                                                     │
│ 📌 creo-memories / dev                              │
│    creo-dev · ● online · Last deploy: 1d ago        │
│                                                     │
│ 📌 fleetflow / live                                 │
│    fleetflow-cp · ● online · Last deploy: 12h ago   │
│                                                     │
│ ─────────────────────────────────────────────────── │
│ [+ Add Pin]                                         │
│                                                     │
└─────────────────────────────────────────────────────┘
```

各ピンに表示する情報:
- プロジェクト名 / ステージ名
- 割り当てサーバー
- サーバーステータス（online / offline）
- 最終デプロイ日時（相対時間）

#### 展開ビュー（ピンをクリック）

```
┌─────────────────────────────────────────────────────┐
│ 📌 creo-memories / live          [Unpin] [Collapse] │
│    creo-prod · ● online · Last deploy: 2h ago       │
│                                                     │
│ ┌─ Services ──────────────────────────────────────┐ │
│ │ web    nginx:alpine     ● running    :80 :443   │ │
│ │ app    creo-app:latest  ● running    :3000      │ │
│ │ db     surrealdb:v3     ● running    :8000      │ │
│ └─────────────────────────────────────────────────┘ │
│                                                     │
│ ┌─ Recent Deployments ────────────────────────────┐ │
│ │ 2h ago   deploy live   success                  │ │
│ │ 1d ago   deploy live   success                  │ │
│ │ 3d ago   deploy live   failed                   │ │
│ └─────────────────────────────────────────────────┘ │
│                                                     │
│ ┌─ Logs ──────────────────────────────────────────┐ │
│ │ 🚧 Coming Soon — ログ集約機能は準備中です       │ │
│ └─────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────┘
```

## 実装フェーズ

### Phase 1: Auth0 認証基盤
- Auth0 テナント作成（fleetflow）
- バックエンド JWT 検証ミドルウェア
- フロントエンド Auth0 SPA SDK 統合
- `/api/auth/config`, `/api/me` 実装

### Phase 2: ピン留め機能
- `pinned_stage` テーブル作成
- ピン留め CRUD API 実装
- 1st ビュー UI（ピン一覧 + ステータス表示）

### Phase 3: 展開ビュー
- サービス一覧 API + UI
- デプロイ履歴 API + UI
- ログ Coming Soon プレースホルダー

## 依存関係

- Auth0 アカウント（fleetflow テナント）
- ステージ・サービスデータが CP に登録されていること
- SurrealDB に `pinned_stage` テーブル

## セキュリティ考慮

- JWT の `sub` クレームでユーザー識別
- テナント境界: `app_metadata.tenant_slug` で自テナントのデータのみアクセス可
- HTTPS 必須（Cloudflare Tunnel 経由を推奨）
