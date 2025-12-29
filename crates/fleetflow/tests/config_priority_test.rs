#![allow(deprecated)] // TODO: cargo_bin → cargo_bin_cmd! へ移行

use assert_cmd::Command;
mod common;
use common::TestProject;
use std::fs;

/// 設定優先度の複合テスト（.env < flow.kdl < flow.{stage}.kdl < flow.local.kdl）
///
/// Docker依存: コンテナ起動・ポートバインディング検証が必要
/// 実行方法: `cargo test --test config_priority_test -- --ignored`
///
/// NOTE: ポートバインディングが空になる問題あり。bollardの使い方を要調査。
#[tokio::test]
#[ignore = "Docker依存テスト - CI Tier2で実行"]
async fn test_config_priority_complex() {
    let project = TestProject::new();

    // 1. .env (最底辺)
    fs::write(project.path().join(".env"), "DB_TAG=v1\nDB_PORT=8000").unwrap();

    // 2. flow.kdl (基本)
    project.write_flow_kdl(
        r#"
project "priority-test"

service "db" {
    image "surrealdb/surrealdb:{{ DB_TAG }}"
    command "start --user root --pass root --bind 0.0.0.0:8000 rocksdb:///data/database.db"
    ports {
        port host={{ DB_PORT }} container=8000
    }
}
"#,
    );

    // 3. flow.live.kdl (ステージ固有)
    fs::write(
        project.path().join("flow.live.kdl"),
        r#"
stage "live" {
    service "db"
}

service "db" {
    image "surrealdb/surrealdb:v2" // 明示的にイメージを上書き
    ports {
        port host=9001 container=8000 // ポートも上書き
    }
}
"#,
    )
    .unwrap();

    // 4. flow.local.kdl (ローカル固有 - 最優先)
    fs::write(
        project.path().join("flow.local.kdl"),
        r#"
service "db" {
    ports {
        port host=9999 container=8000 // さらにローカルで上書き
    }
}
"#,
    )
    .unwrap();

    // 5. デバッグ: 生成されたファイルの内容を確認
    println!("\n=== flow.kdl ===");
    println!(
        "{}",
        fs::read_to_string(project.path().join("flow.kdl")).unwrap()
    );
    println!("\n=== flow.prod.kdl ===");
    println!(
        "{}",
        fs::read_to_string(project.path().join("flow.prod.kdl")).unwrap()
    );
    println!("\n=== flow.local.kdl ===");
    println!(
        "{}",
        fs::read_to_string(project.path().join("flow.local.kdl")).unwrap()
    );

    // 6. 起動 (prodステージ)
    let mut cmd = Command::cargo_bin("flow").unwrap();
    let output = cmd
        .current_dir(project.path())
        .arg("up")
        .arg("--stage")
        .arg("prod")
        .output()
        .unwrap();

    if !output.status.success() {
        println!("\nSTDOUT: {}", String::from_utf8_lossy(&output.stdout));
        println!("STDERR: {}", String::from_utf8_lossy(&output.stderr));
        panic!("flow up failed");
    }

    // 6. 検証
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();

    // DB のイメージが "surrealdb/surrealdb:v2" (flow.prod.kdl) になっているか
    let db_inspect = docker
        .inspect_container(
            "priority-test-prod-db",
            None::<bollard::query_parameters::InspectContainerOptions>,
        )
        .await
        .unwrap();
    let db_image = db_inspect.config.unwrap().image.unwrap();
    println!("  DEBUG: DB Image: {}", db_image);
    assert!(
        db_image.contains("surrealdb/surrealdb:v2"),
        "Expected surrealdb/surrealdb:v2 but got {}",
        db_image
    );

    // DB のポートが 9999 (flow.local.kdl) になっているか
    let host_config = db_inspect.host_config.expect("host_config should exist");
    let port_bindings = host_config
        .port_bindings
        .expect("port_bindings should exist");

    println!("  DEBUG: All port bindings: {:?}", port_bindings);

    let port_binding = port_bindings
        .get("8000/tcp")
        .expect("8000/tcp port should be bound")
        .as_ref()
        .expect("port binding should have value");

    let host_port = port_binding[0]
        .host_port
        .as_ref()
        .expect("host_port should exist");

    println!("  DEBUG: DB Port: {}", host_port);
    assert_eq!(
        host_port, "9999",
        "Expected port 9999 (flow.local.kdl override) but got {}",
        host_port
    );

    // 7. クリーンアップ
    let mut cmd = Command::cargo_bin("flow").unwrap();
    cmd.current_dir(project.path())
        .arg("down")
        .arg("-s")
        .arg("prod")
        .arg("--remove")
        .assert()
        .success();
}
