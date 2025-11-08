# 自動インポート機能 - 仕様書

## コンセプト

**"ファイルを置くだけで、設定完了"**

FleetFlowは、`include` 文すら不要にする、規約ベースの自動インポート機能を提供します。
開発者は決められたディレクトリにKDLファイルを配置するだけで、自動的に読み込まれます。

## 哲学

### Convention over Configuration（設定より規約）

```
❌ 従来の方法（明示的なinclude）:
flow.kdl に以下を記述する必要がある:
  include "services/api.kdl"
  include "services/postgres.kdl"
  include "services/redis.kdl"
  include "stages/local.kdl"
  include "stages/dev.kdl"
  ...

✅ FleetFlowの方法（規約ベース）:
ファイルを配置するだけ:
  services/api.kdl      ← 自動的に読み込まれる
  services/postgres.kdl ← 自動的に読み込まれる
  stages/local.kdl      ← 自動的に読み込まれる
```

### 利点

1. **ボイラープレートの削減**: `include` 文を書く手間がゼロ
2. **一貫性**: 全てのプロジェクトで同じ構造
3. **発見性**: ファイルの場所が予測可能
4. **スケーラビリティ**: ファイルを追加するだけで自動認識

## プロジェクト構造

### 標準的なディレクトリレイアウト

```
project/
├── flow.kdl              # ルートファイル（エントリーポイント）
│
├── services/             # サービス定義ディレクトリ
│   ├── api.kdl           # 自動インポート
│   ├── postgres.kdl      # 自動インポート
│   ├── redis.kdl         # 自動インポート
│   └── worker.kdl        # 自動インポート
│
├── stages/               # ステージ定義ディレクトリ
│   ├── local.kdl         # 自動インポート
│   ├── dev.kdl           # 自動インポート
│   ├── stg.kdl           # 自動インポート
│   └── prod.kdl          # 自動インポート
│
└── variables/            # 変数定義ディレクトリ（将来実装）
    ├── common.kdl        # 自動インポート
    └── secrets.kdl       # 自動インポート
```

### サブディレクトリのサポート

ネストしたディレクトリ構造もサポート:

```
services/
├── backend/
│   ├── api.kdl           # 自動インポート
│   └── worker.kdl        # 自動インポート
├── frontend/
│   └── web.kdl           # 自動インポート
└── infrastructure/
    ├── postgres.kdl      # 自動インポート
    └── redis.kdl         # 自動インポート
```

**検索パターン**: `services/**/*.kdl` で再帰的にスキャン

## 仕様

### FR-001: ディレクトリベースの自動発見

**目的**: 規約に従ったディレクトリから自動的にKDLファイルを読み込む

**動作**:

1. `flow.kdl` が存在するディレクトリをプロジェクトルートとする
2. 以下のディレクトリを自動的にスキャン:
   - `./services/**/*.kdl` → service 定義
   - `./stages/**/*.kdl` → stage 定義
   - `./variables/**/*.kdl` → 変数定義（将来）

**アルゴリズム**:

```
1. プロジェクトルートを特定
2. 各規約ディレクトリの存在を確認
3. 存在する場合、再帰的に.kdlファイルをスキャン
4. ファイル名のアルファベット順にソート
5. 順次パースして統合
```

**例**:

```rust
// 疑似コード
fn discover_files(project_root: PathBuf) -> Result<DiscoveredFiles> {
    let services = glob(project_root.join("services/**/*.kdl"))?
        .sorted();
    
    let stages = glob(project_root.join("stages/**/*.kdl"))?
        .sorted();
    
    Ok(DiscoveredFiles { services, stages })
}
```

### FR-002: ファイル読み込み順序

**目的**: 予測可能で一貫した読み込み順序を保証

**読み込み順序**:

```
1. flow.kdl（ルートファイル）
2. services/**/*.kdl（アルファベット順）
3. stages/**/*.kdl（アルファベット順）
4. variables/**/*.kdl（アルファベット順）
```

**ソート規則**:

- ファイル名のアルファベット順（辞書順）
- ディレクトリ階層は深さ優先探索
- 同じディレクトリ内ではファイル名でソート

**例**:

```
読み込み順序:
1. flow.kdl
2. services/api.kdl
3. services/backend/worker.kdl
4. services/postgres.kdl
5. services/redis.kdl
6. stages/dev.kdl
7. stages/local.kdl
8. stages/prod.kdl
```

### FR-003: ルートファイル (flow.kdl) の役割

**目的**: プロジェクト全体のエントリーポイントとグローバル設定

**flow.kdl の用途**:

1. **プロジェクトの識別**: このファイルの存在でプロジェクトルートを特定
2. **グローバル設定**: 全体に適用される設定を記述
3. **メタデータ**: プロジェクト名、バージョンなど

**最小構成**:

```kdl
// flow.kdl
// 空でもOK（services/, stages/ が自動読み込みされる）
```

**推奨構成**:

```kdl
// flow.kdl
project "myapp" {
    version "1.0.0"
    description "My awesome application"
}

defaults {
    restart_policy "unless-stopped"
    network "bridge"
}
```

**フル構成**:

```kdl
// flow.kdl
project "myapp" {
    version "1.0.0"
    description "My awesome application"
    author "Your Name"
    repository "https://github.com/yourorg/myapp"
}

defaults {
    restart_policy "unless-stopped"
    network "bridge"
    
    // すべてのサービスに適用されるデフォルト環境変数
    environment {
        TZ "Asia/Tokyo"
        LANG "ja_JP.UTF-8"
    }
}

// グローバル変数（将来実装）
variables {
    app_version "1.0.0"
    registry "ghcr.io/myorg"
}
```

### FR-004: サービスファイルの記述

**目的**: サービス定義を個別ファイルで管理

**ファイルパス**: `services/**/*.kdl`

**ファイル内容**: 1ファイルに1つ以上のサービス定義

**例**:

```kdl
// services/api.kdl
service "api" {
    image "myapp:1.0.0"
    
    ports {
        port 8080 3000
    }
    
    environment {
        NODE_ENV "production"
        PORT "3000"
    }
    
    depends_on "postgres" "redis"
}
```

```kdl
// services/postgres.kdl
service "postgres" {
    version "16"
    
    ports {
        port 5432 5432
    }
    
    volumes {
        volume "./data/postgres" "/var/lib/postgresql/data"
    }
    
    environment {
        POSTGRES_PASSWORD "password"
        POSTGRES_DB "myapp"
    }
}
```

```kdl
// services/redis.kdl
service "redis" {
    version "7-alpine"
    
    ports {
        port 6379 6379
    }
}
```

**複数サービスを1ファイルに記述も可能**:

```kdl
// services/infrastructure.kdl
service "postgres" {
    version "16"
}

service "redis" {
    version "7"
}

service "minio" {
    version "latest"
}
```

### FR-005: ステージファイルの記述

**目的**: 実行環境ごとの設定を個別ファイルで管理

**ファイルパス**: `stages/**/*.kdl`

**ファイル内容**: 1ファイルに1つ以上のステージ定義

**例**:

```kdl
// stages/local.kdl
stage "local" {
    service "api"
    service "postgres"
    service "redis"
    
    variables {
        DEBUG "true"
        LOG_LEVEL "debug"
        DATABASE_URL "postgresql://localhost:5432/myapp_dev"
    }
}
```

```kdl
// stages/dev.kdl
stage "dev" {
    service "api"
    service "postgres"
    service "redis"
    
    variables {
        DEBUG "true"
        LOG_LEVEL "info"
        DATABASE_URL "postgresql://dev-db:5432/myapp_dev"
    }
}
```

```kdl
// stages/prod.kdl
stage "prod" {
    service "api"
    service "worker"
    service "postgres"
    service "redis"
    
    variables {
        DEBUG "false"
        LOG_LEVEL "warn"
        DATABASE_URL "postgresql://prod-db:5432/myapp"
    }
}
```

### FR-006: 名前の衝突解決

**目的**: 同じ名前のサービス/ステージが複数ファイルで定義された場合の処理

**原則**: **後勝ち（Last-Win）**

**動作**:

1. 同じ名前の定義が複数ある場合、最後に読み込まれた定義を採用
2. 警告メッセージを出力（デバッグモード）
3. エラーにはしない（柔軟性を保つ）

**例**:

```kdl
// services/postgres.kdl
service "postgres" {
    version "15"
}

// services/database-override.kdl（アルファベット順で後）
service "postgres" {
    version "16"  // ← この定義が採用される
}
```

**警告メッセージ**:

```
⚠️  Warning: Service 'postgres' は複数回定義されています
  最初の定義: services/postgres.kdl:1
  上書き定義: services/database-override.kdl:1
  → 後者の定義が採用されます
```

**推奨事項**:

- 同じ名前の定義は避ける
- 必要な場合は、ファイル名で意図を明示（例: `postgres-override.kdl`）

### FR-007: オーバーライド機能

**目的**: 規約ディレクトリ外でのオーバーライドを可能にする

**使用ケース**:

- ローカル開発用の一時的な設定
- CI/CD環境での動的な設定
- チーム内の個人用カスタマイズ

**オーバーライドファイル**:

```
flow.local.kdl    # ルートディレクトリに配置（gitignore推奨）
```

**読み込み順序**:

```
1. flow.kdl
2. services/**/*.kdl
3. stages/**/*.kdl
4. flow.local.kdl  ← 最後に読み込み、すべてをオーバーライド可能
```

**例**:

```kdl
// flow.local.kdl（個人用カスタマイズ、git管理外）
service "postgres" {
    version "16"  // チームの標準は15だが、個人的に16を使いたい
    
    environment {
        POSTGRES_PASSWORD "my-local-password"
    }
}

stage "local" {
    variables {
        DEBUG "true"
        API_PORT "9000"  // デフォルトの8080から変更
    }
}
```

### FR-008: エラーハンドリング

**目的**: 明確なエラーメッセージで問題を特定しやすくする

#### エラーケース1: 規約ディレクトリが存在しない

```
状況: services/, stages/ どちらも存在しない

動作: エラーにはしない（警告のみ）

理由: 最小構成でも動作させる柔軟性
```

**警告メッセージ**:

```
⚠️  Warning: 規約ディレクトリが見つかりません
  - services/ ディレクトリが存在しません
  - stages/ ディレクトリが存在しません
  
  推奨事項:
    mkdir services stages
    echo 'service "myapp" {}' > services/myapp.kdl
```

#### エラーケース2: KDLパースエラー

```
状況: ファイルの構文エラー

動作: エラーで停止

理由: 不正な設定で実行すると予期しない動作
```

**エラーメッセージ**:

```
✗ Error: KDL構文エラー
  ファイル: services/api.kdl:12:5
  
  12 |     port 8080 3000
     |     ^^^^ 予期しないトークン
  
  services/api.kdl の構文を確認してください
```

#### エラーケース3: 未定義のサービス参照

```
状況: stage が存在しないサービスを参照

動作: エラーで停止

理由: 実行時エラーを防ぐ
```

**エラーメッセージ**:

```
✗ Error: 未定義のサービス参照
  ファイル: stages/local.kdl:3
  
  stage "local" は存在しないサービス "postgres" を参照しています
  
  解決方法:
    1. services/postgres.kdl を作成する
    2. または stage "local" から "postgres" を削除する
```

### FR-009: デバッグモード

**目的**: ファイル発見と読み込みのプロセスを可視化

**コマンド**:

```bash
flow validate --debug
```

**出力例**:

```
🔍 プロジェクト検出
  ルート: /path/to/project
  flow.kdl: 検出

🔍 ディレクトリスキャン
  services/: 検出
  stages/: 検出
  variables/: 未検出

📂 ファイル発見 (services/)
  ✓ services/api.kdl
  ✓ services/postgres.kdl
  ✓ services/redis.kdl
  ✓ services/backend/worker.kdl

📂 ファイル発見 (stages/)
  ✓ stages/local.kdl
  ✓ stages/dev.kdl
  ✓ stages/prod.kdl

📖 読み込み順序
  1. flow.kdl
  2. services/api.kdl
  3. services/backend/worker.kdl
  4. services/postgres.kdl
  5. services/redis.kdl
  6. stages/dev.kdl
  7. stages/local.kdl
  8. stages/prod.kdl

✅ パース完了
  サービス: 4個
  ステージ: 3個

⚠️  警告: 0件
✗ エラー: 0件
```

## 非機能要件

### パフォーマンス

- ファイルスキャン: 1000ファイルで < 100ms
- パース: 100ファイルで < 1秒
- メモリ使用量: O(n) (nはファイル数)

### 互換性

- 相対パスはプロジェクトルートからの相対
- シンボリックリンクをサポート
- .gitignore に従ってスキャン（オプション）

### セキュリティ

- ディレクトリトラバーサル攻撃の防止
- ファイルパスのサニタイズ
- 再帰的なシンボリックリンクの検出

## 移行ガイド

### 既存プロジェクトの移行

**Before（明示的include）**:

```kdl
// flow.kdl
include "services/api.kdl"
include "services/postgres.kdl"
include "stages/local.kdl"
```

**After（自動インポート）**:

```kdl
// flow.kdl
// 空でOK（規約ディレクトリから自動読み込み）
```

**移行手順**:

1. 既存のファイル構造を規約に合わせる:
   ```bash
   mkdir -p services stages
   mv api.kdl services/
   mv postgres.kdl services/
   mv local.kdl stages/
   ```

2. flow.kdl から `include` 文を削除

3. 動作確認:
   ```bash
   flow validate
   ```

## 実装計画

### Phase 1: 基本機能

- [ ] ディレクトリスキャン機能
- [ ] ファイル読み込み順序の実装
- [ ] services/ と stages/ のサポート

### Phase 2: エラーハンドリング

- [ ] 未定義サービス参照の検出
- [ ] 名前衝突の警告
- [ ] わかりやすいエラーメッセージ

### Phase 3: 拡張機能

- [ ] variables/ のサポート
- [ ] flow.local.kdl のオーバーライド
- [ ] デバッグモード

### Phase 4: 最適化

- [ ] パフォーマンス改善
- [ ] キャッシュ機構
- [ ] 並列パース

## 参考資料

### 影響を受けた設計

- **Ruby on Rails**: `app/models/`, `app/controllers/` の自動読み込み
- **Next.js**: `pages/` ディレクトリルーティング
- **NestJS**: `@Module` デコレータの自動発見

### 設計哲学

- **Convention over Configuration**: 設定より規約
- **Progressive Disclosure**: 段階的な開示
- **Least Surprise**: 最小驚き原則
