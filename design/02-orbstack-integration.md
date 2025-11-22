# OrbStack連携 - 設計書

最終更新日: 2025-11-22

## 設計思想: Simplicity（シンプルさ）

シンプルなコードを実現するため、以下の原則に従います。

### 型の分類

- **data**: ラベル、コンテナ設定などの値を保持
- **calculations**: ラベル生成、命名規則適用などの計算
- **actions**: Dockerコンテナ作成、起動などの操作

### Straightforward原則

KDL解析 → ラベル生成 → コンテナ設定 → Docker API呼び出し

という直線的なフローでシンプルに実装します。

## 実装仕様

### 1. コンテナ命名

**実装場所**: `crates/fleetflow-container/src/converter.rs`

```rust
pub fn generate_container_name(
    project_name: &str,
    stage_name: &str,
    service_name: &str
) -> String {
    format!("{}-{}-{}", project_name, stage_name, service_name)
}
```

### 2. Dockerラベル生成

**実装場所**: `crates/fleetflow-container/src/converter.rs`

```rust
use std::collections::HashMap;

pub fn generate_labels(
    project_name: &str,
    stage_name: &str,
    service_name: &str
) -> HashMap<String, String> {
    let mut labels = HashMap::new();

    // OrbStackグループ化用ラベル
    labels.insert(
        "com.docker.compose.project".to_string(),
        format!("{}-{}", project_name, stage_name),
    );
    labels.insert(
        "com.docker.compose.service".to_string(),
        service_name.to_string(),
    );

    // FleetFlowメタデータラベル
    labels.insert("fleetflow.project".to_string(), project_name.to_string());
    labels.insert("fleetflow.stage".to_string(), stage_name.to_string());
    labels.insert("fleetflow.service".to_string(), service_name.to_string());

    labels
}
```

### 3. プロジェクト名のパース

**実装場所**: `crates/fleetflow-config/src/parser.rs`

```rust
use kdl::KdlDocument;

pub fn parse_project_name(doc: &KdlDocument) -> Option<String> {
    doc.get("project")
        .and_then(|node| node.entries().first())
        .and_then(|entry| entry.value().as_string())
        .map(|s| s.to_string())
}
```

### 4. Bollard統合

**実装場所**: `crates/fleetflow-container/src/converter.rs`

```rust
use bollard::container::{Config, CreateContainerOptions};
use bollard::models::HostConfig;

pub async fn create_container_with_labels(
    docker: &Docker,
    project_name: &str,
    stage_name: &str,
    service_config: &ServiceConfig,
) -> Result<String> {
    let container_name = generate_container_name(
        project_name,
        stage_name,
        &service_config.name
    );

    let labels = generate_labels(
        project_name,
        stage_name,
        &service_config.name
    );

    let config = Config {
        image: Some(service_config.image.clone()),
        labels: Some(labels),
        host_config: Some(HostConfig {
            port_bindings: service_config.port_bindings.clone(),
            binds: service_config.volumes.clone(),
            ..Default::default()
        }),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: &container_name,
        ..Default::default()
    };

    let response = docker.create_container(Some(options), config).await?;
    Ok(response.id)
}
```

## データモデル

### ラベル構造

```rust
pub struct ContainerLabels {
    // OrbStackグループ化用
    pub compose_project: String,   // "{project}-{stage}"
    pub compose_service: String,   // "{service}"

    // FleetFlowメタデータ
    pub fleetflow_project: String, // "{project}"
    pub fleetflow_stage: String,   // "{stage}"
    pub fleetflow_service: String, // "{service}"
}

impl ContainerLabels {
    pub fn new(project: &str, stage: &str, service: &str) -> Self {
        Self {
            compose_project: format!("{}-{}", project, stage),
            compose_service: service.to_string(),
            fleetflow_project: project.to_string(),
            fleetflow_stage: stage.to_string(),
            fleetflow_service: service.to_string(),
        }
    }

    pub fn to_hashmap(&self) -> HashMap<String, String> {
        let mut labels = HashMap::new();
        labels.insert("com.docker.compose.project".to_string(), self.compose_project.clone());
        labels.insert("com.docker.compose.service".to_string(), self.compose_service.clone());
        labels.insert("fleetflow.project".to_string(), self.fleetflow_project.clone());
        labels.insert("fleetflow.stage".to_string(), self.fleetflow_stage.clone());
        labels.insert("fleetflow.service".to_string(), self.fleetflow_service.clone());
        labels
    }
}
```

## KDL設定例

### 基本的な設定

```kdl
// プロジェクト名を宣言（必須）
project "fleetflow"

stage "local" {
    service "postgres"
    service "redis"
}

service "postgres" {
    image "postgres:16"
    ports {
        port host=5432 container=5432
    }
    env {
        POSTGRES_PASSWORD "postgres"
    }
}

service "redis" {
    image "redis:7-alpine"
    ports {
        port host=6379 container=6379
    }
}
```

生成されるコンテナ：
- 名前: `fleetflow-local-postgres`, `fleetflow-local-redis`
- グループ: `fleetflow-local`

### 複数ステージの設定

```kdl
project "myapp"

stage "local" {
    service "api"
    service "db"
}

stage "dev" {
    service "api"
    service "db"
}

service "api" {
    image "myapp:latest"
    ports {
        port host=8080 container=8080
    }
}

service "db" {
    image "postgres:16"
    ports {
        port host=5432 container=5432
    }
}
```

生成されるグループ：
- `myapp-local`: api, db
- `myapp-dev`: api, db

## エラーハンドリング

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OrbStackError {
    #[error("Project name not found in KDL document")]
    ProjectNameMissing,

    #[error("Stage '{0}' not found")]
    StageNotFound(String),

    #[error("Service '{0}' not defined")]
    ServiceNotDefined(String),

    #[error("Docker error: {0}")]
    DockerError(#[from] bollard::errors::Error),
}
```

## テスト戦略

### ユニットテスト

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_container_name() {
        let name = generate_container_name("myapp", "local", "postgres");
        assert_eq!(name, "myapp-local-postgres");
    }

    #[test]
    fn test_generate_labels() {
        let labels = generate_labels("myapp", "local", "postgres");

        assert_eq!(
            labels.get("com.docker.compose.project"),
            Some(&"myapp-local".to_string())
        );
        assert_eq!(
            labels.get("fleetflow.project"),
            Some(&"myapp".to_string())
        );
    }

    #[test]
    fn test_parse_project_name() {
        let kdl = r#"project "test-project""#;
        let doc = kdl.parse::<KdlDocument>().unwrap();
        let project = parse_project_name(&doc);

        assert_eq!(project, Some("test-project".to_string()));
    }
}
```

### 統合テスト

```rust
#[tokio::test]
async fn test_create_container_with_labels() {
    let docker = Docker::connect_with_local_defaults().unwrap();

    let service_config = ServiceConfig {
        name: "test-service".to_string(),
        image: "alpine:latest".to_string(),
        port_bindings: None,
        volumes: None,
    };

    let container_id = create_container_with_labels(
        &docker,
        "test-project",
        "local",
        &service_config
    ).await.unwrap();

    // ラベルを確認
    let info = docker.inspect_container(&container_id, None).await.unwrap();
    let labels = info.config.unwrap().labels.unwrap();

    assert_eq!(
        labels.get("com.docker.compose.project"),
        Some(&"test-project-local".to_string())
    );

    // クリーンアップ
    docker.remove_container(&container_id, None).await.unwrap();
}
```

## 実装チェックリスト

### Phase 1: MVP機能
- [x] コンテナ命名規則の実装 (`{project}-{stage}-{service}`)
- [x] Dockerラベル生成の実装
  - [x] `com.docker.compose.project` / `com.docker.compose.service`
  - [x] `fleetflow.project` / `fleetflow.stage` / `fleetflow.service`
- [x] KDLパーサーでproject解析
- [x] Bollard統合（コンテナ作成・起動・停止・削除）
- [x] CLIコマンド実装
  - [x] up（コンテナ起動）
  - [x] down（コンテナ停止・削除）
  - [x] ps（コンテナ一覧）
  - [x] logs（ログ表示）
  - [x] validate（設定検証）
- [x] 自動イメージpull機能（Issue #8）
- [x] ユニットテストの追加
  - [x] projectノードのパーステスト (parser/tests.rs) - 3件
  - [x] OrbStackラベル生成のテスト (converter.rs) - 3件
  - [x] 複数ステージ・プロジェクトでのラベルテスト
- [x] ドキュメント更新

### Phase 2: 品質向上
- [ ] エラーハンドリングの強化
  - [ ] ネットワークエラーのリトライ
  - [ ] タイムアウト設定
  - [ ] より詳細なエラーメッセージ
- [ ] 統合テストの追加（実際のDocker環境を使用）
  - [ ] E2Eテスト（up → ps → down）
  - [ ] エラーケーステスト

## 変更履歴

### 2025-11-23: 自動イメージpull機能実装とチェックリスト更新
- **理由**: UX改善とMVP完成状況の記録
- **影響**:
  - main.rs: 自動pull機能実装（parse_image_tag, pull_image関数追加）
  - 設計書: チェックリストを更新してPhase 1 MVP完了を記録
- **コミット**: 22a0cc5

### 2025-11-23: ユニットテスト追加
- **理由**: OrbStack連携機能の品質保証
- **影響**: parser/tests.rs, converter.rsにテスト追加
  - projectノードのパーステスト3件
  - OrbStackラベル生成のテスト3件
  - KDL 2.0構文エラーの修正
- **コミット**: 53c7ad6

### 2025-11-22: ドキュメント構造変更
- **理由**: SDGスキルのフラット構造に対応
- **影響**: docs/design/から design/へ移行
- **コミット**: d80720e

### 2025-11-16: 初版作成
- **理由**: OrbStack連携の実装完了
- **影響**: converter.rs, parser.rsを変更
- **コミット**: bddb58f
