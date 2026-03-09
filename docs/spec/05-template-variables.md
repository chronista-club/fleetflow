# テンプレートと変数展開 - 仕様書

## コンセプト

**"繰り返しを書かない。変数で表現する。"**

FleetFlowは、[Tera](https://tera.netlify.app/)テンプレートエンジンを使用して、強力な変数展開とテンプレート機能を提供します。
設定の重複を排除し、環境ごとの差分を明確にします。

## 哲学

### DRY（Don't Repeat Yourself）

```kdl
// ❌ Bad: 同じバージョンを何度も書く
service "api" {
    image "myapp:1.0.0"
}

service "worker" {
    image "myapp:1.0.0"
}

service "scheduler" {
    image "myapp:1.0.0"
}

// ✅ Good: 変数で一元管理
variables {
    app_version "1.0.0"
}

service "api" {
    image "myapp:{{ app_version }}"
}

service "worker" {
    image "myapp:{{ app_version }}"
}

service "scheduler" {
    image "myapp:{{ app_version }}"
}
```

## Teraを選んだ理由

| 特徴 | 説明 |
|------|------|
| **Rust製** | Rustで書かれており、パフォーマンスと型安全性が高い |
| **Jinja2互換** | Python/Ansible等で広く使われるJinja2の構文に類似 |
| **豊富な機能** | フィルター、マクロ、継承など強力な機能 |
| **学習コスト** | 既存のテンプレート言語と似ており学びやすい |

## 基本機能

### 変数定義と展開

#### 1. 基本的な変数展開

```kdl
// fleet.kdl
variables {
    app_version "1.0.0"
    registry "ghcr.io/myorg"
    node_version "20"
}

service "api" {
    image "{{ registry }}/api:{{ app_version }}"
    
    environment {
        NODE_VERSION "{{ node_version }}"
    }
}
```

**展開後**:

```kdl
service "api" {
    image "ghcr.io/myorg/api:1.0.0"
    
    environment {
        NODE_VERSION "20"
    }
}
```

#### 2. 環境変数からの読み込み

```kdl
variables {
    // 環境変数から値を取得
    app_version env("APP_VERSION", default="1.0.0")
    database_password env("DB_PASSWORD")  // defaultなし = 必須
    api_port env("API_PORT", default="8080")
}

service "api" {
    ports {
        port {{ api_port }} 3000
    }
}
```

#### 3. ネストした変数

```kdl
variables {
    project "myapp"
    environment "production"
    
    // 変数を組み合わせ
    full_name "{{ project }}-{{ environment }}"
    image_tag "{{ project }}:{{ environment }}"
}

service "api" {
    image "{{ image_tag }}"
}
```

**展開後**:

```kdl
service "api" {
    image "myapp:production"
}
```

### Teraのフィルター機能

#### 1. 文字列操作

```kdl
variables {
    project "MyApp"
    env "PRODUCTION"
}

service "api" {
    // lower: 小文字に変換
    image "{{ project | lower }}:{{ env | lower }}"
    
    // upper: 大文字に変換
    environment {
        ENV_NAME "{{ env | upper }}"
    }
}
```

**展開後**:

```kdl
service "api" {
    image "myapp:production"
    
    environment {
        ENV_NAME "PRODUCTION"
    }
}
```

#### 2. デフォルト値

```kdl
variables {
    custom_port ""
}

service "api" {
    ports {
        // default: 変数が空の場合にデフォルト値を使用
        port {{ custom_port | default(value="8080") }} 3000
    }
}
```

#### 3. 条件分岐

```kdl
variables {
    is_production true
    debug_mode false
}

service "api" {
    environment {
        // if: 条件による値の切り替え
        LOG_LEVEL "{{ is_production | ternary(true='warn', false='debug') }}"
        DEBUG "{{ debug_mode }}"
    }
}
```

#### 4. リスト操作

```kdl
variables {
    services ["api", "worker", "scheduler"]
}

stage "live" {
    // for: リストをループ
    {% for service in services %}
    service "{{ service }}"
    {% endfor %}
}
```

**展開後**:

```kdl
stage "live" {
    service "api"
    service "worker"
    service "scheduler"
}
```

### 条件分岐

#### 1. if文

```kdl
variables {
    enable_worker true
    enable_scheduler false
}

stage "live" {
    service "api"

    {% if enable_worker %}
    service "worker"
    {% endif %}

    {% if enable_scheduler %}
    service "scheduler"
    {% endif %}
}
```

**展開後**:

```kdl
stage "live" {
    service "api"
    service "worker"
}
```

#### 2. if-else

```kdl
variables {
    environment "production"
}

service "api" {
    environment {
        {% if environment == "production" %}
        DEBUG "false"
        LOG_LEVEL "warn"
        {% else %}
        DEBUG "true"
        LOG_LEVEL "debug"
        {% endif %}
    }
}
```

#### 3. if-elif-else

```kdl
variables {
    environment "pre"
}

service "api" {
    {% if environment == "live" %}
    replicas 3
    {% elif environment == "pre" %}
    replicas 2
    {% else %}
    replicas 1
    {% endif %}
}
```

### マクロ（再利用可能なテンプレート）

```kdl
// マクロ定義
{% macro database(name, version, port) %}
service "{{ name }}" {
    version "{{ version }}"
    ports {
        port {{ port }} {{ port }}
    }
    volumes {
        volume "./data/{{ name }}" "/var/lib/{{ name }}/data"
    }
}
{% endmacro %}

// マクロ使用
{{ database(name="postgres", version="16", port="5432") }}
{{ database(name="mysql", version="8", port="3306") }}
```

**展開後**:

```kdl
service "postgres" {
    version "16"
    ports {
        port 5432 5432
    }
    volumes {
        volume "./data/postgres" "/var/lib/postgres/data"
    }
}

service "mysql" {
    version "8"
    ports {
        port 3306 3306
    }
    volumes {
        volume "./data/mysql" "/var/lib/mysql/data"
    }
}
```

## 高度な機能

### 1. 変数のスコープ

```kdl
// グローバル変数（fleet.kdl）
variables {
    global_version "1.0.0"
}

// ステージ固有の変数（stages/local.kdl）
stage "local" {
    variables {
        debug "true"           // このステージのみ有効
        port "8080"            // このステージのみ有効
    }
    
    service "api"
}

// サービス定義で両方の変数が使える
service "api" {
    image "myapp:{{ global_version }}"
    
    environment {
        DEBUG "{{ debug }}"
        PORT "{{ port }}"
    }
}
```

**変数の優先順位**:

```
1. ステージ固有の変数（stage内のvariables）
2. サービスファイル内の変数
3. グローバル変数（fleet.kdl内のvariables）
4. 環境変数
```

### 2. インクルードとテンプレート

```kdl
// variables/common.kdl
{% set registry = "ghcr.io/myorg" %}
{% set app_version = "1.0.0" %}

// services/api.kdl（commonを使用）
service "api" {
    image "{{ registry }}/api:{{ app_version }}"
}
```

### 3. 計算式

```kdl
variables {
    base_port 8000
    service_count 3
}

{% for i in range(end=service_count) %}
service "api-{{ i }}" {
    ports {
        port {{ base_port + i }} 3000
    }
}
{% endfor %}
```

**展開後**:

```kdl
service "api-0" {
    ports {
        port 8000 3000
    }
}

service "api-1" {
    ports {
        port 8001 3000
    }
}

service "api-2" {
    ports {
        port 8002 3000
    }
}
```

## プロジェクト構造

### 推奨ディレクトリレイアウト

```
project/
├── fleet.kdl              # グローバル設定と変数
│
├── variables/            # 変数定義を分離
│   ├── common.kdl        # 共通変数
│   ├── live.kdl          # ライブ環境用変数
│   └── development.kdl   # 開発環境用変数
│
├── services/             # サービス定義（テンプレート使用）
│   ├── api.kdl
│   └── database.kdl
│
└── stages/               # ステージ定義（テンプレート使用）
    ├── local.kdl
    └── live.kdl
```

### 例: 環境ごとの変数管理

```kdl
// variables/common.kdl
variables {
    app_version "1.0.0"
    registry "ghcr.io/myorg"
    node_version "20"
}

// variables/development.kdl
variables {
    debug "true"
    log_level "debug"
    replicas 1
}

// variables/live.kdl
variables {
    debug "false"
    log_level "warn"
    replicas 3
}

// services/api.kdl
service "api" {
    image "{{ registry }}/api:{{ app_version }}"
    replicas {{ replicas }}
    
    environment {
        DEBUG "{{ debug }}"
        LOG_LEVEL "{{ log_level }}"
    }
}
```

## 実装仕様

### FR-001: テンプレート処理フロー

**目的**: KDLファイルをパースする前にTeraでテンプレート展開

**処理順序**:

```
1. ファイル発見（自動インポート）
   ↓
2. 変数定義の収集
   ↓
3. Teraコンテキストの構築
   ↓
4. 各ファイルをTeraで展開
   ↓
5. 展開後のKDLをパース
   ↓
6. FlowConfigの構築
```

**疑似コード**:

```rust
fn parse_with_template(project_root: PathBuf) -> Result<FlowConfig> {
    // 1. ファイル発見
    let files = discover_files(&project_root)?;
    
    // 2. 変数収集
    let mut context = Context::new();
    collect_variables(&files, &mut context)?;
    
    // 3. 環境変数を追加
    add_env_vars(&mut context)?;
    
    // 4. Teraインスタンス作成
    let tera = Tera::default();
    
    // 5. 各ファイルをテンプレート展開
    let mut expanded_content = String::new();
    for file in files {
        let content = fs::read_to_string(file)?;
        let rendered = tera.render_str(&content, &context)?;
        expanded_content.push_str(&rendered);
    }
    
    // 6. 展開後のKDLをパース
    parse_kdl_string(&expanded_content)
}
```

### FR-002: 変数の収集

**目的**: 複数ファイルから変数を収集し、優先順位に従って統合

**アルゴリズム**:

```
1. fleet.kdl のグローバル変数を収集
2. variables/**/*.kdl の変数を収集
3. 環境変数を env() 関数として登録
4. ステージ固有の変数は後で上書き
```

### FR-003: エラーハンドリング

#### エラーケース1: 未定義変数

```kdl
service "api" {
    image "{{ undefined_var }}"
}
```

**エラーメッセージ**:

```
✗ Error: 未定義の変数
  ファイル: services/api.kdl:2
  
  変数 'undefined_var' が定義されていません
  
  解決方法:
    1. fleet.kdl に変数を定義:
       variables {
           undefined_var "value"
       }
    
    2. または環境変数を設定:
       export UNDEFINED_VAR="value"
```

#### エラーケース2: テンプレート構文エラー

```kdl
service "api" {
    {% if is_prod  // endif がない
    replicas 3
}
```

**エラーメッセージ**:

```
✗ Error: テンプレート構文エラー
  ファイル: services/api.kdl:2
  
  {% if %} ブロックが閉じられていません
  
  2 |     {% if is_prod
     |     ^^^^^^^^^^^^^ ここで開始
  
  {% endif %} を追加してください
```

### FR-004: デバッグモード

**コマンド**:

```bash
fleet validate --debug-template
```

**出力**:

```
🔍 変数収集
  グローバル変数:
    app_version = "1.0.0"
    registry = "ghcr.io/myorg"
  
  環境変数:
    APP_VERSION = "1.0.0"
    DEBUG = "true"

📝 テンプレート展開
  services/api.kdl:
    展開前: image "{{ registry }}/api:{{ app_version }}"
    展開後: image "ghcr.io/myorg/api:1.0.0"

✅ 展開完了
```

## 使用例

### ユースケース1: マイクロサービスのバージョン統一

```kdl
// fleet.kdl
variables {
    app_version "1.2.3"
    registry "ghcr.io/myorg"
}

// services/*.kdl
service "api" {
    image "{{ registry }}/api:{{ app_version }}"
}

service "worker" {
    image "{{ registry }}/worker:{{ app_version }}"
}

service "scheduler" {
    image "{{ registry }}/scheduler:{{ app_version }}"
}
```

### ユースケース2: 環境ごとのレプリカ数

```kdl
// fleet.kdl
variables {
    environment env("ENV", default="local")
    replicas_map {
        local 1
        pre 2
        live 5
    }
}

service "api" {
    {% if environment == "local" %}
    replicas 1
    {% elif environment == "pre" %}
    replicas 2
    {% else %}
    replicas 5
    {% endif %}
}
```

### ユースケース3: 動的ポート割り当て

```kdl
variables {
    services ["api", "worker", "scheduler"]
    base_port 8000
}

{% for service in services %}
service "{{ service }}" {
    ports {
        port {{ base_port + loop.index0 }} 3000
    }
}
{% endfor %}
```

## 実装計画

### Phase 1: 基本機能

- [ ] Teraの統合
- [ ] 変数定義のパース
- [ ] 基本的な変数展開
- [ ] 環境変数からの読み込み

### Phase 2: 高度な機能

- [ ] if/for などの制御構文
- [ ] フィルター機能
- [ ] マクロ機能
- [ ] ネストした変数

### Phase 3: エラーハンドリング

- [ ] 未定義変数の検出
- [ ] 構文エラーの詳細表示
- [ ] デバッグモード

### Phase 4: 最適化

- [ ] テンプレートキャッシュ
- [ ] 変数解決の最適化
- [ ] パフォーマンス改善

## 依存関係

### Cargo.toml

```toml
[dependencies]
tera = "1.19"
```

## 参考資料

- [Tera Documentation](https://tera.netlify.app/)
- [Jinja2 Documentation](https://jinja.palletsprojects.com/)
- [Ansible Templates](https://docs.ansible.com/ansible/latest/user_guide/playbooks_templating.html)
