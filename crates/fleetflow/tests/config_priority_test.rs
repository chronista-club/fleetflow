use assert_cmd::Command;
mod common;
use common::TestProject;
use std::fs;

#[tokio::test]
async fn test_config_priority_complex() {
    let project = TestProject::new();

    // 1. .env (最底辺)
    fs::write(project.path().join(".env"), "DB_TAG=15\nAPP_PORT=8000").unwrap();

    // 2. flow.kdl (基本)
    project.write_flow_kdl(
        "
project priority-test

service db {
    image postgres:{{ DB_TAG }}
}

service app {
    image nginx:alpine
    ports {
        port host={{ APP_PORT }} container=80
    }
}
",
    );

    // 3. flow.prod.kdl (ステージ固有)
    fs::write(
        project.path().join("flow.prod.kdl"),
        "
stage prod {
    service db
    service app
}

service db {
    image postgres:16 // 明示的にイメージを上書き
}
",
    )
    .unwrap();

    // 4. flow.local.kdl (ローカル固有 - 最優先)
    fs::write(
        project.path().join("flow.local.kdl"),
        "
service app {
    ports {
        port host=9000 container=80
    }
}
",
    )
    .unwrap();

    // 5. 起動 (prodステージ)
    let mut cmd = Command::cargo_bin("fleetflow").unwrap();
    cmd.current_dir(project.path())
        .arg("up")
        .arg("prod")
        .assert()
        .success();

    // 6. 検証
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();

    // DB のイメージが "postgres:16" (flow.prod.kdl) になっているか
    let db_inspect = docker
        .inspect_container(
            "priority-test-prod-db",
            None::<bollard::query_parameters::InspectContainerOptions>,
        )
        .await
        .unwrap();
    let db_image = db_inspect.config.unwrap().image.unwrap();
    println!("  DEBUG: DB Image: {}", db_image);
    assert!(db_image.contains("postgres:16"));

    // App のポートが 9000 (flow.local.kdl) になっているか
    let app_inspect = docker
        .inspect_container(
            "priority-test-prod-app",
            None::<bollard::query_parameters::InspectContainerOptions>,
        )
        .await
        .unwrap();
    let port_bindings = app_inspect.host_config.unwrap().port_bindings.unwrap();
    let host_port = port_bindings.get("80/tcp").unwrap().as_ref().unwrap()[0]
        .host_port
        .as_ref()
        .unwrap();
    println!("  DEBUG: App Port: {}", host_port);
    assert_eq!(host_port, "9000");

    // 7. クリーンアップ
    let mut cmd = Command::cargo_bin("fleetflow").unwrap();
    cmd.current_dir(project.path())
        .arg("down")
        .arg("prod")
        .arg("--remove")
        .assert()
        .success();
}
