# Bollard非推奨警告修正計画書

## 目的
FleetFlowコードベースからBollard APIの非推奨警告をすべて除去し、最新のOpenAPI生成されたAPIに移行する。

## 現状分析

### 影響を受けるクレート
1. `fleetflow-container` - コンテナ設定の変換ロジック
2. `fleetflow` - CLIコマンド実装
3. `fleetflow-build` - イメージビルド機能

### 非推奨API一覧

#### 1. Container API
- **非推奨**: `bollard::container::Config`
- **新API**: `bollard::models::ContainerConfig` または `bollard::models::ContainerCreateBody`
- **影響箇所**:
  - `fleetflow-container/src/converter.rs`
  - `fleetflow/src/main.rs` (複数箇所)

#### 2. CreateContainerOptions
- **非推奨**: `bollard::container::CreateContainerOptions`
- **新API**: `bollard::query_parameters::CreateContainerOptions` + `CreateContainerOptionsBuilder`
- **影響箇所**:
  - `fleetflow-container/src/converter.rs`
  - `fleetflow/src/main.rs`

#### 3. Network API
- **非推奨**: `bollard::network::CreateNetworkOptions`
- **新API**: `bollard::models::NetworkCreateRequest`
- **影響箇所**:
  - `fleetflow/src/main.rs:387`

#### 4. Image API
- **非推奨**: `bollard::image::CreateImageOptions`
- **新API**: `bollard::query_parameters::CreateImageOptions` + `CreateImageOptionsBuilder`
- **影響箇所**:
  - `fleetflow/src/main.rs:70`

- **非推奨**: `bollard::image::BuildImageOptions`
- **新API**: `bollard::query_parameters::BuildImageOptions` + `BuildImageOptionsBuilder`
- **影響箇所**:
  - `fleetflow-build/src/builder.rs`

#### 5. Logs API
- **非推奨**: `bollard::container::LogsOptions`
- **新API**: `bollard::query_parameters::LogsOptions` + `LogsOptionsBuilder`
- **影響箇所**:
  - `fleetflow/src/main.rs:850`

#### 6. ListContainers API
- **非推奨**: `bollard::container::ListContainersOptions`
- **新API**: `bollard::query_parameters::ListContainersOptions` + `ListContainersOptionsBuilder`
- **影響箇所**:
  - `fleetflow/src/main.rs:721`

## 移行戦略

### フェーズ1: converter.rs の修正
**優先度**: 高

#### 課題
- `Config<String>` → `ContainerConfig` への移行
- `CreateContainerOptions<String>` → 新APIへの移行
- 関数シグネチャの変更が必要

#### 実装手順
1. 新しいAPIのimport追加
2. `service_to_container_config_with_network`関数のシグネチャ変更
3. `Config`構築ロジックを`ContainerConfig`または`ContainerCreateBody`に移行
4. テストケースの更新と検証

#### 懸念事項
- `Config<String>`のジェネリクスが新APIでどう扱われるか
- `NetworkingConfig`との互換性
- `HealthCheck`のマッピング

### フェーズ2: fleetflow の修正
**優先度**: 中

#### 対象箇所
1. **イメージプル** (`main.rs:70`)
   - `CreateImageOptions` → `CreateImageOptionsBuilder`使用

2. **ネットワーク作成** (`main.rs:387`)
   - `CreateNetworkOptions` → `NetworkCreateRequest`

3. **コンテナ一覧** (`main.rs:721`)
   - `ListContainersOptions` → `ListContainersOptionsBuilder`

4. **ログ取得** (`main.rs:850`)
   - `LogsOptions` → `LogsOptionsBuilder`

#### 実装手順
各API呼び出しを個別に修正:
1. Builderパターンへの移行
2. フィールド名の確認と調整
3. エラーハンドリングの確認

### フェーズ3: fleetflow-build の修正
**優先度**: 低

#### 対象箇所
- `builder.rs:35` - `BuildImageOptions` → `BuildImageOptionsBuilder`

#### 実装手順
1. Builderパターンへの移行
2. `dockerfile`, `t`, `buildargs`フィールドの新API対応
3. ビルドテストの実行

## リスク評価

### 高リスク
- **converter.rs**: FleetFlowのコア機能。変更により既存の全機能に影響
- **型の変更**: 関数シグネチャ変更によるAPIの破壊的変更

### 中リスク
- **CLI コマンド**: ユーザーが直接使用する機能だが、内部実装の変更のみ
- **テストケース**: 大量のテストケース更新が必要

### 低リスク
- **ビルド機能**: 比較的独立した機能

## マイルストーン

### マイルストーン1: 調査と設計 (1-2時間)
- [ ] 新APIのドキュメント確認
- [ ] 各非推奨APIの新APIへのマッピング表作成
- [ ] テストケース洗い出し

### マイルストーン2: converter.rs移行 (2-3時間)
- [ ] 新APIへのimport切り替え
- [ ] 関数実装の書き換え
- [ ] ユニットテスト実行・修正
- [ ] 統合テスト実行

### マイルストーン3: fleetflow移行 (2-3時間)
- [ ] イメージAPI移行
- [ ] ネットワークAPI移行
- [ ] コンテナAPI移行
- [ ] ログAPI移行
- [ ] CLIコマンドテスト

### マイルストーン4: fleetflow-build移行 (1-2時間)
- [ ] BuildImageOptions移行
- [ ] ビルド機能テスト

### マイルストーン5: 最終検証 (1時間)
- [ ] 全クレートのビルド確認
- [ ] 全警告の除去確認
- [ ] 統合テストスイート実行
- [ ] ドキュメント更新

## 開始前チェックリスト
- [ ] 現在のコードのバックアップまたはgit commit
- [ ] Bollardのバージョン確認 (Cargo.toml)
- [ ] Bollardのchangelogとマイグレーションガイド確認
- [ ] テスト環境の準備

## 補足: `#[allow(deprecated)]`の使用について
緊急対応として、一時的に`#[allow(deprecated)]`を使用して警告を抑制することは可能だが、技術的負債となるため、計画的な移行を推奨。

## 参考リンク
- Bollard Documentation: https://docs.rs/bollard/
- Docker Engine API: https://docs.docker.com/engine/api/
