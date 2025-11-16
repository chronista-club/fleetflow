# OrbStack連携設計書

## 概要

fleetflowにOrbStackとの連携機能を実装し、コンテナのグループ化と視認性向上を実現する設計です。

### 推奨利用環境

この機能は**主にローカル開発環境**での利用を想定しています。OrbStackはmacOSのローカル開発で特に威力を発揮するため、開発者の日常的なワークフローを大幅に改善します。

- **ローカル開発**: 最も効果的な利用シーン
- **チーム開発**: 統一された命名規則でチーム間の認識を共有
- **マルチプロジェクト**: 複数プロジェクトの並行開発を支援

## 背景と課題

### 課題
- OrbStackで複数プロジェクトのコンテナが混在すると管理が困難
- Docker Composeと異なり、デフォルトではグループ化されない
- プロジェクト、ステージ、サービスの関係が不明瞭

### 解決策
- Docker Compose互換のラベルを自動付与
- 階層的な命名規則の採用
- OrbStackのグループ化機能を活用

## 実装仕様

### 1. コンテナ命名規則

```
{project}-{stage}-{service}
```

#### 例
- `vantage-local-surrealdb`
- `vantage-dev-qdrant`
- `estatebox-prod-api`

#### 利点
- プロジェクトの識別が容易
- ステージ（環境）が明確
- サービスの役割が一目瞭然

### 2. Dockerラベル仕様

#### 必須ラベル（OrbStackグループ化用）

| ラベル名 | 値の形式 | 用途 |
|---------|---------|------|
| `com.docker.compose.project` | `{project}-{stage}` | OrbStackグループ化キー |
| `com.docker.compose.service` | `{service}` | サービス識別子 |

#### 追加ラベル（メタデータ）

| ラベル名 | 値 | 用途 |
|---------|-----|------|
| `fleetflow.project` | プロジェクト名 | プロジェクト識別 |
| `fleetflow.stage` | ステージ名 | 環境識別 |
| `fleetflow.service` | サービス名 | サービス識別 |

### 3. 実装詳細

#### converter.rs の変更

```rust
// ラベル設定（OrbStackグループ化対応）
let mut labels = HashMap::new();
labels.insert(
    "com.docker.compose.project".to_string(),
    format!("{}-{}", project_name, stage_name),
);
labels.insert(
    "com.docker.compose.service".to_string(),
    service_name.to_string(),
);
labels.insert("fleetflow.project".to_string(), project_name.to_string());
labels.insert("fleetflow.stage".to_string(), stage_name.to_string());
labels.insert("fleetflow.service".to_string(), service_name.to_string());
```

#### parser.rs の変更

KDLファイルから`project`ノードを解析：

```rust
"project" => {
    if let Some(project_name) = node.entries()
        .first()
        .and_then(|e| e.value().as_string()) {
        name = project_name.to_string();
    }
}
```

## KDL設定例

```kdl
// プロジェクト名を宣言（必須）
project "vantage"

stage "local" {
    service "surrealdb"
    service "qdrant"
    service "vantage-hub"
}

service "surrealdb" {
    image "surrealdb/surrealdb"
    ports {
        port host=11000 container=8000
    }
}
```

## OrbStackでの表示

### グループ化の仕組み
1. `com.docker.compose.project`ラベルでグループ化
2. 同じプロジェクト・ステージのコンテナが1つのグループに
3. グループ名：`vantage-local`、`vantage-dev`など

### 表示階層
```
📁 vantage-local
  ├── surrealdb
  ├── qdrant
  └── vantage-hub

📁 estatebox-local
  ├── postgres
  └── redis
```

## 使用方法

> 💡 **ローカル開発での活用**
> 以下の手順は主にローカル開発環境（macOS + OrbStack）での利用を想定しています。
> 本番環境では異なるオーケストレーションツールの使用を推奨します。

### 1. flow.kdlにproject宣言を追加

```kdl
project "my-project"
```

### 2. fleetflowでコンテナを起動

```bash
fleetflow up -s local
```

### 3. OrbStackで確認
- OrbStackアプリケーションを開く
- Containersセクションでグループ化を確認

## 技術的メリット

### 1. 互換性
- Docker Compose標準のラベルを使用
- 他のツールとの相互運用性を確保

### 2. 拡張性
- 追加のメタデータラベルで将来の機能拡張に対応
- プロジェクト固有の情報を保持

### 3. 視認性
- 一貫性のある命名規則
- プロジェクト間の明確な分離
- 環境（ステージ）の識別が容易

## ポート設定ガイドライン

プロジェクトごとに推奨ポート範囲を設定：

| プロジェクト | ポート範囲 | 用途 |
|-------------|-----------|------|
| vantage | 11000-11099 | Hub関連サービス |
| estatebox | 5432, 6333-6334 | データベース・ベクトルDB |
| fleetflow | 8080-8089 | 管理UI・API |

## 今後の拡張案

### 1. ラベルベースのフィルタリング
```bash
fleetflow ps --filter project=vantage
fleetflow ps --filter stage=local
```

### 2. 自動クリーンアップ
```bash
fleetflow clean --project vantage
```

### 3. プロジェクト間の依存関係
```kdl
project "frontend" {
    depends_on "backend"
}
```

### 4. OrbStackネイティブ統合
- OrbStack APIとの直接連携
- グループ単位での操作（一括停止・削除）

## トラブルシューティング

### グループ化されない場合
1. ラベルの確認
```bash
docker inspect <container> | jq '.[] | .Config.Labels'
```

2. fleetflowの再ビルド
```bash
cd fleetflow && cargo build --release
```

3. コンテナの再作成
```bash
fleetflow down -s local --remove
fleetflow up -s local
```

### 名前の競合
- 異なるプロジェクトで同じコンテナ名は不可
- プロジェクト名を一意に設定

## ローカル開発環境でのメリット

### なぜローカル環境に最適か

1. **macOS固有の最適化**
   - OrbStackはmacOS専用に最適化されている
   - Docker Desktopよりも軽量で高速

2. **開発効率の向上**
   - GUI経由での直感的なコンテナ管理
   - グループ単位での一括操作が可能
   - ログの確認やシェルアクセスが容易

3. **リソース管理**
   - メモリ使用量の削減
   - CPUリソースの効率的な利用
   - バッテリー消費の最小化

### チーム開発での活用例

```bash
# 朝の開発開始
fleetflow up -s local  # vantage-localグループが作成される

# 別プロジェクトも同時に起動
cd ../estatebox
fleetflow up -s local  # estatebox-localグループが作成される

# OrbStackで両プロジェクトを独立管理
# グループごとに停止・再起動が可能
```

## まとめ

このOrbStack連携により、fleetflowは以下を実現：

1. **視認性の向上** - 階層的な命名とグループ化
2. **管理の簡素化** - プロジェクト単位での操作
3. **互換性の確保** - Docker Compose標準への準拠
4. **拡張性の確保** - 将来の機能追加への対応

特に**ローカル開発環境において**、複数プロジェクトの並行開発が格段に効率化されます。OrbStackのGUIと組み合わせることで、コンテナ管理の煩雑さから開発者を解放し、本来のコーディングに集中できる環境を提供します。

---

作成日: 2025-11-16
バージョン: 1.0.0
著者: fleetflow開発チーム