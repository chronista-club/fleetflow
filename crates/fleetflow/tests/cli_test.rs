#![allow(deprecated)] // TODO: cargo_bin → cargo_bin_cmd! へ移行

use assert_cmd::Command;
use predicates::prelude::*;

/// CLIヘルプが正しく表示されることを確認
#[test]
fn test_cli_help() {
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("環境構築は、対話になった"))
        .stdout(predicate::str::contains("up"))
        .stdout(predicate::str::contains("down"))
        .stdout(predicate::str::contains("ps"))
        .stdout(predicate::str::contains("logs"));
}

/// バージョン表示が正しく動作することを確認
#[test]
fn test_cli_version() {
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    cmd.arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains("fleetflow"));
}

/// validateコマンドのヘルプが正しく表示されることを確認
#[test]
fn test_validate_help() {
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    cmd.arg("validate")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("[STAGE]"));
}

/// upコマンドのヘルプが正しく表示されることを確認
#[test]
fn test_up_help() {
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    cmd.arg("up")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("[STAGE]"))
        .stdout(predicate::str::contains("--pull"));
}

/// downコマンドのヘルプが正しく表示されることを確認
#[test]
fn test_down_help() {
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    cmd.arg("down")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("[STAGE]"))
        .stdout(predicate::str::contains("--remove"));
}

/// 不正なコマンドでエラーになることを確認
#[test]
fn test_invalid_command() {
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    cmd.arg("invalid-command").assert().failure();
}

/// ステージ未指定でvalidateを実行するとエラーになることを確認
/// （プロジェクトディレクトリ外で実行）
#[test]
fn test_validate_without_project() {
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    cmd.current_dir(std::env::temp_dir())
        .arg("validate")
        .assert()
        .failure();
}

/// 位置引数でステージを指定できることを確認
#[test]
fn test_deploy_positional_stage() {
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    cmd.arg("deploy")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("[STAGE]"))
        .stdout(predicate::str::contains("--yes"));
}

/// -s/--stage フラグも引き続き使えることを確認（後方互換）
#[test]
fn test_deploy_flag_backward_compat() {
    // -s フラグはhiddenだがパースは可能（プロジェクト外なのでfleet.kdl不在でエラーになるが、引数パースエラーではない）
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    let result = cmd
        .current_dir(std::env::temp_dir())
        .arg("deploy")
        .arg("-s")
        .arg("prod")
        .arg("--yes")
        .assert()
        .failure();
    // 引数パースエラー（"unexpected argument"等）ではないことを確認
    result.stderr(predicate::str::contains("unexpected argument").not());
}

/// 位置引数と-sフラグの同時指定はエラーになることを確認
#[test]
fn test_deploy_conflict_positional_and_flag() {
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    cmd.arg("deploy")
        .arg("prod")
        .arg("-s")
        .arg("dev")
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot be used with"));
}

/// execコマンドのヘルプが正しく表示されることを確認
#[test]
fn test_exec_help() {
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    cmd.arg("exec")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("[STAGE]"))
        .stdout(predicate::str::contains("--service"))
        .stdout(predicate::str::contains("[COMMAND]"));
}
