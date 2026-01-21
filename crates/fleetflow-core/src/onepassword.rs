//! 1Password統合モジュール
//!
//! `op://` 形式の参照を1Password CLIを使って解決します。
//!
//! ## 参照形式
//!
//! ```text
//! op://Vault/Item/Field
//! op://Vault/Item/Section/Field
//! ```
//!
//! ## 解決の優先順位
//!
//! 1. **環境変数**（最優先）: op://参照から推測した変数名が環境変数に存在すればそれを使用
//! 2. **1Password CLI**: 環境変数がなければ `op read` で解決
//!
//! これにより、以下の環境で透過的に動作します:
//! - **ローカル開発**: 1Password CLIで解決
//! - **GitHub CI**: 1Password Actions で環境変数に展開 → 環境変数から取得
//! - **VPS**: 環境変数を事前設定 → 環境変数から取得
//!
//! ## 変数名の推測ルール
//!
//! ```text
//! op://Vault/Item/Field         → FIELD (大文字スネークケース)
//! op://Vault/Item/Section/Field → FIELD (最後のセグメント)
//!
//! 例:
//! op://FleetFlowVault/shared/stripe/secret_key → STRIPE_SECRET_KEY
//! op://FleetFlowVault/prod-database/app_pass   → APP_PASS
//! ```
//!
//! ## 必要な環境
//!
//! - 1Password CLI (`op`) がインストールされていること（環境変数がない場合）
//! - `OP_SERVICE_ACCOUNT_TOKEN` 環境変数が設定されていること（サーバー環境）
//! - または1Password CLIでサインイン済みであること（ローカル開発）
//!
//! ## セキュリティ
//!
//! - 解決された秘密情報はログに出力されません
//! - エラーメッセージにも秘密情報は含まれません

use crate::error::{FlowError, Result};
use std::collections::HashMap;
use std::process::Command;
use tracing::{debug, info, warn};

/// 1Password参照のプレフィックス
const OP_PREFIX: &str = "op://";

/// op://参照から環境変数名を推測する
///
/// パスの最後のセグメントを大文字スネークケースに変換します。
///
/// # 変換例
///
/// ```text
/// op://FleetFlowVault/shared/stripe/secret_key → STRIPE_SECRET_KEY
/// op://FleetFlowVault/prod-database/app_pass   → APP_PASS
/// op://Vault/Item/Field                        → FIELD
/// ```
///
/// # Arguments
///
/// * `reference` - `op://` で始まる参照文字列
///
/// # Returns
///
/// 推測された環境変数名
fn infer_env_var_name(reference: &str) -> Option<String> {
    // op://を除去してパスを取得
    let path = reference.strip_prefix(OP_PREFIX)?;

    // 最後のセグメント（フィールド名）を取得
    let field = path.rsplit('/').next()?;

    if field.is_empty() {
        return None;
    }

    // 大文字スネークケースに変換
    // すでにスネークケースの場合は大文字化のみ
    // キャメルケースの場合はスネークケースに変換
    let env_var = field
        .chars()
        .fold(String::new(), |mut acc, c| {
            if c.is_uppercase() && !acc.is_empty() && !acc.ends_with('_') {
                acc.push('_');
            }
            acc.push(c.to_ascii_uppercase());
            acc
        })
        .replace('-', "_");

    Some(env_var)
}

/// 環境変数から値を取得（フォールバック用）
fn try_resolve_from_env(reference: &str) -> Option<String> {
    let env_var = infer_env_var_name(reference)?;

    match std::env::var(&env_var) {
        Ok(value) if !value.is_empty() => {
            debug!(
                env_var = %env_var,
                reference = %reference,
                "Resolved from environment variable (fallback)"
            );
            Some(value)
        }
        _ => None,
    }
}

/// 1Password CLIが利用可能かチェック
pub fn is_available() -> bool {
    Command::new("op")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// 値が1Password参照かどうかをチェック
pub fn is_op_reference(value: &str) -> bool {
    value.starts_with(OP_PREFIX)
}

/// 1Password参照を解決
///
/// `op://Vault/Item/Field` 形式の参照を実際の値に解決します。
///
/// ## 解決の優先順位
///
/// 1. **環境変数**（最優先）: op://参照から推測した変数名が環境変数に存在すればそれを使用
/// 2. **1Password CLI**: 環境変数がなければ `op read` で解決
///
/// # Arguments
///
/// * `reference` - `op://` で始まる参照文字列
///
/// # Returns
///
/// 解決された秘密情報、または参照形式が不正な場合はエラー
///
/// # Example
///
/// ```ignore
/// let secret = resolve_reference("op://FleetFlowVault/creo-dev-database/password")?;
/// ```
pub fn resolve_reference(reference: &str) -> Result<String> {
    if !is_op_reference(reference) {
        return Err(FlowError::OnePasswordError(format!(
            "無効な1Password参照: {} (op://で始まる必要があります)",
            reference
        )));
    }

    debug!(reference = %reference, "Resolving 1Password reference");

    // 1. 環境変数からの解決を試みる（CI/VPS環境向け）
    if let Some(value) = try_resolve_from_env(reference) {
        return Ok(value);
    }

    // 2. 1Password CLIで解決（ローカル開発向け）
    resolve_via_op_cli(reference)
}

/// 1Password CLIを使用して参照を解決
fn resolve_via_op_cli(reference: &str) -> Result<String> {
    let mut cmd = Command::new("op");
    cmd.arg("read").arg(reference);

    // OP_ACCOUNT環境変数が設定されている場合は使用
    if let Ok(account) = std::env::var("OP_ACCOUNT") {
        debug!(account = %account, "Using OP_ACCOUNT");
        cmd.arg("--account").arg(account);
    }

    let output = cmd.output().map_err(|e| {
        // CLIが見つからない場合のヒント
        let hint = if e.kind() == std::io::ErrorKind::NotFound {
            format!(
                "\nヒント: 1Password CLIがインストールされていません。\n\
                 環境変数 {} を設定することでも解決できます。",
                infer_env_var_name(reference).unwrap_or_else(|| "???".to_string())
            )
        } else {
            String::new()
        };
        FlowError::OnePasswordError(format!("1Password CLI実行エラー: {}{}", e, hint))
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // エラーメッセージを解析して適切なヒントを提供
        let env_var_hint = infer_env_var_name(reference)
            .map(|v| format!("\nヒント: 環境変数 {} を設定することでも解決できます", v))
            .unwrap_or_default();

        let hint = if stderr.contains("not signed in") || stderr.contains("session expired") {
            format!(
                "\nヒント: `op signin` でサインインするか、OP_SERVICE_ACCOUNT_TOKEN を設定してください{}",
                env_var_hint
            )
        } else if stderr.contains("not found") {
            format!(
                "\nヒント: Vault名、Item名、Field名が正しいか確認してください{}",
                env_var_hint
            )
        } else if stderr.contains("multiple accounts") {
            format!(
                "\nヒント: OP_ACCOUNT 環境変数でアカウントを指定してください{}",
                env_var_hint
            )
        } else {
            env_var_hint
        };

        return Err(FlowError::OnePasswordError(format!(
            "1Password参照の解決に失敗: {}{}",
            stderr.trim(),
            hint
        )));
    }

    let secret = String::from_utf8_lossy(&output.stdout).trim().to_string();
    debug!("Successfully resolved 1Password reference via CLI");

    Ok(secret)
}

/// 複数の1Password参照をバッチで解決
///
/// パフォーマンスのため、将来的には `op read` の複数呼び出しを
/// 最適化する可能性があります。
///
/// # Arguments
///
/// * `references` - 解決する参照のリスト
///
/// # Returns
///
/// 参照から解決された値へのマッピング
pub fn resolve_references(references: &[&str]) -> Result<HashMap<String, String>> {
    let mut results = HashMap::new();
    let mut errors = Vec::new();

    for reference in references {
        match resolve_reference(reference) {
            Ok(value) => {
                results.insert(reference.to_string(), value);
            }
            Err(e) => {
                errors.push(format!("{}: {}", reference, e));
            }
        }
    }

    if !errors.is_empty() {
        return Err(FlowError::OnePasswordError(format!(
            "一部の1Password参照の解決に失敗:\n{}",
            errors.join("\n")
        )));
    }

    info!(count = results.len(), "Resolved all 1Password references");

    Ok(results)
}

/// 変数マップ内の1Password参照を解決
///
/// HashMap内の値を走査し、`op://` で始まる値を実際の秘密情報に置き換えます。
///
/// ## 解決の優先順位
///
/// 1. **変数名と同名の環境変数**（最優先）: fleet.kdlのキー名と同じ環境変数があればそれを使用
/// 2. **op://パスから推測した環境変数**: パスの最後のセグメントから変数名を推測
/// 3. **1Password CLI**: 環境変数がなければ `op read` で解決
///
/// # Arguments
///
/// * `variables` - 変数マップ（変更される）
///
/// # Returns
///
/// 解決された参照の数
pub fn resolve_variables(variables: &mut HashMap<String, serde_json::Value>) -> Result<usize> {
    let mut resolved_count = 0;
    let mut errors = Vec::new();
    let mut need_op_cli = false;

    // 1Password参照を持つキーを収集
    let op_keys: Vec<_> = variables
        .iter()
        .filter_map(|(key, value)| {
            if let Some(s) = value.as_str()
                && is_op_reference(s)
            {
                return Some((key.clone(), s.to_string()));
            }
            None
        })
        .collect();

    if op_keys.is_empty() {
        debug!("No 1Password references found in variables");
        return Ok(0);
    }

    info!(
        count = op_keys.len(),
        "Found 1Password references to resolve"
    );

    // 各参照を解決
    for (key, reference) in &op_keys {
        // 1. 変数名と同名の環境変数があればそれを使用（最優先）
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() {
                debug!(
                    key = %key,
                    "Resolved from environment variable (variable name match)"
                );
                variables.insert(key.clone(), serde_json::Value::String(value));
                resolved_count += 1;
                continue;
            }
        }

        // 2. op://パスから推測した環境変数名でも試す
        if let Some(value) = try_resolve_from_env(reference) {
            variables.insert(key.clone(), serde_json::Value::String(value));
            resolved_count += 1;
            continue;
        }

        // 3. 1Password CLIが必要なキーとしてマーク
        need_op_cli = true;
    }

    // 環境変数で解決できなかったものがある場合、1Password CLIを使用
    if need_op_cli {
        // 1Password CLIが利用可能かチェック
        if !is_available() {
            // CLIが利用できない場合、未解決の変数をリストアップ
            let unresolved: Vec<_> = op_keys
                .iter()
                .filter(|(key, _)| {
                    variables
                        .get(key)
                        .map(|v| v.as_str().map(is_op_reference).unwrap_or(false))
                        .unwrap_or(true)
                })
                .map(|(key, ref_)| format!("  {} (環境変数 {} を設定してください)", key, key))
                .collect();

            if !unresolved.is_empty() {
                return Err(FlowError::OnePasswordError(format!(
                    "1Password CLI (op) が利用できず、以下の変数が未解決です:\n{}\n\n\
                     ヒント: CI/VPS環境では環境変数を設定することで解決できます",
                    unresolved.join("\n")
                )));
            }
        }

        // 未解決の参照をCLIで解決
        for (key, reference) in op_keys {
            // すでに解決済みかチェック
            if let Some(value) = variables.get(&key) {
                if !value.as_str().map(is_op_reference).unwrap_or(true) {
                    continue;
                }
            }

            match resolve_via_op_cli(&reference) {
                Ok(secret) => {
                    variables.insert(key.clone(), serde_json::Value::String(secret));
                    resolved_count += 1;
                    debug!(key = %key, "Resolved 1Password reference via CLI");
                }
                Err(e) => {
                    errors.push(format!(
                        "{} ({}): {}\n  → 環境変数 {} を設定することでも解決できます",
                        key, reference, e, key
                    ));
                }
            }
        }
    }

    if !errors.is_empty() {
        return Err(FlowError::OnePasswordError(format!(
            "一部の1Password参照の解決に失敗:\n{}",
            errors.join("\n")
        )));
    }

    info!(
        resolved = resolved_count,
        "Successfully resolved 1Password references"
    );

    Ok(resolved_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_is_op_reference() {
        assert!(is_op_reference("op://Vault/Item/Field"));
        assert!(is_op_reference(
            "op://FleetFlowVault/creo-dev-database/password"
        ));
        assert!(!is_op_reference("https://example.com"));
        assert!(!is_op_reference("password123"));
        assert!(!is_op_reference(""));
    }

    #[test]
    fn test_infer_env_var_name() {
        // 基本的なケース
        assert_eq!(
            infer_env_var_name("op://Vault/Item/field"),
            Some("FIELD".to_string())
        );

        // スネークケース → 大文字スネークケース
        assert_eq!(
            infer_env_var_name("op://FleetFlowVault/shared/stripe/secret_key"),
            Some("SECRET_KEY".to_string())
        );

        // ハイフン → アンダースコア
        assert_eq!(
            infer_env_var_name("op://Vault/Item/api-key"),
            Some("API_KEY".to_string())
        );

        // キャメルケース → スネークケース
        assert_eq!(
            infer_env_var_name("op://Vault/Item/secretKey"),
            Some("SECRET_KEY".to_string())
        );

        // 深いパス（最後のセグメントのみ）
        assert_eq!(
            infer_env_var_name("op://FleetFlowVault/prod-database/section/app_pass"),
            Some("APP_PASS".to_string())
        );

        // 無効なケース
        assert_eq!(infer_env_var_name("not-op-reference"), None);
        assert_eq!(infer_env_var_name("op://Vault/Item/"), None);
    }

    #[test]
    fn test_resolve_from_env_fallback() {
        // SAFETY: テスト環境での環境変数操作
        // 他のテストと競合しないユニークな変数名を使用
        unsafe {
            // テスト用の環境変数を設定
            let test_key = "TEST_SECRET_VALUE_12345";
            env::set_var("MY_TEST_VAR_123", test_key);

            // 環境変数から解決されることを確認
            let result = try_resolve_from_env("op://Vault/Item/my_test_var_123");
            assert_eq!(result, Some(test_key.to_string()));

            // クリーンアップ
            env::remove_var("MY_TEST_VAR_123");
        }

        // 環境変数がない場合はNone
        let result = try_resolve_from_env("op://Vault/Item/nonexistent_var");
        assert_eq!(result, None);
    }

    #[test]
    fn test_resolve_reference_env_fallback() {
        // SAFETY: テスト環境での環境変数操作
        // 他のテストと競合しないユニークな変数名を使用
        unsafe {
            // テスト用の環境変数を設定
            let test_value = "test_stripe_key_xyz";
            env::set_var("STRIPE_SECRET_FOR_TEST", test_value);

            // 環境変数があれば1Password CLIを呼ばずに解決
            let result =
                resolve_reference("op://FleetFlowVault/shared/stripe/stripe_secret_for_test");
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), test_value);

            // クリーンアップ
            env::remove_var("STRIPE_SECRET_FOR_TEST");
        }
    }

    #[test]
    fn test_is_available() {
        // このテストは環境依存
        // 1Password CLIがインストールされている場合はtrueを返す
        let available = is_available();
        println!("1Password CLI available: {}", available);
    }

    // 注意: 以下のテストは実際の1Password環境が必要なため、
    // CI環境ではスキップされます

    #[test]
    #[ignore = "requires 1Password CLI and authentication"]
    fn test_resolve_reference() {
        // 実際の1Password参照をテスト
        let result = resolve_reference("op://FleetFlowVault/creo-dev-database/password");
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_reference_invalid() {
        let result = resolve_reference("not-an-op-reference");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("op://"));
    }
}
