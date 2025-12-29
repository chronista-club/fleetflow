#![allow(deprecated)] // TODO: cargo_bin → cargo_bin_cmd! へ移行

use assert_cmd::Command;
mod common;
use common::TestProject;
use std::net::TcpListener;

/// ポート競合時の自動クリーンアップテスト
///
/// Docker依存: コンテナ起動・ポートバインディングが必要
/// 実行方法: `cargo test --test port_cleanup_test -- --ignored`
#[tokio::test]
#[ignore = "Docker依存テスト - CI Tier2で実行"]
async fn test_port_cleanup_on_up() {
    let project = TestProject::new();
    let port = 18083;

    // 1. 意図的にポートを占有する
    // Note: TcpListener を別スレッドで保持し続ける
    let _listener =
        TcpListener::bind(format!("127.0.0.1:{}", port)).expect("Failed to bind to test port");

    // 2. プロジェクト設定を作成
    project.write_flow_kdl(&format!(
        r#"
project "test-port-cleanup"

stage "local" {{
    service "web"
}}

service "web" {{
    image "nginx:alpine"
    ports {{
        port host={} container=80
    }}
}}
"#,
        port
    ));

    // 3. 起動 (up)
    // ポートが占有されているが、FleetFlow が lsof で PID を見つけ、
    // SIGTERM -> (wait) -> 解放検知 を行うことを期待。
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    cmd.current_dir(project.path())
        .arg("up")
        .arg("-s")
        .arg("local")
        .assert()
        .success();

    // 4. 検証: コンテナが起動していること
    let container_name = "test-port-cleanup-local-web";
    assert!(project.docker_container_exists(container_name).await);

    // 5. 削除
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    cmd.current_dir(project.path())
        .arg("down")
        .arg("-s")
        .arg("local")
        .arg("--remove")
        .assert()
        .success();
}
