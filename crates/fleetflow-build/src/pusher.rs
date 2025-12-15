//! イメージプッシュ処理
//!
//! ビルドしたイメージをコンテナレジストリにプッシュします。

use crate::auth::RegistryAuth;
use crate::error::{BuildError, BuildResult};
use bollard::Docker;
use bollard::models::PushImageInfo;
use colored::Colorize;
use futures_util::StreamExt;
use std::io::Write;

/// イメージプッシュを実行するハンドラ
pub struct ImagePusher {
    docker: Docker,
    auth: RegistryAuth,
}

impl ImagePusher {
    /// 新しい ImagePusher を作成
    pub fn new(docker: Docker) -> Self {
        Self {
            docker,
            auth: RegistryAuth::new(),
        }
    }

    /// 認証情報マネージャーを指定して作成
    pub fn with_auth(docker: Docker, auth: RegistryAuth) -> Self {
        Self { docker, auth }
    }

    /// イメージをレジストリにプッシュ
    ///
    /// # Arguments
    /// * `image` - イメージ名（レジストリ込み、タグなし）
    /// * `tag` - イメージタグ
    ///
    /// # Returns
    /// プッシュ成功時は完全なイメージ名を返す
    pub async fn push(&self, image: &str, tag: &str) -> BuildResult<String> {
        let full_image = format!("{}:{}", image, tag);

        // タグのバリデーション
        self.validate_tag(tag)?;

        // 認証情報を取得
        let credentials = self.auth.get_credentials(&full_image)?;

        // プッシュオプション（新しいAPI）
        #[allow(deprecated)]
        let options = bollard::image::PushImageOptions::<String> {
            tag: tag.to_string(),
        };

        println!("  → {}", full_image.cyan());

        // プッシュを実行
        #[allow(deprecated)]
        let mut stream = self.docker.push_image(image, Some(options), credentials);

        let mut last_status = String::new();
        let mut error_message: Option<String> = None;

        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(err) = info.error {
                        error_message = Some(err);
                    } else {
                        self.handle_progress(&info, &mut last_status);
                    }
                }
                Err(e) => {
                    return Err(BuildError::PushFailed {
                        message: e.to_string(),
                    });
                }
            }
        }

        // 最終行の改行
        println!();

        // エラーがあった場合
        if let Some(err) = error_message {
            return Err(BuildError::PushFailed { message: err });
        }

        Ok(full_image)
    }

    /// タグのバリデーション
    fn validate_tag(&self, tag: &str) -> BuildResult<()> {
        // Docker タグの制約:
        // - 128文字以下
        // - 英数字、ピリオド、ハイフン、アンダースコアのみ
        // - 先頭はピリオドまたはハイフンではない

        if tag.is_empty() {
            return Err(BuildError::InvalidTag {
                tag: "(empty)".to_string(),
            });
        }

        if tag.len() > 128 {
            return Err(BuildError::InvalidTag {
                tag: format!("Tag too long ({} characters, max 128)", tag.len()),
            });
        }

        if tag.starts_with('.') || tag.starts_with('-') {
            return Err(BuildError::InvalidTag {
                tag: tag.to_string(),
            });
        }

        for c in tag.chars() {
            if !c.is_ascii_alphanumeric() && c != '.' && c != '-' && c != '_' {
                return Err(BuildError::InvalidTag {
                    tag: format!("Invalid character '{}' in tag: {}", c, tag),
                });
            }
        }

        Ok(())
    }

    /// プッシュ進捗を表示
    fn handle_progress(&self, info: &PushImageInfo, last_status: &mut String) {
        if let Some(status) = &info.status {
            let progress = info.progress.as_deref().unwrap_or("");

            // 状態に応じた表示
            match status.as_str() {
                "Pushing" => {
                    // プログレスバー表示
                    print!("\r  ↑ {} {}     ", status, progress);
                    std::io::stdout().flush().ok();
                }
                "Pushed" => {
                    println!("\r  {} Pushed                    ", "✓".green());
                }
                "Layer already exists" => {
                    println!("\r  {} Layer already exists      ", "✓".green());
                }
                "Preparing" | "Waiting" => {
                    // 準備中は表示をスキップ（ノイズ軽減）
                }
                _ => {
                    // その他のステータス
                    if status != last_status {
                        println!("\r  ℹ {}                    ", status);
                        *last_status = status.clone();
                    }
                }
            }
        }
    }
}

/// イメージ名とタグを分離
///
/// # Examples
/// - `ghcr.io/org/app:v1.0` -> `("ghcr.io/org/app", "v1.0")`
/// - `ghcr.io/org/app` -> `("ghcr.io/org/app", "latest")`
/// - `localhost:5000/app:dev` -> `("localhost:5000/app", "dev")`
pub fn split_image_tag(image: &str) -> (String, String) {
    // 最後の : を探す
    if let Some(pos) = image.rfind(':') {
        let potential_tag = &image[pos + 1..];
        let potential_image = &image[..pos];

        // タグか、ポート番号かを判定
        // ポート番号の場合: localhost:5000/app (タグなし)
        // タグの場合: ghcr.io/org/app:v1.0
        //
        // ポート番号は / を含まない純粋な数字
        if !potential_tag.contains('/') && !potential_tag.chars().all(|c| c.is_ascii_digit()) {
            return (potential_image.to_string(), potential_tag.to_string());
        }
    }

    (image.to_string(), "latest".to_string())
}

/// CLIのタグ指定とKDL設定からタグを解決
///
/// # Priority
/// 1. CLI `--tag` オプション（最優先）
/// 2. KDL の image フィールドに含まれるタグ
/// 3. デフォルト: "latest"
pub fn resolve_tag(cli_tag: Option<&str>, kdl_image: &str) -> (String, String) {
    if let Some(tag) = cli_tag {
        // CLI タグが指定されていれば、イメージからタグを除去して使用
        let (base_image, _) = split_image_tag(kdl_image);
        return (base_image, tag.to_string());
    }

    // KDL のイメージにタグが含まれていればそれを使用
    split_image_tag(kdl_image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_image_tag_with_tag() {
        let (image, tag) = split_image_tag("ghcr.io/org/app:v1.0");
        assert_eq!(image, "ghcr.io/org/app");
        assert_eq!(tag, "v1.0");
    }

    #[test]
    fn test_split_image_tag_without_tag() {
        let (image, tag) = split_image_tag("ghcr.io/org/app");
        assert_eq!(image, "ghcr.io/org/app");
        assert_eq!(tag, "latest");
    }

    #[test]
    fn test_split_image_tag_with_port() {
        // localhost:5000/app はポート番号を含むレジストリ
        let (image, tag) = split_image_tag("localhost:5000/app");
        assert_eq!(image, "localhost:5000/app");
        assert_eq!(tag, "latest");
    }

    #[test]
    fn test_split_image_tag_with_port_and_tag() {
        let (image, tag) = split_image_tag("localhost:5000/app:dev");
        assert_eq!(image, "localhost:5000/app");
        assert_eq!(tag, "dev");
    }

    #[test]
    fn test_resolve_tag_cli_priority() {
        let (image, tag) = resolve_tag(Some("v2.0"), "ghcr.io/org/app:v1.0");
        assert_eq!(image, "ghcr.io/org/app");
        assert_eq!(tag, "v2.0");
    }

    #[test]
    fn test_resolve_tag_kdl_tag() {
        let (image, tag) = resolve_tag(None, "ghcr.io/org/app:main");
        assert_eq!(image, "ghcr.io/org/app");
        assert_eq!(tag, "main");
    }

    #[test]
    fn test_resolve_tag_default() {
        let (image, tag) = resolve_tag(None, "ghcr.io/org/app");
        assert_eq!(image, "ghcr.io/org/app");
        assert_eq!(tag, "latest");
    }
}
