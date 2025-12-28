use assert_cmd::Command;
mod common;
use common::TestProject;

#[tokio::test]
async fn test_stage_lifecycle() {
    let project = TestProject::new();

    // 1. 作成 (Up)
    project.write_flow_kdl(
        r#"
project "test-lifecycle"

stage "local" {
    service "web"
}

service "web" {
    image "nginx:alpine"
    ports {
        port host=18080 container=80
    }
}
"#,
    );

    let mut cmd = Command::cargo_bin("flow").unwrap();
    cmd.current_dir(project.path())
        .arg("up")
        .arg("--stage")
        .arg("local")
        .assert()
        .success();

    // 検証: コンテナが存在すること
    // コンテナ命名規則: {project}-{stage}-{service}
    let container_name = "test-lifecycle-local-web";
    assert!(project.docker_container_exists(container_name).await);
    assert!(project.docker_network_exists("test-lifecycle-local").await);

    // 2. 更新 (Update) - ポートを変更
    project.write_flow_kdl(
        r#"
project "test-lifecycle"

stage "local" {
    service "web"
}

service "web" {
    image "nginx:alpine"
    ports {
        port host=18081 container=80
    }
}
"#,
    );

    let mut cmd = Command::cargo_bin("flow").unwrap();
    cmd.current_dir(project.path())
        .arg("up")
        .arg("-s")
        .arg("local")
        .assert()
        .success();

    // 検証: 更新後もコンテナが存在すること
    assert!(project.docker_container_exists(container_name).await);

    // 3. 削除 (Down --remove)
    let mut cmd = Command::cargo_bin("flow").unwrap();
    cmd.current_dir(project.path())
        .arg("down")
        .arg("--stage")
        .arg("local")
        .arg("--remove")
        .assert()
        .success();

    // 検証: コンテナとネットワークが削除されていること
    assert!(!project.docker_container_exists(container_name).await);
    assert!(!project.docker_network_exists("test-lifecycle-local").await);
}
