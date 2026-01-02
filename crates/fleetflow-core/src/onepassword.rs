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
//! ## 必要な環境
//!
//! - 1Password CLI (`op`) がインストールされていること
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

    let mut cmd = Command::new("op");
    cmd.arg("read").arg(reference);

    // OP_ACCOUNT環境変数が設定されている場合は使用
    if let Ok(account) = std::env::var("OP_ACCOUNT") {
        debug!(account = %account, "Using OP_ACCOUNT");
        cmd.arg("--account").arg(account);
    }

    let output = cmd
        .output()
        .map_err(|e| FlowError::OnePasswordError(format!("1Password CLI実行エラー: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);

        // エラーメッセージを解析して適切なヒントを提供
        let hint = if stderr.contains("not signed in") || stderr.contains("session expired") {
            "\nヒント: `op signin` でサインインするか、OP_SERVICE_ACCOUNT_TOKEN を設定してください"
        } else if stderr.contains("not found") {
            "\nヒント: Vault名、Item名、Field名が正しいか確認してください"
        } else if stderr.contains("multiple accounts") {
            "\nヒント: OP_ACCOUNT 環境変数でアカウントを指定してください"
        } else {
            ""
        };

        return Err(FlowError::OnePasswordError(format!(
            "1Password参照の解決に失敗: {}{}",
            stderr.trim(),
            hint
        )));
    }

    let secret = String::from_utf8_lossy(&output.stdout).trim().to_string();
    debug!("Successfully resolved 1Password reference");

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

    // 1Password CLIが利用可能かチェック
    if !is_available() {
        warn!("1Password CLI is not available, skipping op:// resolution");
        return Err(FlowError::OnePasswordError(
            "1Password CLI (op) がインストールされていないか、PATHに存在しません".to_string(),
        ));
    }

    info!(
        count = op_keys.len(),
        "Found 1Password references to resolve"
    );

    // 各参照を解決
    for (key, reference) in op_keys {
        match resolve_reference(&reference) {
            Ok(secret) => {
                variables.insert(key.clone(), serde_json::Value::String(secret));
                resolved_count += 1;
                debug!(key = %key, "Resolved 1Password reference");
            }
            Err(e) => {
                errors.push(format!("{} ({}): {}", key, reference, e));
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
