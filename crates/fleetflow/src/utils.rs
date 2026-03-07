use colored::Colorize;

/// ステージ名を決定する（共通ロジック）
pub fn determine_stage_name(
    stage: Option<String>,
    config: &fleetflow_core::Flow,
) -> anyhow::Result<String> {
    if let Some(s) = stage {
        Ok(s)
    } else if config.stages.contains_key("default") {
        Ok("default".to_string())
    } else if config.stages.len() == 1 {
        Ok(config.stages.keys().next().unwrap().clone())
    } else {
        Err(anyhow::anyhow!(
            "ステージ名を指定してください: fleet <command> <stage> または FLEET_STAGE=<stage>\n利用可能なステージ: {}",
            config
                .stages
                .keys()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

/// 読み込んだ設定ファイル情報を表示
pub fn print_loaded_config_files(project_root: &std::path::Path) {
    println!("📄 読み込んだ設定ファイル:");

    let flow_kdl = project_root.join("fleet.kdl");
    if flow_kdl.exists() {
        println!("  • {}", flow_kdl.display().to_string().cyan());
    }

    let flow_local_kdl = project_root.join("flow.local.kdl");
    if flow_local_kdl.exists() {
        println!(
            "  • {} (ローカルオーバーライド)",
            flow_local_kdl.display().to_string().cyan()
        );
    }
}

/// サービスフィルタ（ステージ定義順を維持）
pub fn filter_services(
    stage_services: &[String],
    filters: &[String],
    stage_name: &str,
) -> anyhow::Result<Vec<String>> {
    if filters.is_empty() {
        return Ok(stage_services.to_vec());
    }

    // 指定されたサービスがステージに存在するか確認
    for filter in filters {
        if !stage_services.contains(filter) {
            return Err(anyhow::anyhow!(
                "サービス '{}' はステージ '{}' に含まれていません。\n利用可能なサービス: {}",
                filter,
                stage_name,
                stage_services.join(", ")
            ));
        }
    }

    // ステージ定義順を維持してフィルタ
    Ok(stage_services
        .iter()
        .filter(|s| filters.contains(s))
        .cloned()
        .collect())
}

/// 変数を展開する ({{ VAR_NAME }} 形式)
pub fn expand_variables(
    value: &str,
    variables: &std::collections::HashMap<String, String>,
) -> String {
    let mut result = value.to_string();

    // まず {{ env.XXX }} パターンを展開（ローカル環境変数から取得）
    let env_pattern = regex::Regex::new(r"\{\{\s*env\.(\w+)\s*\}\}").unwrap();
    result = env_pattern
        .replace_all(&result, |caps: &regex::Captures| {
            let env_var_name = &caps[1];
            match std::env::var(env_var_name) {
                Ok(val) => val,
                Err(_) => {
                    eprintln!("    ⚠ 環境変数 {} が見つかりません", env_var_name.yellow());
                    format!("{{{{ env.{} }}}}", env_var_name) // 展開失敗時は元のまま
                }
            }
        })
        .to_string();

    // 次に playbook内の変数を展開
    for (key, val) in variables {
        let pattern = format!("{{{{ {} }}}}", key);
        result = result.replace(&pattern, val);
    }

    // 残りの {{ VAR_NAME }} パターンを環境変数からフォールバック
    let var_pattern = regex::Regex::new(r"\{\{\s*(\w+)\s*\}\}").unwrap();
    result = var_pattern
        .replace_all(&result, |caps: &regex::Captures| {
            let var_name = &caps[1];
            match std::env::var(var_name) {
                Ok(val) => val,
                Err(_) => {
                    eprintln!(
                        "    ⚠ 変数 {} が未定義です（環境変数にもありません）",
                        var_name.yellow()
                    );
                    format!("{{{{ {} }}}}", var_name) // 展開失敗時は元のまま
                }
            }
        })
        .to_string();

    result
}

/// シェル用にエスケープ
pub fn shell_escape(s: &str) -> String {
    // シングルクォートでラップしてエスケープ
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_expand_variables_basic() {
        let mut variables = HashMap::new();
        variables.insert("NAME".to_string(), "Alice".to_string());
        variables.insert("GREETING".to_string(), "Hello".to_string());

        // 単一変数の展開
        assert_eq!(expand_variables("{{ NAME }}", &variables), "Alice");

        // 複数変数の展開
        assert_eq!(
            expand_variables("{{ GREETING }}, {{ NAME }}!", &variables),
            "Hello, Alice!"
        );

        // 変数なしの文字列はそのまま
        assert_eq!(
            expand_variables("No variables here", &variables),
            "No variables here"
        );
    }

    #[test]
    fn test_expand_variables_env_pattern() {
        let variables = HashMap::new();

        temp_env::with_var("TEST_EXPAND_VAR", Some("test_value"), || {
            // {{ env.XXX }} パターンの展開
            assert_eq!(
                expand_variables("Value: {{ env.TEST_EXPAND_VAR }}", &variables),
                "Value: test_value"
            );
        });
    }

    #[test]
    fn test_expand_variables_builtin_fallback() {
        let variables = HashMap::new();

        temp_env::with_var("FLEET_STAGE_TEST", Some("production"), || {
            // {{ VAR_NAME }} パターンが環境変数にフォールバック
            assert_eq!(
                expand_variables("Stage: {{ FLEET_STAGE_TEST }}", &variables),
                "Stage: production"
            );
        });
    }

    #[test]
    fn test_expand_variables_priority() {
        let mut variables = HashMap::new();
        variables.insert("MY_VAR".to_string(), "from_hashmap".to_string());

        temp_env::with_var("MY_VAR", Some("from_env"), || {
            // HashMapの値が優先される
            assert_eq!(expand_variables("{{ MY_VAR }}", &variables), "from_hashmap");
        });
    }

    #[test]
    fn test_expand_variables_undefined() {
        let variables = HashMap::new();

        // 未定義の変数はそのまま残る
        let result = expand_variables("{{ UNDEFINED_VAR_12345 }}", &variables);
        assert_eq!(result, "{{ UNDEFINED_VAR_12345 }}");
    }

    #[test]
    fn test_expand_variables_mixed() {
        let mut variables = HashMap::new();
        variables.insert("PROJECT".to_string(), "myproject".to_string());

        temp_env::with_var("TEST_STAGE", Some("dev"), || {
            // 混合パターン
            let result = expand_variables(
                "{{ PROJECT }}-{{ TEST_STAGE }}-{{ env.TEST_STAGE }}",
                &variables,
            );
            assert_eq!(result, "myproject-dev-dev");
        });
    }

    #[test]
    fn test_shell_escape() {
        assert_eq!(shell_escape("hello"), "'hello'");
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
        assert_eq!(shell_escape(""), "''");
    }
}
