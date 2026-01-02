//! テンプレート展開機能
//!
//! Teraを使用してKDLファイルのテンプレート展開を行います。

use crate::error::{FlowError, Result};
use crate::onepassword;
use std::collections::HashMap;
use std::path::Path;
use tera::{Context, Tera};
use tracing::{debug, info, warn};

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
    /// - FLEET_*: FleetFlow専用の環境変数
    /// - CI_*: CI/CD環境の変数
    /// - APP_*: アプリケーション設定
    #[tracing::instrument(skip(self))]
    pub fn add_env_variables(&mut self) {
        const ALLOWED_PREFIXES: &[&str] = &["FLEET_", "CI_", "APP_"];
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

    /// .env ファイルから変数を読み込んで追加
    ///
    /// .env ファイルの変数はプレフィックス制限なしで全て読み込まれます。
    /// これは .env が明示的に配置されたファイルであるためです。
    #[tracing::instrument(skip(self))]
    pub fn add_env_file_variables(&mut self, env_file_path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(env_file_path).map_err(|e| FlowError::IoError {
            path: env_file_path.to_path_buf(),
            message: e.to_string(),
        })?;

        let mut count = 0;
        for line in content.lines() {
            let line = line.trim();

            // 空行とコメント行をスキップ
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // KEY=VALUE 形式をパース
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();

                // クォートを除去（"value" や 'value' の場合）
                let value = strip_quotes(value);

                // op:// 参照の場合は1Passwordから解決
                let resolved_value = if onepassword::is_op_reference(value) {
                    debug!(key = %key, "Resolving 1Password reference");
                    match onepassword::resolve_reference(value) {
                        Ok(secret) => secret,
                        Err(e) => {
                            warn!(key = %key, error = %e, "Failed to resolve 1Password reference, using original value");
                            value.to_string()
                        }
                    }
                } else {
                    value.to_string()
                };

                debug!(key = %key, "Adding variable from .env file");
                self.context
                    .insert(key, &serde_json::Value::String(resolved_value));
                count += 1;
            }
        }

        info!(
            env_file = %env_file_path.display(),
            variable_count = count,
            "Loaded variables from .env file"
        );

        Ok(())
    }

    /// 文字列をテンプレートとして展開
    pub fn render_str(&mut self, template: &str) -> Result<String> {
        self.tera.render_str(template, &self.context).map_err(|e| {
            // Teraのエラーから詳細情報を抽出
            let error_detail = extract_tera_error_detail(&e);
            FlowError::TemplateRenderError(error_detail)
        })
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
/// variables { ... } ブロックを探してHashMapに変換。
/// 正規表現を使用してブロックを抽出することで、ドキュメント内の他の場所にある
/// テンプレート変数 {{ ... }} によるパースエラーを回避します。
///
/// 注意: この関数はグローバル変数のみを抽出します。
/// ステージ固有の変数は `extract_variables_with_stage` を使用してください。
pub fn extract_variables(kdl_content: &str) -> Result<Variables> {
    extract_variables_with_stage(kdl_content, None)
}

/// ステージを考慮して変数を抽出
///
/// グローバルなvariablesブロックと、指定されたステージのvariablesブロックを抽出します。
/// ステージ固有の変数はグローバル変数を上書きします。
///
/// # Arguments
/// * `kdl_content` - KDLファイルの内容
/// * `stage` - 対象のステージ名（Noneの場合はグローバル変数のみ）
pub fn extract_variables_with_stage(kdl_content: &str, stage: Option<&str>) -> Result<Variables> {
    let mut all_vars = HashMap::new();

    // 1. グローバルなvariablesブロックを抽出
    // stageブロック内ではないvariablesを抽出するため、stageブロックを除外してから処理
    let global_vars = extract_global_variables(kdl_content)?;
    all_vars.extend(global_vars);

    // 2. 指定されたステージのvariablesブロックを抽出
    if let Some(stage_name) = stage {
        let stage_vars = extract_stage_variables(kdl_content, stage_name)?;
        all_vars.extend(stage_vars);
    }

    Ok(all_vars)
}

/// グローバルな変数ブロック（stageブロック外）を抽出
fn extract_global_variables(kdl_content: &str) -> Result<Variables> {
    use regex::Regex;

    // stageブロックを一時的に除去してからvariablesを抽出
    // stage "xxx" { ... } パターンをマッチ（ネストした{}に対応するため、バランスを取る）
    let stage_re = Regex::new(r#"(?s)stage\s+["'][^"']+["']\s*\{"#)
        .map_err(|e| FlowError::InvalidConfig(format!("正規表現のコンパイルエラー: {}", e)))?;

    // stageブロックの開始位置と終了位置を特定
    let mut stage_ranges: Vec<(usize, usize)> = Vec::new();
    for mat in stage_re.find_iter(kdl_content) {
        let start = mat.start();
        // stageブロックの終了位置を見つける（波括弧のバランスを取る）
        if let Some(end) = find_matching_brace(kdl_content, mat.end() - 1) {
            stage_ranges.push((start, end + 1));
        }
    }

    // stageブロックを除去したコンテンツを作成
    let mut global_content = String::with_capacity(kdl_content.len());
    let mut last_end = 0;
    for (start, end) in &stage_ranges {
        global_content.push_str(&kdl_content[last_end..*start]);
        last_end = *end;
    }
    global_content.push_str(&kdl_content[last_end..]);

    // グローバルコンテンツからvariablesを抽出
    extract_variables_from_content(&global_content)
}

/// 指定されたステージのvariablesブロックを抽出
fn extract_stage_variables(kdl_content: &str, stage_name: &str) -> Result<Variables> {
    use regex::Regex;

    // stage "stage_name" { ... } パターンをマッチ
    // ステージ名をエスケープしてパターンを構築
    let escaped_stage = regex::escape(stage_name);
    let stage_pattern = format!(r#"(?s)stage\s+["']{escaped_stage}["']\s*\{{"#);
    let stage_re = Regex::new(&stage_pattern)
        .map_err(|e| FlowError::InvalidConfig(format!("正規表現のコンパイルエラー: {}", e)))?;

    let mut stage_vars = HashMap::new();

    for mat in stage_re.find_iter(kdl_content) {
        // stageブロックの終了位置を見つける
        if let Some(end) = find_matching_brace(kdl_content, mat.end() - 1) {
            // stageブロックの内容を取得
            let stage_content = &kdl_content[mat.end()..end];
            // そのstageブロック内のvariablesを抽出
            let vars = extract_variables_from_content(stage_content)?;
            stage_vars.extend(vars);
        }
    }

    Ok(stage_vars)
}

/// コンテンツからvariablesブロックを抽出
fn extract_variables_from_content(content: &str) -> Result<Variables> {
    use regex::Regex;

    let re = Regex::new(r"(?s)variables\s*\{(?P<content>.*?)\}")
        .map_err(|e| FlowError::InvalidConfig(format!("正規表現のコンパイルエラー: {}", e)))?;

    let mut all_vars = HashMap::new();

    for cap in re.captures_iter(content) {
        if let Some(var_content) = cap.name("content") {
            // ブロックの中身だけをダミーのKDLとしてパース
            let dummy_kdl = format!("extracted {{\n{}\n}}", var_content.as_str());
            let doc: kdl::KdlDocument = dummy_kdl.parse().map_err(|e| {
                FlowError::InvalidConfig(format!("KDL パースエラー (変数抽出ブロック): {}", e))
            })?;

            if let Some(node) = doc.nodes().first()
                && let Some(children) = node.children()
            {
                for var_node in children.nodes() {
                    let key = var_node.name().value().to_string();
                    if let Some(entry) = var_node.entries().first() {
                        let value = kdl_value_to_json(entry.value());
                        all_vars.insert(key, value);
                    }
                }
            }
        }
    }

    Ok(all_vars)
}

/// 対応する閉じ波括弧の位置を見つける
fn find_matching_brace(content: &str, open_pos: usize) -> Option<usize> {
    let bytes = content.as_bytes();
    if open_pos >= bytes.len() || bytes[open_pos] != b'{' {
        return None;
    }

    let mut depth = 1;
    let mut pos = open_pos + 1;
    let mut in_string = false;
    let mut escape_next = false;

    while pos < bytes.len() && depth > 0 {
        let c = bytes[pos];

        if escape_next {
            escape_next = false;
            pos += 1;
            continue;
        }

        if c == b'\\' {
            escape_next = true;
            pos += 1;
            continue;
        }

        if c == b'"' {
            in_string = !in_string;
        } else if !in_string {
            if c == b'{' {
                depth += 1;
            } else if c == b'}' {
                depth -= 1;
            }
        }

        pos += 1;
    }

    if depth == 0 { Some(pos - 1) } else { None }
}

/// クォートを除去するヘルパー関数
///
/// "value" → value
/// 'value' → value
/// value → value
fn strip_quotes(s: &str) -> &str {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        &s[1..s.len() - 1]
    } else {
        s
    }
}

/// Teraエラーから詳細情報を抽出
///
/// Teraのエラーメッセージを解析して、未定義変数などの具体的な情報を取得します。
fn extract_tera_error_detail(e: &tera::Error) -> String {
    use std::error::Error;

    // エラーチェーンを走査して詳細を収集
    let mut details = Vec::new();
    details.push(e.to_string());

    // sourceチェーンをたどる
    let mut source = e.source();
    while let Some(err) = source {
        details.push(err.to_string());
        source = err.source();
    }

    // 未定義変数のパターンを検出
    let full_error = details.join(" | ");

    // Teraの典型的なエラーパターンを解析
    if full_error.contains("not found in context") {
        // 変数名を抽出: "Variable `xxx` not found in context"
        if let Some(start) = full_error.find("Variable `")
            && let Some(end) = full_error[start..].find("` not found")
        {
            let var_name = &full_error[start + 10..start + end];
            return format!(
                "未定義の変数: `{}`\nヒント: variables ブロックで定義するか、.env ファイルに追加してください",
                var_name
            );
        }
    }

    // フィルターエラーの検出
    if full_error.contains("Filter") && full_error.contains("not found") {
        return format!("未定義のフィルター\n詳細: {full_error}");
    }

    // その他のエラーはそのまま返す
    full_error
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
        let services = ["api", "worker", "scheduler"];
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

        let template = "Hello {{ undefined_var }}!";
        let result = processor.render_str(template);

        assert!(result.is_err());

        // エラーメッセージに変数名が含まれていることを確認
        let err = result.unwrap_err();
        let err_msg = err.to_string();
        assert!(
            err_msg.contains("undefined_var"),
            "エラーメッセージに変数名が含まれていません: {}",
            err_msg
        );
    }

    #[test]
    fn test_env_variables_filtering() {
        // 環境変数を設定
        unsafe {
            std::env::set_var("FLEET_VERSION", "1.0.0");
            std::env::set_var("CI_PIPELINE_ID", "12345");
            std::env::set_var("APP_NAME", "myapp");
            std::env::set_var("SECRET_KEY", "should_not_be_included");
            std::env::set_var("HOME", "/home/user");
        }

        let mut processor = TemplateProcessor::new();
        processor.add_env_variables();

        // 許可されたプレフィックスの変数は展開できる
        let template1 = "{{ FLEET_VERSION }}";
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
            std::env::remove_var("FLEET_VERSION");
            std::env::remove_var("CI_PIPELINE_ID");
            std::env::remove_var("APP_NAME");
            std::env::remove_var("SECRET_KEY");
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn test_env_file_variables() {
        let temp_dir = tempfile::tempdir().unwrap();
        let env_file = temp_dir.path().join(".env");

        // .env ファイルを作成
        std::fs::write(
            &env_file,
            r#"
# コメント行
AUTH0_DOMAIN=my-tenant.auth0.com
AUTH0_CLIENT_ID=abc123
DATABASE_URL="postgres://localhost/db"
EMPTY_VALUE=
QUOTED_SINGLE='single quoted'

# 空行の後
API_KEY=secret-key-123
"#,
        )
        .unwrap();

        let mut processor = TemplateProcessor::new();
        processor.add_env_file_variables(&env_file).unwrap();

        // 変数が正しく読み込まれていることを確認
        assert_eq!(
            processor.render_str("{{ AUTH0_DOMAIN }}").unwrap(),
            "my-tenant.auth0.com"
        );
        assert_eq!(
            processor.render_str("{{ AUTH0_CLIENT_ID }}").unwrap(),
            "abc123"
        );
        // ダブルクォートが除去されている
        assert_eq!(
            processor.render_str("{{ DATABASE_URL }}").unwrap(),
            "postgres://localhost/db"
        );
        // シングルクォートが除去されている
        assert_eq!(
            processor.render_str("{{ QUOTED_SINGLE }}").unwrap(),
            "single quoted"
        );
        // 空の値
        assert_eq!(processor.render_str("{{ EMPTY_VALUE }}").unwrap(), "");
        // プレフィックス制限なしで読み込まれている
        assert_eq!(
            processor.render_str("{{ API_KEY }}").unwrap(),
            "secret-key-123"
        );
    }

    #[test]
    fn test_strip_quotes() {
        assert_eq!(strip_quotes("\"hello\""), "hello");
        assert_eq!(strip_quotes("'hello'"), "hello");
        assert_eq!(strip_quotes("hello"), "hello");
        assert_eq!(strip_quotes("\"hello"), "\"hello"); // 不完全なクォート
        assert_eq!(strip_quotes(""), "");
    }

    #[test]
    fn test_extract_stage_specific_variables() {
        // Issue #28: ステージ固有の変数が正しく解決されない問題のテスト
        let kdl = r#"
variables {
    GLOBAL_VAR "global_value"
}

stage "local" {
    variables {
        SURREALDB_DATA_VOLUME "./data/surrealdb"
    }
}

stage "dev" {
    variables {
        SURREALDB_DATA_VOLUME "/opt/creo-memories/data/surrealdb"
    }
}
"#;

        // localステージを指定した場合
        let local_vars = extract_variables_with_stage(kdl, Some("local")).unwrap();
        assert_eq!(
            local_vars.get("SURREALDB_DATA_VOLUME").unwrap(),
            "./data/surrealdb",
            "localステージの変数が取得されるべき"
        );
        assert_eq!(
            local_vars.get("GLOBAL_VAR").unwrap(),
            "global_value",
            "グローバル変数も取得されるべき"
        );

        // devステージを指定した場合
        let dev_vars = extract_variables_with_stage(kdl, Some("dev")).unwrap();
        assert_eq!(
            dev_vars.get("SURREALDB_DATA_VOLUME").unwrap(),
            "/opt/creo-memories/data/surrealdb",
            "devステージの変数が取得されるべき"
        );
        assert_eq!(
            dev_vars.get("GLOBAL_VAR").unwrap(),
            "global_value",
            "グローバル変数も取得されるべき"
        );

        // ステージ指定なしの場合
        let global_only = extract_variables_with_stage(kdl, None).unwrap();
        assert!(
            !global_only.contains_key("SURREALDB_DATA_VOLUME"),
            "ステージ指定なしではステージ固有の変数は取得されないべき"
        );
        assert_eq!(
            global_only.get("GLOBAL_VAR").unwrap(),
            "global_value",
            "グローバル変数は取得されるべき"
        );
    }

    #[test]
    fn test_extract_global_variables_excludes_stage_variables() {
        let kdl = r#"
variables {
    GLOBAL_VAR "global"
}

stage "local" {
    variables {
        STAGE_VAR "local_value"
    }
}
"#;

        let global_vars = extract_global_variables(kdl).unwrap();
        assert_eq!(global_vars.get("GLOBAL_VAR").unwrap(), "global");
        assert!(
            !global_vars.contains_key("STAGE_VAR"),
            "stageブロック内の変数はグローバル変数として抽出されないべき"
        );
    }

    #[test]
    fn test_stage_variables_override_global() {
        let kdl = r#"
variables {
    MY_VAR "global_value"
}

stage "local" {
    variables {
        MY_VAR "local_value"
    }
}
"#;

        let vars = extract_variables_with_stage(kdl, Some("local")).unwrap();
        assert_eq!(
            vars.get("MY_VAR").unwrap(),
            "local_value",
            "ステージ固有の変数がグローバル変数を上書きするべき"
        );
    }

    #[test]
    fn test_find_matching_brace() {
        // 単純なケース
        let content = "{ hello }";
        assert_eq!(find_matching_brace(content, 0), Some(8));

        // ネストしたケース
        let content = "{ outer { inner } }";
        assert_eq!(find_matching_brace(content, 0), Some(18));

        // 文字列内の波括弧
        let content = r#"{ "hello { world }" }"#;
        assert_eq!(find_matching_brace(content, 0), Some(20));

        // 閉じ括弧がない
        let content = "{ hello";
        assert_eq!(find_matching_brace(content, 0), None);
    }
}
