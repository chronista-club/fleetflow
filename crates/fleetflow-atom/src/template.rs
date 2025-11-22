//! テンプレート展開機能
//!
//! Teraを使用してKDLファイルのテンプレート展開を行います。

use crate::error::{FlowError, Result};
use std::collections::HashMap;
use std::path::Path;
use tera::{Context, Tera};
use tracing::{debug, info};

/// ファイルあたりの推定バイト数（容量事前確保用）
const ESTIMATED_BYTES_PER_FILE: usize = 500;

/// 変数コンテキスト
pub type Variables = HashMap<String, serde_json::Value>;

/// テンプレートプロセッサ
pub struct TemplateProcessor {
    tera: Tera,
    context: Context,
}

impl TemplateProcessor {
    /// 新しいテンプレートプロセッサを作成
    pub fn new() -> Self {
        Self {
            tera: Tera::default(),
            context: Context::new(),
        }
    }

    /// 変数を追加
    pub fn add_variable(&mut self, key: impl Into<String>, value: serde_json::Value) {
        self.context.insert(key.into(), &value);
    }

    /// 複数の変数を追加
    pub fn add_variables(&mut self, variables: Variables) {
        for (key, value) in variables {
            self.context.insert(key, &value);
        }
    }

    /// 環境変数を追加（安全なもののみ）
    ///
    /// セキュリティ上の理由から、以下のプレフィックスを持つ環境変数のみを許可:
    /// - FLOW_*: FleetFlow専用の環境変数
    /// - CI_*: CI/CD環境の変数
    /// - APP_*: アプリケーション設定
    #[tracing::instrument(skip(self))]
    pub fn add_env_variables(&mut self) {
        const ALLOWED_PREFIXES: &[&str] = &["FLOW_", "CI_", "APP_"];
        let mut count = 0;

        for (key, value) in std::env::vars() {
            // 許可されたプレフィックスを持つ環境変数のみを追加
            if ALLOWED_PREFIXES
                .iter()
                .any(|prefix| key.starts_with(prefix))
            {
                debug!(key = %key, "Adding environment variable");
                self.context.insert(key, &serde_json::Value::String(value));
                count += 1;
            }
        }

        info!(
            env_var_count = count,
            "Added filtered environment variables"
        );
    }

    /// 環境変数を安全にフィルタリングせずに追加（テスト用）
    ///
    /// # Safety
    /// この関数は信頼できる環境でのみ使用してください。
    /// 本番環境では `add_env_variables()` を使用することを推奨します。
    #[cfg(test)]
    pub fn add_env_variables_unfiltered(&mut self) {
        for (key, value) in std::env::vars() {
            self.context.insert(key, &serde_json::Value::String(value));
        }
    }

    /// env() 関数を登録
    pub fn register_env_function(&mut self) {
        // Teraの組み込み関数として env() を使えるようにする
        // 実装は後で追加
    }

    /// 文字列をテンプレートとして展開
    pub fn render_str(&mut self, template: &str) -> Result<String> {
        self.tera
            .render_str(template, &self.context)
            .map_err(|e| FlowError::TemplateRenderError(format!("テンプレート展開エラー: {}", e)))
    }

    /// ファイルを読み込んでテンプレート展開
    pub fn render_file(&mut self, path: &Path) -> Result<String> {
        let content = std::fs::read_to_string(path).map_err(|e| FlowError::IoError {
            path: path.to_path_buf(),
            message: e.to_string(),
        })?;

        self.render_str(&content).map_err(|e| {
            // TemplateRenderErrorをより詳細なTemplateErrorに変換
            if let FlowError::TemplateRenderError(msg) = e {
                FlowError::TemplateError {
                    file: path.to_path_buf(),
                    line: None,
                    message: msg,
                }
            } else {
                e
            }
        })
    }

    /// 複数のファイルを順に展開して結合
    pub fn render_files(&mut self, paths: &[impl AsRef<Path>]) -> Result<String> {
        // ファイル数から概算容量を計算
        let estimated_capacity = paths.len() * ESTIMATED_BYTES_PER_FILE;
        let mut result = String::with_capacity(estimated_capacity);

        for path in paths {
            let rendered = self.render_file(path.as_ref())?;
            result.push_str(&rendered);
            result.push('\n'); // ファイル間の区切り
        }

        Ok(result)
    }
}

impl Default for TemplateProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// KDLファイルから変数定義を抽出
///
/// variables { ... } ブロックを探してHashMapに変換
pub fn extract_variables(kdl_content: &str) -> Result<Variables> {
    let doc: kdl::KdlDocument = kdl_content
        .parse()
        .map_err(|e| FlowError::InvalidConfig(format!("KDL パースエラー: {}", e)))?;

    let mut variables = HashMap::new();

    // variables ノードを探す
    for node in doc.nodes() {
        if node.name().value() == "variables" {
            if let Some(children) = node.children() {
                for var_node in children.nodes() {
                    let key = var_node.name().value().to_string();

                    // 最初のエントリから値を取得
                    if let Some(entry) = var_node.entries().first() {
                        let value = kdl_value_to_json(entry.value());
                        variables.insert(key, value);
                    }
                }
            }
        }
    }

    Ok(variables)
}

/// KDL値をJSON値に変換
fn kdl_value_to_json(value: &kdl::KdlValue) -> serde_json::Value {
    if let Some(s) = value.as_string() {
        serde_json::Value::String(s.to_string())
    } else if let Some(i) = value.as_integer() {
        // i128をi64に変換してからJSONに変換
        serde_json::Value::Number((i as i64).into())
    } else if let Some(f) = value.as_float() {
        serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null)
    } else if let Some(b) = value.as_bool() {
        serde_json::Value::Bool(b)
    } else {
        serde_json::Value::Null
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_variable_expansion() {
        let mut processor = TemplateProcessor::new();
        processor.add_variable("name", serde_json::Value::String("world".to_string()));

        let template = "Hello {{ name }}!";
        let result = processor.render_str(template).unwrap();

        assert_eq!(result, "Hello world!");
    }

    #[test]
    fn test_nested_variables() {
        let mut processor = TemplateProcessor::new();
        processor.add_variable("project", serde_json::Value::String("myapp".to_string()));
        processor.add_variable("env", serde_json::Value::String("prod".to_string()));

        let template = r#"image "{{ project }}:{{ env }}""#;
        let result = processor.render_str(template).unwrap();

        assert_eq!(result, r#"image "myapp:prod""#);
    }

    #[test]
    fn test_filter_lower() {
        let mut processor = TemplateProcessor::new();
        processor.add_variable("name", serde_json::Value::String("HELLO".to_string()));

        let template = "{{ name | lower }}";
        let result = processor.render_str(template).unwrap();

        assert_eq!(result, "hello");
    }

    #[test]
    fn test_filter_upper() {
        let mut processor = TemplateProcessor::new();
        processor.add_variable("name", serde_json::Value::String("hello".to_string()));

        let template = "{{ name | upper }}";
        let result = processor.render_str(template).unwrap();

        assert_eq!(result, "HELLO");
    }

    #[test]
    fn test_if_condition() {
        let mut processor = TemplateProcessor::new();
        processor.add_variable("is_prod", serde_json::Value::Bool(true));

        let template = r#"
{% if is_prod %}
replicas 3
{% else %}
replicas 1
{% endif %}
"#;
        let result = processor.render_str(template).unwrap();

        assert!(result.contains("replicas 3"));
        assert!(!result.contains("replicas 1"));
    }

    #[test]
    fn test_for_loop() {
        let mut processor = TemplateProcessor::new();
        let services = vec!["api", "worker", "scheduler"];
        processor.add_variable(
            "services",
            serde_json::Value::Array(
                services
                    .iter()
                    .map(|s| serde_json::Value::String(s.to_string()))
                    .collect(),
            ),
        );

        let template = r#"
{% for service in services %}
service "{{ service }}"
{% endfor %}
"#;
        let result = processor.render_str(template).unwrap();

        assert!(result.contains(r#"service "api""#));
        assert!(result.contains(r#"service "worker""#));
        assert!(result.contains(r#"service "scheduler""#));
    }

    #[test]
    fn test_extract_variables() {
        let kdl = r#"
variables {
    app_version "1.0.0"
    port 8080
    debug #true
}
"#;

        let vars = extract_variables(kdl).unwrap();

        assert_eq!(vars.get("app_version").unwrap(), "1.0.0");
        assert_eq!(vars.get("port").unwrap(), 8080);
        assert_eq!(vars.get("debug").unwrap(), true);
    }

    #[test]
    fn test_extract_multiple_variables_blocks() {
        let kdl = r#"
variables {
    name "first"
}

service "api" {}

variables {
    name "second"
}
"#;

        let vars = extract_variables(kdl).unwrap();

        // 最後の定義が優先される（後勝ち）
        assert_eq!(vars.get("name").unwrap(), "second");
    }

    #[test]
    fn test_undefined_variable_error() {
        let mut processor = TemplateProcessor::new();

        let template = "Hello {{ undefined }}!";
        let result = processor.render_str(template);

        assert!(result.is_err());
    }

    #[test]
    fn test_env_variables_filtering() {
        // 環境変数を設定
        unsafe {
            std::env::set_var("FLOW_VERSION", "1.0.0");
            std::env::set_var("CI_PIPELINE_ID", "12345");
            std::env::set_var("APP_NAME", "myapp");
            std::env::set_var("SECRET_KEY", "should_not_be_included");
            std::env::set_var("HOME", "/home/user");
        }

        let mut processor = TemplateProcessor::new();
        processor.add_env_variables();

        // 許可されたプレフィックスの変数は展開できる
        let template1 = "{{ FLOW_VERSION }}";
        assert_eq!(processor.render_str(template1).unwrap(), "1.0.0");

        let template2 = "{{ CI_PIPELINE_ID }}";
        assert_eq!(processor.render_str(template2).unwrap(), "12345");

        let template3 = "{{ APP_NAME }}";
        assert_eq!(processor.render_str(template3).unwrap(), "myapp");

        // 許可されていない変数は展開できない（エラーになる）
        let template4 = "{{ SECRET_KEY }}";
        assert!(processor.render_str(template4).is_err());

        let template5 = "{{ HOME }}";
        assert!(processor.render_str(template5).is_err());

        // クリーンアップ
        unsafe {
            std::env::remove_var("FLOW_VERSION");
            std::env::remove_var("CI_PIPELINE_ID");
            std::env::remove_var("APP_NAME");
            std::env::remove_var("SECRET_KEY");
            std::env::remove_var("HOME");
        }
    }
}
