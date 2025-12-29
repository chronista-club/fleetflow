#![allow(deprecated)] // TODO: cargo_bin → cargo_bin_cmd! へ移行

use assert_cmd::Command;
mod common;
use common::TestProject;
use std::fs;

/// 設定オーバーライドのテスト
///
/// Docker依存: コンテナ起動・環境変数検証が必要
/// 実行方法: `cargo test --test override_test -- --ignored`
#[tokio::test]
#[ignore = "Docker依存テスト - CI Tier2で実行"]
async fn test_config_override() {
    let project = TestProject::new();

    // 1. メイン設定を作成
    project.write_flow_kdl(
        r#"
project "test-override"

stage "local" {
    service "web"
}

service "web" {
    image "nginx:alpine"
    environment {
        APP_NAME "original"
    }
}
"#,
    );

    // 2. ローカルオーバーライドを作成
    fs::write(
        project.path().join("flow.local.kdl"),
        r#"
service "web" {
    environment {
        APP_NAME "overridden"
    }
}
"#,
    )
    .unwrap();

    // 3. 起動
    let mut cmd = Command::cargo_bin("fleet").unwrap();
    cmd.current_dir(project.path())
        .arg("up")
        .arg("-s")
        .arg("local")
        .assert()
        .success();

    // 4. 検証: 環境変数が上書きされていること
    // Docker APIでコンテナ情報を取得
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let container_name = "test-override-local-web";
    let inspect = docker
        .inspect_container(
            container_name,
            None::<bollard::query_parameters::InspectContainerOptions>,
        )
        .await
        .unwrap();
    let env = inspect.config.unwrap().env.unwrap();

    assert!(env.contains(&"APP_NAME=overridden".to_string()));
    assert!(!env.contains(&"APP_NAME=original".to_string()));

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
