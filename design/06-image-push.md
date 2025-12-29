# 設計書: イメージプッシュ機能

**作成日**: 2025-12-13
**関連仕様**: [spec/10-image-push.md](../spec/10-image-push.md)

## How - どのように実装するか

### アーキテクチャ概要

```
fleetflow
    │
    ├── build コマンド
    │   ├── --push オプション処理
    │   └── --tag オプション処理
    │
    └── fleetflow-build クレート
        ├── builder.rs (既存: ビルド処理)
        ├── pusher.rs (新規: プッシュ処理)
        └── auth.rs (新規: 認証処理)
```

### コンポーネント設計

#### 1. CLI拡張 (fleetflow)

**変更ファイル**: `crates/fleetflow/src/commands/build.rs`

```rust
#[derive(Parser)]
pub struct BuildArgs {
    /// ビルド後にレジストリにプッシュ
    #[arg(long)]
    pub push: bool,

    /// イメージタグを指定（--pushと併用）
    #[arg(long)]
    pub tag: Option<String>,

    /// キャッシュを使用しない
    #[arg(long)]
    pub no_cache: bool,

    /// ビルド対象のサービス（省略時は全サービス）
    pub service: Option<String>,

    /// ステージ名
    pub stage: String,
}
```

#### 2. プッシュ処理 (fleetflow-build)

**新規ファイル**: `crates/fleetflow-build/src/pusher.rs`

```rust
pub struct ImagePusher {
    docker: Docker,
    auth: RegistryAuth,
}

impl ImagePusher {
    pub async fn push(&self, image: &str, tag: &str) -> Result<()> {
        let full_image = format!("{}:{}", image, tag);

        // Bollard の push_image API を使用
        let options = PushImageOptions {
            tag: tag.to_string(),
        };

        let credentials = self.auth.get_credentials(image)?;

        let mut stream = self.docker.push_image(&image, Some(options), credentials);

        while let Some(result) = stream.next().await {
            match result {
                Ok(output) => self.handle_progress(output),
                Err(e) => return Err(e.into()),
            }
        }

        Ok(())
    }
}
```

#### 3. 認証処理 (fleetflow-build)

**新規ファイル**: `crates/fleetflow-build/src/auth.rs`

```rust
pub struct RegistryAuth {
    config_path: PathBuf,
}

impl RegistryAuth {
    /// Docker config.json から認証情報を取得
    pub fn get_credentials(&self, image: &str) -> Result<Option<DockerCredentials>> {
        let registry = self.extract_registry(image)?;
        let config = self.load_docker_config()?;

        // 1. config.json の auths セクションを確認
        if let Some(auth) = config.auths.get(&registry) {
            return Ok(Some(self.decode_auth(auth)?));
        }

        // 2. credential helper を確認
        if let Some(helper) = config.creds_store {
            return self.get_from_helper(&helper, &registry);
        }

        Ok(None)
    }

    /// イメージ名からレジストリを抽出
    fn extract_registry(&self, image: &str) -> Result<String> {
        // ghcr.io/org/app:tag -> ghcr.io
        // myuser/app:tag -> docker.io (デフォルト)
        // 123456.dkr.ecr.region.amazonaws.com/app -> 123456.dkr.ecr.region.amazonaws.com

        let parts: Vec<&str> = image.split('/').collect();
        if parts.len() >= 2 && (parts[0].contains('.') || parts[0].contains(':')) {
            Ok(parts[0].to_string())
        } else {
            Ok("docker.io".to_string())
        }
    }
}
```

### データフロー

```
1. fleet build --push --tag v1.0 api live
   │
2. CLI: BuildArgs をパース
   │
3. Config: flow.kdl を読み込み
   │  - service "api" { image "ghcr.io/org/myapp" }
   │
4. Builder: イメージをビルド
   │  - ghcr.io/org/myapp:v1.0 (タグは --tag から)
   │
5. Auth: 認証情報を取得
   │  - ~/.docker/config.json から ghcr.io の認証を取得
   │
6. Pusher: イメージをプッシュ
   │  - Bollard push_image API
   │
7. Output: 結果を表示
      ✓ api: ghcr.io/org/myapp:v1.0
```

### タグ解決ロジック

```rust
fn resolve_tag(cli_tag: Option<&str>, kdl_image: &str) -> (String, String) {
    // cli_tag が指定されていれば最優先
    if let Some(tag) = cli_tag {
        let base_image = remove_tag(kdl_image);
        return (base_image, tag.to_string());
    }

    // KDL の image にタグが含まれていればそれを使用
    if let Some((base, tag)) = split_image_tag(kdl_image) {
        return (base, tag);
    }

    // デフォルトは latest
    (kdl_image.to_string(), "latest".to_string())
}

fn split_image_tag(image: &str) -> Option<(String, String)> {
    // ghcr.io/org/app:v1.0 -> Some(("ghcr.io/org/app", "v1.0"))
    // ghcr.io/org/app -> None
    if let Some(pos) = image.rfind(':') {
        let potential_tag = &image[pos + 1..];
        // タグか、ポート番号かを判定
        if !potential_tag.contains('/') && !potential_tag.chars().all(|c| c.is_numeric()) {
            return Some((image[..pos].to_string(), potential_tag.to_string()));
        }
    }
    None
}
```

### エラーハンドリング

```rust
#[derive(Debug, thiserror::Error)]
pub enum PushError {
    #[error("レジストリへの認証に失敗しました: {registry}")]
    AuthFailed { registry: String },

    #[error("イメージ名にレジストリが含まれていません: {image}")]
    NoRegistry { image: String },

    #[error("イメージのプッシュに失敗しました: {message}")]
    PushFailed { message: String },

    #[error("タグに使用できない文字が含まれています: {tag}")]
    InvalidTag { tag: String },
}
```

### 進捗表示

```rust
fn handle_progress(&self, output: PushImageInfo) {
    match output {
        PushImageInfo { status: Some(status), progress, .. } => {
            // "Pushing" -> プログレスバー表示
            // "Layer already exists" -> スキップ表示
            // "Pushed" -> 完了表示
        }
        _ => {}
    }
}
```

### テスト戦略

#### ユニットテスト

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_extract_registry() {
        let auth = RegistryAuth::default();

        assert_eq!(auth.extract_registry("ghcr.io/org/app").unwrap(), "ghcr.io");
        assert_eq!(auth.extract_registry("myuser/app").unwrap(), "docker.io");
        assert_eq!(
            auth.extract_registry("123456.dkr.ecr.ap-northeast-1.amazonaws.com/app").unwrap(),
            "123456.dkr.ecr.ap-northeast-1.amazonaws.com"
        );
    }

    #[test]
    fn test_resolve_tag() {
        // --tag 指定あり
        assert_eq!(
            resolve_tag(Some("v1.0"), "ghcr.io/org/app:latest"),
            ("ghcr.io/org/app".to_string(), "v1.0".to_string())
        );

        // --tag なし、KDLにタグあり
        assert_eq!(
            resolve_tag(None, "ghcr.io/org/app:main"),
            ("ghcr.io/org/app".to_string(), "main".to_string())
        );

        // --tag なし、KDLにタグなし
        assert_eq!(
            resolve_tag(None, "ghcr.io/org/app"),
            ("ghcr.io/org/app".to_string(), "latest".to_string())
        );
    }
}
```

#### 統合テスト

```rust
#[tokio::test]
#[ignore] // 実際のレジストリが必要
async fn test_push_to_registry() {
    // テスト用のローカルレジストリを使用
    let pusher = ImagePusher::new().await.unwrap();
    pusher.push("localhost:5000/test-app", "test").await.unwrap();
}
```

### 実装順序

1. **Phase 1: 認証処理**
   - `auth.rs` の実装
   - `config.json` の読み込み
   - レジストリ抽出ロジック

2. **Phase 2: プッシュ処理**
   - `pusher.rs` の実装
   - Bollard `push_image` API の呼び出し
   - 進捗表示

3. **Phase 3: CLI統合**
   - `--push`, `--tag` オプションの追加
   - build コマンドへの統合
   - エラーメッセージの整備

### 依存関係

```toml
# crates/fleetflow-build/Cargo.toml
[dependencies]
bollard = { version = "0.18", features = ["ssl"] }
base64 = "0.22"
serde_json = "1.0"
dirs = "5.0"
```

### 設定ファイル対応

#### Docker config.json の構造

```json
{
  "auths": {
    "ghcr.io": {
      "auth": "base64(username:password)"
    },
    "docker.io": {
      "auth": "base64(username:password)"
    }
  },
  "credsStore": "osxkeychain"
}
```

#### credential helper 対応

```rust
fn get_from_helper(&self, helper: &str, registry: &str) -> Result<Option<DockerCredentials>> {
    // docker-credential-{helper} get を実行
    let output = Command::new(format!("docker-credential-{}", helper))
        .arg("get")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    // registry を stdin に渡す
    output.stdin.unwrap().write_all(registry.as_bytes())?;

    // JSON レスポンスをパース
    let response: CredentialResponse = serde_json::from_reader(output.stdout.unwrap())?;

    Ok(Some(DockerCredentials {
        username: Some(response.username),
        password: Some(response.secret),
        ..Default::default()
    }))
}
```
