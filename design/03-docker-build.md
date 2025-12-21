# 設計書: Dockerイメージビルド機能

**Issue**: #10
**関連仕様**: `spec/07-docker-build.md`
**作成日**: 2025-11-23
**ステータス**: Phase 3 - 設計中

## How - どう実現するか

### アーキテクチャ概要

```
┌─────────────────┐
│   flow.kdl      │
└────────┬────────┘
         │ parse
         ▼
┌─────────────────┐
│  FlowConfig     │
│  └─ Service     │
│     ├─ dockerfile?
│     ├─ context?
│     ├─ build_args?
│     └─ target?
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  BuildResolver  │  ← 新規モジュール
│  ・Dockerfile検出
│  ・変数展開
│  ・タグ生成
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  ImageBuilder   │  ← 新規モジュール
│  ・Bollard API
│  ・ビルド実行
│  ・進捗表示
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Docker Daemon   │
└─────────────────┘
```

### モジュール構成

```
fleetflow/
├── crates/
│   ├── fleetflow-atom/
│   │   ├── src/
│   │   │   ├── model.rs          # Serviceモデルにbuild関連フィールド追加
│   │   │   └── parser.rs         # buildブロックのパース処理追加
│   │   └── Cargo.toml
│   │
│   ├── fleetflow-build/          # 新規クレート
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── resolver.rs       # Dockerfile検出、変数展開
│   │   │   ├── builder.rs        # ビルド実行
│   │   │   ├── context.rs        # ビルドコンテキスト作成
│   │   │   ├── progress.rs       # 進捗表示
│   │   │   └── error.rs          # エラー型定義
│   │   ├── tests/
│   │   │   ├── resolver_tests.rs
│   │   │   └── builder_tests.rs
│   │   └── Cargo.toml
│   │
│   ├── fleetflow/
│   │   ├── src/
│   │   │   ├── main.rs           # rebuild, buildコマンド追加
│   │   │   └── commands/         # 新規：コマンド分離
│   │   │       ├── mod.rs
│   │   │       ├── up.rs
│   │   │       ├── down.rs
│   │   │       ├── build.rs      # 新規
│   │   │       └── rebuild.rs    # 新規
│   │   └── Cargo.toml
│   │
│   └── fleetflow-container/
│       └── src/
│           └── converter.rs      # イメージ名解決ロジックの調整
```

## データモデル拡張

### 1. Service構造体の拡張

**ファイル**: `crates/fleetflow-atom/src/model.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Service {
    // 既存フィールド
    pub image: Option<String>,
    pub version: Option<String>,
    pub command: Option<String>,
    pub ports: Vec<Port>,
    pub volumes: Vec<Volume>,
    #[serde(default)]
    pub environment: HashMap<String, String>,

    // 新規フィールド（ビルド関連）
    #[serde(default)]
    pub build: Option<BuildConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    /// Dockerfileのパス（プロジェクトルートからの相対パス）
    pub dockerfile: Option<PathBuf>,

    /// ビルドコンテキストのパス（プロジェクトルートからの相対パス）
    /// 未指定の場合はプロジェクトルート
    pub context: Option<PathBuf>,

    /// ビルド引数
    #[serde(default)]
    pub args: HashMap<String, String>,

    /// マルチステージビルドのターゲット
    pub target: Option<String>,

    /// キャッシュ無効化フラグ
    #[serde(default)]
    pub no_cache: bool,

    /// イメージタグの明示的指定
    pub image_tag: Option<String>,
}
```

### 2. KDLパーサーの拡張

**ファイル**: `crates/fleetflow-atom/src/parser.rs`

```rust
fn parse_service(node: &KdlNode) -> Option<Service> {
    let mut service = Service::default();

    if let Some(children) = node.children() {
        for child in children.nodes() {
            match child.name().value() {
                // 既存の処理...

                "dockerfile" => {
                    if let Some(path) = child.entries().first()
                        .and_then(|e| e.value().as_string())
                    {
                        service.build.get_or_insert_with(Default::default)
                            .dockerfile = Some(PathBuf::from(path));
                    }
                }

                "context" => {
                    if let Some(path) = child.entries().first()
                        .and_then(|e| e.value().as_string())
                    {
                        service.build.get_or_insert_with(Default::default)
                            .context = Some(PathBuf::from(path));
                    }
                }

                "target" => {
                    if let Some(target) = child.entries().first()
                        .and_then(|e| e.value().as_string())
                    {
                        service.build.get_or_insert_with(Default::default)
                            .target = Some(target.to_string());
                    }
                }

                "build_args" => {
                    if let Some(args_children) = child.children() {
                        let build_config = service.build
                            .get_or_insert_with(Default::default);

                        for arg_node in args_children.nodes() {
                            let key = arg_node.name().value().to_string();
                            let value = arg_node.entries().first()
                                .and_then(|e| e.value().as_string())
                                .unwrap_or("")
                                .to_string();
                            build_config.args.insert(key, value);
                        }
                    }
                }

                "build" => {
                    // ネストしたbuildブロック対応
                    if let Some(build_children) = child.children() {
                        service.build = Some(parse_build_config(build_children));
                    }
                }

                _ => {}
            }
        }
    }

    Some(service)
}

fn parse_build_config(nodes: &KdlDocument) -> BuildConfig {
    let mut config = BuildConfig::default();

    for node in nodes.nodes() {
        match node.name().value() {
            "dockerfile" => {
                if let Some(path) = node.entries().first()
                    .and_then(|e| e.value().as_string())
                {
                    config.dockerfile = Some(PathBuf::from(path));
                }
            }
            "context" => {
                if let Some(path) = node.entries().first()
                    .and_then(|e| e.value().as_string())
                {
                    config.context = Some(PathBuf::from(path));
                }
            }
            "args" => {
                if let Some(args_children) = node.children() {
                    for arg_node in args_children.nodes() {
                        let key = arg_node.name().value().to_string();
                        let value = arg_node.entries().first()
                            .and_then(|e| e.value().as_string())
                            .unwrap_or("")
                            .to_string();
                        config.args.insert(key, value);
                    }
                }
            }
            "target" => {
                if let Some(target) = node.entries().first()
                    .and_then(|e| e.value().as_string())
                {
                    config.target = Some(target.to_string());
                }
            }
            "no_cache" => {
                if let Some(value) = node.entries().first()
                    .and_then(|e| e.value().as_bool())
                {
                    config.no_cache = value;
                }
            }
            "image_tag" => {
                if let Some(tag) = node.entries().first()
                    .and_then(|e| e.value().as_string())
                {
                    config.image_tag = Some(tag.to_string());
                }
            }
            _ => {}
        }
    }

    config
}
```

### KDL記法の両サポート

#### フラット記法（推奨）

```kdl
service "api" {
    dockerfile "./services/api/Dockerfile"
    context "."

    build_args {
        NODE_VERSION "20"
        APP_ENV "production"
    }

    target "production"
}
```

#### ネスト記法

```kdl
service "api" {
    build {
        dockerfile "./services/api/Dockerfile"
        context "."

        args {
            NODE_VERSION "20"
            APP_ENV "production"
        }

        target "production"
        no_cache false
    }
}
```

どちらの記法も同じ`BuildConfig`構造体にパースされます。

## 新規クレート: fleetflow-build

### 1. resolver.rs - Dockerfile検出と変数展開

```rust
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use fleetflow_atom::{Service, Flow};

pub struct BuildResolver {
    project_root: PathBuf,
}

impl BuildResolver {
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    /// Dockerfileのパスを解決
    pub fn resolve_dockerfile(
        &self,
        service_name: &str,
        service: &Service,
    ) -> Result<Option<PathBuf>, ResolverError> {
        // 明示的な指定がある場合
        if let Some(build) = &service.build {
            if let Some(dockerfile) = &build.dockerfile {
                let path = self.project_root.join(dockerfile);
                if path.exists() {
                    return Ok(Some(path));
                } else {
                    return Err(ResolverError::DockerfileNotFound(path));
                }
            }
        }

        // 規約ベースの検索
        let candidates = vec![
            format!("services/{}/Dockerfile", service_name),
            format!("{}/Dockerfile", service_name),
            format!("Dockerfile.{}", service_name),
        ];

        for candidate in candidates {
            let path = self.project_root.join(&candidate);
            if path.exists() {
                return Ok(Some(path));
            }
        }

        // Dockerfileが見つからない場合はNone（pullで対応）
        Ok(None)
    }

    /// ビルドコンテキストのパスを解決
    pub fn resolve_context(
        &self,
        service: &Service,
    ) -> PathBuf {
        if let Some(build) = &service.build {
            if let Some(context) = &build.context {
                return self.project_root.join(context);
            }
        }

        // デフォルトはプロジェクトルート
        self.project_root.clone()
    }

    /// ビルド引数の変数展開
    pub fn resolve_build_args(
        &self,
        service: &Service,
        variables: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        let mut resolved_args = HashMap::new();

        if let Some(build) = &service.build {
            for (key, value) in &build.args {
                // 変数展開: {VAR_NAME} → 実際の値
                let resolved_value = self.expand_variables(value, variables);
                resolved_args.insert(key.clone(), resolved_value);
            }
        }

        resolved_args
    }

    /// 変数展開処理（既存のtemplate.rsから移植）
    fn expand_variables(
        &self,
        template: &str,
        variables: &HashMap<String, String>,
    ) -> String {
        let mut result = template.to_string();

        for (key, value) in variables {
            let placeholder = format!("{{{}}}", key);
            result = result.replace(&placeholder, value);
        }

        result
    }

    /// イメージタグの解決
    pub fn resolve_image_tag(
        &self,
        service_name: &str,
        service: &Service,
        project_name: &str,
        stage_name: &str,
    ) -> String {
        // 明示的なタグ指定
        if let Some(build) = &service.build {
            if let Some(tag) = &build.image_tag {
                return tag.clone();
            }
        }

        // 自動生成タグ: {project}-{service}:{stage}
        format!("{}−{}:{}", project_name, service_name, stage_name)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResolverError {
    #[error("Dockerfile not found: {0}")]
    DockerfileNotFound(PathBuf),

    #[error("Variable '{0}' not found")]
    VariableNotFound(String),
}
```

### 2. context.rs - ビルドコンテキスト作成

```rust
use std::path::Path;
use std::io::Read;
use tar::Builder;
use flate2::Compression;
use flate2::write::GzEncoder;

pub struct ContextBuilder;

impl ContextBuilder {
    /// ビルドコンテキストをtar.gzアーカイブとして作成
    pub fn create_context(
        context_path: &Path,
        dockerfile_path: &Path,
    ) -> Result<Vec<u8>, ContextError> {
        // .dockerignoreの読み込み
        let ignore = Self::load_dockerignore(context_path)?;

        // tarアーカイブの作成
        let mut archive_data = Vec::new();
        {
            let encoder = GzEncoder::new(&mut archive_data, Compression::default());
            let mut tar = Builder::new(encoder);

            // コンテキストディレクトリを再帰的に追加
            tar.append_dir_all(".", context_path)?;

            // Dockerfileを "Dockerfile" として追加
            let mut dockerfile_file = std::fs::File::open(dockerfile_path)?;
            let mut dockerfile_content = Vec::new();
            dockerfile_file.read_to_end(&mut dockerfile_content)?;

            let mut header = tar::Header::new_gnu();
            header.set_path("Dockerfile")?;
            header.set_size(dockerfile_content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();

            tar.append(&header, &dockerfile_content[..])?;

            tar.finish()?;
        }

        Ok(archive_data)
    }

    /// .dockerignoreファイルの読み込み
    fn load_dockerignore(context_path: &Path) -> Result<Option<String>, ContextError> {
        let dockerignore_path = context_path.join(".dockerignore");

        if dockerignore_path.exists() {
            let content = std::fs::read_to_string(&dockerignore_path)?;
            Ok(Some(content))
        } else {
            Ok(None)
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ContextError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to create tar archive")]
    TarError,
}
```

### 3. builder.rs - Bollard APIを使ったビルド実行

```rust
use bollard::Docker;
use bollard::image::BuildImageOptions;
use futures_util::stream::StreamExt;
use std::collections::HashMap;
use std::path::Path;

pub struct ImageBuilder {
    docker: Docker,
}

impl ImageBuilder {
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }

    /// イメージをビルド
    pub async fn build_image(
        &self,
        context_data: Vec<u8>,
        tag: &str,
        build_args: HashMap<String, String>,
        target: Option<&str>,
        no_cache: bool,
    ) -> Result<(), BuildError> {
        // ビルドオプションの設定
        let options = BuildImageOptions {
            dockerfile: "Dockerfile",
            t: tag,
            buildargs: build_args,
            target: target.unwrap_or(""),
            nocache: no_cache,
            rm: true,  // 中間コンテナを削除
            forcerm: true,  // ビルド失敗時も中間コンテナを削除
            pull: true,  // ベースイメージを常にpull
            ..Default::default()
        };

        // ビルドストリームの開始
        let mut stream = self.docker.build_image(
            options,
            None,
            Some(context_data.into()),
        );

        // ビルド進捗の表示
        while let Some(msg) = stream.next().await {
            match msg {
                Ok(output) => {
                    self.handle_build_output(output)?;
                }
                Err(e) => {
                    return Err(BuildError::DockerError(e));
                }
            }
        }

        Ok(())
    }

    /// ビルド出力の処理
    fn handle_build_output(
        &self,
        output: bollard::models::BuildInfo,
    ) -> Result<(), BuildError> {
        use colored::Colorize;

        if let Some(stream) = output.stream {
            print!("{}", stream);
        }

        if let Some(error) = output.error {
            return Err(BuildError::BuildFailed(error));
        }

        if let Some(status) = output.status {
            println!("{}", status.cyan());
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("Docker error: {0}")]
    DockerError(#[from] bollard::errors::Error),

    #[error("Build failed: {0}")]
    BuildFailed(String),
}
```

### 4. progress.rs - 進捗表示

```rust
use indicatif::{ProgressBar, ProgressStyle};

pub struct BuildProgress {
    progress_bar: ProgressBar,
}

impl BuildProgress {
    pub fn new(service_name: &str) -> Self {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} [{elapsed_precise}] {msg}")
                .unwrap(),
        );
        pb.set_message(format!("Building {}...", service_name));

        Self { progress_bar: pb }
    }

    pub fn set_message(&self, msg: &str) {
        self.progress_bar.set_message(msg.to_string());
    }

    pub fn finish(&self) {
        self.progress_bar.finish_with_message("Build completed");
    }

    pub fn fail(&self, error: &str) {
        self.progress_bar.finish_with_message(format!("Build failed: {}", error));
    }
}
```

## CLIコマンド実装

### 1. コマンド構造のリファクタリング

**ファイル**: `crates/fleetflow/src/commands/mod.rs`

```rust
pub mod up;
pub mod down;
pub mod build;
pub mod rebuild;
pub mod ps;
pub mod logs;
pub mod validate;

pub use up::execute_up;
pub use down::execute_down;
pub use build::execute_build;
pub use rebuild::execute_rebuild;
pub use ps::execute_ps;
pub use logs::execute_logs;
pub use validate::execute_validate;
```

### 2. buildコマンド

**ファイル**: `crates/fleetflow/src/commands/build.rs`

```rust
use bollard::Docker;
use fleetflow_atom::Flow;
use fleetflow_build::{BuildResolver, ContextBuilder, ImageBuilder};
use fleetflow_config::find_flow_file;
use std::path::PathBuf;

pub async fn execute_build(
    service: Option<String>,
    stage: String,
    no_cache: bool,
) -> anyhow::Result<()> {
    // 設定ファイルの読み込み
    let config_path = find_flow_file()?;
    let project_root = config_path.parent().unwrap().to_path_buf();
    let config_content = std::fs::read_to_string(&config_path)?;
    let flow: Flow = fleetflow_atom::parse(&config_content)?;

    // Dockerクライアントの初期化
    let docker = Docker::connect_with_local_defaults()?;

    // Resolverの初期化
    let resolver = BuildResolver::new(project_root.clone());

    // ステージのサービス一覧を取得
    let stage_config = flow.stages.get(&stage)
        .ok_or_else(|| anyhow::anyhow!("Stage '{}' not found", stage))?;

    let services_to_build: Vec<String> = if let Some(svc) = service {
        vec![svc]
    } else {
        stage_config.services.clone()
    };

    // 変数の解決（グローバル + ステージ）
    let mut variables = flow.variables.clone().unwrap_or_default();
    if let Some(stage_vars) = &stage_config.variables {
        variables.extend(stage_vars.clone());
    }

    // 各サービスのビルド
    for service_name in services_to_build {
        let service = flow.services.get(&service_name)
            .ok_or_else(|| anyhow::anyhow!("Service '{}' not found", service_name))?;

        // Dockerfileの検出
        let dockerfile_path = resolver.resolve_dockerfile(&service_name, service)?;

        if let Some(dockerfile) = dockerfile_path {
            println!("Building service: {}", service_name);
            println!("  Dockerfile: {}", dockerfile.display());

            // ビルドコンテキストの解決
            let context_path = resolver.resolve_context(service);
            println!("  Context: {}", context_path.display());

            // ビルド引数の解決
            let build_args = resolver.resolve_build_args(service, &variables);

            // イメージタグの解決
            let image_tag = resolver.resolve_image_tag(
                &service_name,
                service,
                &flow.name,
                &stage,
            );
            println!("  Tag: {}", image_tag);

            // ビルドコンテキストの作成
            let context_data = ContextBuilder::create_context(&context_path, &dockerfile)?;

            // ターゲットの取得
            let target = service.build.as_ref()
                .and_then(|b| b.target.as_deref());

            // キャッシュ設定
            let use_no_cache = no_cache || service.build.as_ref()
                .map(|b| b.no_cache)
                .unwrap_or(false);

            // ビルド実行
            let builder = ImageBuilder::new(docker.clone());
            builder.build_image(
                context_data,
                &image_tag,
                build_args,
                target,
                use_no_cache,
            ).await?;

            println!("✓ Successfully built: {}", image_tag);
        } else {
            println!("No Dockerfile found for service '{}', skipping build", service_name);
        }
    }

    Ok(())
}
```

### 3. rebuildコマンド

**ファイル**: `crates/fleetflow/src/commands/rebuild.rs`

```rust
pub async fn execute_rebuild(
    service: String,
    stage: Option<String>,
    no_cache: bool,
) -> anyhow::Result<()> {
    let stage = stage.unwrap_or_else(|| "local".to_string());

    // コンテナが起動中であれば停止
    let docker = Docker::connect_with_local_defaults()?;
    let container_name = format!("{}-{}-{}", project_name, &stage, &service);

    // コンテナ情報の取得
    match docker.inspect_container(&container_name, None).await {
        Ok(info) => {
            if let Some(state) = info.state {
                if state.running.unwrap_or(false) {
                    println!("Stopping container: {}", container_name);
                    docker.stop_container(&container_name, None).await?;
                }
            }

            // コンテナ削除
            println!("Removing container: {}", container_name);
            docker.remove_container(&container_name, None).await?;
        }
        Err(_) => {
            // コンテナが存在しない場合は無視
        }
    }

    // ビルド実行
    execute_build(Some(service.clone()), stage.clone(), no_cache).await?;

    // コンテナ再作成・起動
    println!("Starting container: {}", container_name);
    // upコマンドのロジックを再利用
    execute_up(stage, false).await?;

    Ok(())
}
```

### 4. upコマンドの拡張

**ファイル**: `crates/fleetflow/src/commands/up.rs`

```rust
pub async fn execute_up(
    stage: String,
    build: bool,  // --buildフラグ
) -> anyhow::Result<()> {
    // ... 既存の処理 ...

    for service_name in &stage_config.services {
        let service = flow.services.get(service_name)
            .ok_or_else(|| anyhow::anyhow!("Service '{}' not found", service_name))?;

        // イメージの解決
        let image_name = if service.build.is_some() {
            // ビルド対象のサービス
            let resolver = BuildResolver::new(project_root.clone());
            let image_tag = resolver.resolve_image_tag(
                service_name,
                service,
                &flow.name,
                &stage,
            );

            // ビルドが必要かチェック
            let needs_build = build ||  // --buildフラグが指定されている
                              !image_exists(&docker, &image_tag).await?;  // イメージが存在しない

            if needs_build {
                // ビルド実行
                execute_build(Some(service_name.clone()), stage.clone(), false).await?;
            }

            image_tag
        } else {
            // 既存イメージを使用
            format!(
                "{}:{}",
                service.image.as_ref().unwrap_or(&service_name),
                service.version.as_ref().unwrap_or(&"latest".to_string())
            )
        };

        // コンテナ作成・起動（既存ロジック）
        // ...
    }

    Ok(())
}

/// イメージの存在確認
async fn image_exists(docker: &Docker, image_tag: &str) -> anyhow::Result<bool> {
    match docker.inspect_image(image_tag).await {
        Ok(_) => Ok(true),
        Err(bollard::errors::Error::DockerResponseServerError {
            status_code: 404,
            ..
        }) => Ok(false),
        Err(e) => Err(e.into()),
    }
}
```

## 依存関係

### Cargo.toml

**fleetflow-build/Cargo.toml**:

```toml
[package]
name = "fleetflow-build"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "Docker image build functionality for FleetFlow"

[dependencies]
fleetflow-atom.workspace = true
bollard.workspace = true
tokio.workspace = true
futures-util.workspace = true
anyhow.workspace = true
thiserror.workspace = true
tar = "0.4"
flate2 = "1.0"
colored = "2.1"
indicatif = "0.17"
tracing.workspace = true

[dev-dependencies]
tempfile.workspace = true
```

**ワークスペースCargo.toml**（ルート）:

```toml
[workspace]
members = [
    "crates/fleetflow",
    "crates/fleetflow-atom",
    "crates/fleetflow-config",
    "crates/fleetflow-container",
    "crates/fleetflow-build",  # 追加
]

[workspace.dependencies]
# ... 既存の依存関係 ...

# fleetflow-build用の新しい依存関係
tar = "0.4"
flate2 = "1.0"
colored = "2.1"
indicatif = "0.17"
```

## テスト戦略

### 1. ユニットテスト

#### Resolver テスト

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_resolve_dockerfile_explicit() {
        let temp_dir = tempdir().unwrap();
        let dockerfile_path = temp_dir.path().join("custom.dockerfile");
        std::fs::write(&dockerfile_path, "FROM alpine").unwrap();

        let resolver = BuildResolver::new(temp_dir.path().to_path_buf());

        let mut service = Service::default();
        service.build = Some(BuildConfig {
            dockerfile: Some(PathBuf::from("custom.dockerfile")),
            ..Default::default()
        });

        let result = resolver.resolve_dockerfile("test", &service).unwrap();
        assert_eq!(result, Some(dockerfile_path));
    }

    #[test]
    fn test_resolve_dockerfile_convention() {
        let temp_dir = tempdir().unwrap();
        let services_dir = temp_dir.path().join("services/api");
        std::fs::create_dir_all(&services_dir).unwrap();

        let dockerfile_path = services_dir.join("Dockerfile");
        std::fs::write(&dockerfile_path, "FROM alpine").unwrap();

        let resolver = BuildResolver::new(temp_dir.path().to_path_buf());

        let service = Service::default();

        let result = resolver.resolve_dockerfile("api", &service).unwrap();
        assert_eq!(result, Some(dockerfile_path));
    }

    #[test]
    fn test_expand_variables() {
        let resolver = BuildResolver::new(PathBuf::from("/tmp"));

        let mut variables = HashMap::new();
        variables.insert("NODE_VERSION".to_string(), "20".to_string());
        variables.insert("REGISTRY".to_string(), "ghcr.io/myorg".to_string());

        let template = "{REGISTRY}/app:node{NODE_VERSION}";
        let result = resolver.expand_variables(template, &variables);

        assert_eq!(result, "ghcr.io/myorg/app:node20");
    }
}
```

### 2. 統合テスト

```rust
#[tokio::test]
async fn test_full_build_workflow() {
    // テスト用のプロジェクト構造を作成
    let temp_dir = tempdir().unwrap();
    let services_dir = temp_dir.path().join("services/test-app");
    std::fs::create_dir_all(&services_dir).unwrap();

    // Dockerfileを作成
    std::fs::write(
        services_dir.join("Dockerfile"),
        r#"
FROM alpine:latest
ARG APP_ENV
ENV APP_ENV=${APP_ENV}
CMD echo "Hello from ${APP_ENV}"
        "#,
    ).unwrap();

    // flow.kdlを作成
    std::fs::write(
        temp_dir.path().join("flow.kdl"),
        r#"
project "test"

variables {
    APP_ENV "test"
}

stage "local" {
    service "test-app"
}

service "test-app" {
    build_args {
        APP_ENV "{APP_ENV}"
    }
}
        "#,
    ).unwrap();

    // ビルド実行
    let result = execute_build(
        Some("test-app".to_string()),
        "local".to_string(),
        false,
    ).await;

    assert!(result.is_ok());

    // イメージが作成されたか確認
    let docker = Docker::connect_with_local_defaults().unwrap();
    let image_tag = "test-test-app:local";
    let inspect_result = docker.inspect_image(image_tag).await;
    assert!(inspect_result.is_ok());

    // クリーンアップ
    docker.remove_image(image_tag, None, None).await.unwrap();
}
```

## エラーハンドリング

### エラー型の定義

```rust
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("Dockerfile not found: {0}")]
    DockerfileNotFound(PathBuf),

    #[error("Build context directory not found: {0}")]
    ContextNotFound(PathBuf),

    #[error("Docker connection error: {0}")]
    DockerConnection(#[from] bollard::errors::Error),

    #[error("Build failed: {0}")]
    BuildFailed(String),

    #[error("Invalid build configuration: {0}")]
    InvalidConfig(String),

    #[error("Variable not found: {0}")]
    VariableNotFound(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
```

### ユーザー向けエラーメッセージ

```rust
impl BuildError {
    pub fn user_message(&self) -> String {
        match self {
            BuildError::DockerfileNotFound(path) => {
                format!(
                    "Dockerfileが見つかりません: {}\n\
                     \n\
                     解決方法:\n\
                     1. Dockerfileのパスを確認してください\n\
                     2. flow.kdlで明示的にパスを指定してください:\n\
                        dockerfile \"path/to/Dockerfile\"",
                    path.display()
                )
            }
            BuildError::BuildFailed(msg) => {
                format!(
                    "ビルドに失敗しました: {}\n\
                     \n\
                     Dockerfileの内容を確認してください。",
                    msg
                )
            }
            _ => format!("{}", self),
        }
    }
}
```

## パフォーマンス最適化

### 1. 並列ビルド

```rust
pub async fn build_services_parallel(
    services: Vec<String>,
    // ... その他の引数
) -> anyhow::Result<()> {
    use futures::future::join_all;

    let build_tasks: Vec<_> = services.iter().map(|service_name| {
        async move {
            // 各サービスのビルド処理
        }
    }).collect();

    // 並列実行
    let results = join_all(build_tasks).await;

    // エラーチェック
    for result in results {
        result?;
    }

    Ok(())
}
```

### 2. キャッシュ戦略

- Dockerのビルドキャッシュを最大限活用
- `.dockerignore`で不要なファイルを除外
- レイヤーの順序最適化を推奨

## セキュリティ考慮事項

### 1. ビルド引数のサニタイズ

```rust
fn validate_build_arg(key: &str, value: &str) -> Result<(), BuildError> {
    // パスワード・トークンなどの検出
    let sensitive_patterns = [
        "password",
        "token",
        "secret",
        "api_key",
        "private_key",
    ];

    let key_lower = key.to_lowercase();
    for pattern in &sensitive_patterns {
        if key_lower.contains(pattern) {
            eprintln!(
                "警告: ビルド引数 '{}' は機密情報を含む可能性があります。\n\
                 ビルド引数はイメージ履歴に記録されます。\n\
                 機密情報はビルド引数ではなく、シークレットマウントを使用してください。",
                key
            );
        }
    }

    Ok(())
}
```

### 2. コンテキストサイズ制限

```rust
const MAX_CONTEXT_SIZE: u64 = 500 * 1024 * 1024; // 500MB

fn check_context_size(context_path: &Path) -> Result<(), BuildError> {
    let size = calculate_dir_size(context_path)?;

    if size > MAX_CONTEXT_SIZE {
        eprintln!(
            "警告: ビルドコンテキストが大きすぎます（{}MB）\n\
             .dockerignoreファイルで不要なファイルを除外することを推奨します。",
            size / 1024 / 1024
        );
    }

    Ok(())
}
```

## マイグレーション

### 既存設定との互換性

既存の`image` + `version`記法は引き続きサポート：

```kdl
# 既存の記法（変更なし）
service "db" {
    image "postgres"
    version "16"
}

# 新しいビルド記法
service "api" {
    dockerfile "./services/api/Dockerfile"
}

# 両方の共存も可能（ビルドが優先）
service "cache" {
    image "redis"        # フォールバック
    version "7"
    dockerfile "./custom-redis/Dockerfile"  # こちらが優先
}
```

---

**次のステップ**: Phase 4 - 実装
