use crate::error::{BuildError, Result};
use fleetflow_atom::Service;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct BuildResolver {
    project_root: PathBuf,
}

impl BuildResolver {
    pub fn new(project_root: PathBuf) -> Self {
        Self { project_root }
    }

    /// Dockerfileのパスを解決
    ///
    /// 検索順序:
    /// 1. 明示的な指定（build.dockerfileフィールド）
    /// 2. 規約ベース:
    ///    - ./services/{service-name}/Dockerfile
    ///    - ./{service-name}/Dockerfile
    ///    - ./Dockerfile.{service-name}
    pub fn resolve_dockerfile(
        &self,
        service_name: &str,
        service: &Service,
    ) -> Result<Option<PathBuf>> {
        // 明示的な指定がある場合
        if let Some(build) = &service.build {
            if let Some(dockerfile) = &build.dockerfile {
                let path = self.project_root.join(dockerfile);
                if path.exists() {
                    return Ok(Some(path));
                } else {
                    return Err(BuildError::DockerfileNotFound(path));
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
                tracing::debug!(
                    "Found Dockerfile for service '{}' at: {}",
                    service_name,
                    path.display()
                );
                return Ok(Some(path));
            }
        }

        // Dockerfileが見つからない場合はNone（pullで対応）
        Ok(None)
    }

    /// ビルドコンテキストのパスを解決
    ///
    /// デフォルトはプロジェクトルート
    pub fn resolve_context(&self, service: &Service) -> Result<PathBuf> {
        let context = if let Some(build) = &service.build {
            if let Some(ctx) = &build.context {
                self.project_root.join(ctx)
            } else {
                self.project_root.clone()
            }
        } else {
            self.project_root.clone()
        };

        // コンテキストディレクトリの存在確認
        if !context.exists() {
            return Err(BuildError::ContextNotFound(context));
        }

        if !context.is_dir() {
            return Err(BuildError::InvalidConfig(format!(
                "Build context is not a directory: {}",
                context.display()
            )));
        }

        Ok(context)
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

    /// 変数展開処理
    ///
    /// テンプレート文字列内の {VAR_NAME} を実際の値に置換
    fn expand_variables(&self, template: &str, variables: &HashMap<String, String>) -> String {
        let mut result = template.to_string();

        for (key, value) in variables {
            let placeholder = format!("{{{}}}", key);
            result = result.replace(&placeholder, value);
        }

        result
    }

    /// イメージタグの解決
    ///
    /// 優先順位:
    /// 1. 明示的なタグ指定（build.image_tag）
    /// 2. 自動生成タグ: {project}-{service}:{stage}
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
        format!("{}-{}:{}", project_name, service_name, stage_name)
    }

    /// ビルド引数の検証（機密情報の警告）
    pub fn validate_build_arg(&self, key: &str, value: &str) {
        let sensitive_patterns = ["password", "token", "secret", "api_key", "private_key"];

        let key_lower = key.to_lowercase();
        for pattern in &sensitive_patterns {
            if key_lower.contains(pattern) {
                tracing::warn!(
                    "警告: ビルド引数 '{}' は機密情報を含む可能性があります。\n\
                     ビルド引数はイメージ履歴に記録されます。\n\
                     機密情報はビルド引数ではなく、環境変数やシークレットマウントを使用してください。",
                    key
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleetflow_atom::{BuildConfig, Service};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_resolve_dockerfile_explicit() {
        let temp_dir = tempdir().unwrap();
        let dockerfile_path = temp_dir.path().join("custom.dockerfile");
        fs::write(&dockerfile_path, "FROM alpine").unwrap();

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
    fn test_resolve_dockerfile_convention_services() {
        let temp_dir = tempdir().unwrap();
        let services_dir = temp_dir.path().join("services/api");
        fs::create_dir_all(&services_dir).unwrap();

        let dockerfile_path = services_dir.join("Dockerfile");
        fs::write(&dockerfile_path, "FROM alpine").unwrap();

        let resolver = BuildResolver::new(temp_dir.path().to_path_buf());
        let service = Service::default();

        let result = resolver.resolve_dockerfile("api", &service).unwrap();
        assert_eq!(result, Some(dockerfile_path));
    }

    #[test]
    fn test_resolve_dockerfile_convention_root() {
        let temp_dir = tempdir().unwrap();
        let api_dir = temp_dir.path().join("api");
        fs::create_dir_all(&api_dir).unwrap();

        let dockerfile_path = api_dir.join("Dockerfile");
        fs::write(&dockerfile_path, "FROM alpine").unwrap();

        let resolver = BuildResolver::new(temp_dir.path().to_path_buf());
        let service = Service::default();

        let result = resolver.resolve_dockerfile("api", &service).unwrap();
        assert_eq!(result, Some(dockerfile_path));
    }

    #[test]
    fn test_resolve_dockerfile_not_found() {
        let temp_dir = tempdir().unwrap();
        let resolver = BuildResolver::new(temp_dir.path().to_path_buf());
        let service = Service::default();

        let result = resolver.resolve_dockerfile("nonexistent", &service).unwrap();
        assert_eq!(result, None);
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

    #[test]
    fn test_resolve_image_tag_explicit() {
        let resolver = BuildResolver::new(PathBuf::from("/tmp"));

        let mut service = Service::default();
        service.build = Some(BuildConfig {
            image_tag: Some("myapp:v1.0.0".to_string()),
            ..Default::default()
        });

        let tag = resolver.resolve_image_tag("api", &service, "project", "local");
        assert_eq!(tag, "myapp:v1.0.0");
    }

    #[test]
    fn test_resolve_image_tag_auto() {
        let resolver = BuildResolver::new(PathBuf::from("/tmp"));
        let service = Service::default();

        let tag = resolver.resolve_image_tag("api", &service, "myproject", "local");
        assert_eq!(tag, "myproject-api:local");
    }

    #[test]
    fn test_resolve_context_default() {
        let temp_dir = tempdir().unwrap();
        let resolver = BuildResolver::new(temp_dir.path().to_path_buf());
        let service = Service::default();

        let context = resolver.resolve_context(&service).unwrap();
        assert_eq!(context, temp_dir.path());
    }

    #[test]
    fn test_resolve_context_explicit() {
        let temp_dir = tempdir().unwrap();
        let ctx_dir = temp_dir.path().join("backend");
        fs::create_dir(&ctx_dir).unwrap();

        let resolver = BuildResolver::new(temp_dir.path().to_path_buf());

        let mut service = Service::default();
        service.build = Some(BuildConfig {
            context: Some(PathBuf::from("backend")),
            ..Default::default()
        });

        let context = resolver.resolve_context(&service).unwrap();
        assert_eq!(context, ctx_dir);
    }
}
