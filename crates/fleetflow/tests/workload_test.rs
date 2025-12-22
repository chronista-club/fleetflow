use assert_cmd::Command;
mod common;
use common::TestProject;

#[tokio::test]
async fn test_workload_inclusion() {
    let project = TestProject::new();

    // 1. ワークロード定義を作成
    project.write_workload(
        "web-stack",
        r#"
service "nginx" {
    image "nginx:alpine"
    ports {
        port host=18082 container=80
    }
}
"#,
    );

    // 2. メイン設定でワークロードを使用
    project.write_flow_kdl(
        r#"
project "test-workload"
workload "web-stack"

stage "local" {
    service "nginx"
}
"#,
    );

    // 3. 起動
    let mut cmd = Command::cargo_bin("fleetflow").unwrap();
    cmd.current_dir(project.path())
        .arg("up")
        .arg("local")
        .assert()
        .success();

    // 検証: ワークロードで定義されたコンテナが起動していること
    let container_name = "test-workload-local-nginx";
    assert!(project.docker_container_exists(container_name).await);

    // 4. 削除
    let mut cmd = Command::cargo_bin("fleetflow").unwrap();
    cmd.current_dir(project.path())
        .arg("down")
        .arg("local")
        .arg("--remove")
        .assert()
        .success();

    assert!(!project.docker_container_exists(container_name).await);
}
