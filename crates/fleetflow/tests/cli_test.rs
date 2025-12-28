#![allow(deprecated)] // TODO: cargo_bin → cargo_bin_cmd! へ移行

use assert_cmd::Command;
use predicates::prelude::*;

/// CLIヘルプが正しく表示されることを確認
#[test]
fn test_cli_help() {
    let mut cmd = Command::cargo_bin("flow").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Docker Composeよりシンプル"))
        .stdout(predicate::str::contains("up"))
        .stdout(predicate::str::contains("down"))
        .stdout(predicate::str::contains("ps"))
        .stdout(predicate::str::contains("logs"));
}

/// バージョン表示が正しく動作することを確認
#[test]
fn test_cli_version() {
    let mut cmd = Command::cargo_bin("flow").unwrap();
    cmd.arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains("fleetflow"));
}

/// validateコマンドのヘルプが正しく表示されることを確認
#[test]
fn test_validate_help() {
    let mut cmd = Command::cargo_bin("flow").unwrap();
    cmd.arg("validate")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--stage"))
        .stdout(predicate::str::contains("-s"));
}

/// upコマンドのヘルプが正しく表示されることを確認
#[test]
fn test_up_help() {
    let mut cmd = Command::cargo_bin("flow").unwrap();
    cmd.arg("up")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--stage"))
        .stdout(predicate::str::contains("-s"))
        .stdout(predicate::str::contains("--pull"))
        .stdout(predicate::str::contains("FLEET_STAGE"));
}

/// downコマンドのヘルプが正しく表示されることを確認
#[test]
fn test_down_help() {
    let mut cmd = Command::cargo_bin("flow").unwrap();
    cmd.arg("down")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("--stage"))
        .stdout(predicate::str::contains("--remove"));
}

/// 不正なコマンドでエラーになることを確認
#[test]
fn test_invalid_command() {
    let mut cmd = Command::cargo_bin("flow").unwrap();
    cmd.arg("invalid-command").assert().failure();
}

/// ステージ未指定でvalidateを実行するとエラーになることを確認
/// （プロジェクトディレクトリ外で実行）
#[test]
fn test_validate_without_project() {
    let mut cmd = Command::cargo_bin("flow").unwrap();
    cmd.current_dir(std::env::temp_dir())
        .arg("validate")
        .assert()
        .failure();
}
