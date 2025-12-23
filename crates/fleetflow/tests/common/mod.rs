use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

pub struct TestProject {
    pub root: TempDir,
}

impl TestProject {
    pub fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        Self { root }
    }

    pub fn write_flow_kdl(&self, content: &str) {
        let path = self.root.path().join("flow.kdl");
        fs::write(path, content).unwrap();
    }

    #[allow(dead_code)]
    pub fn write_workload(&self, name: &str, content: &str) {
        let dir = self.root.path().join("workloads");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(format!("{}.kdl", name)), content).unwrap();
    }

    pub fn path(&self) -> PathBuf {
        self.root.path().to_path_buf()
    }

    #[allow(dead_code)]
    pub async fn docker_container_exists(&self, name: &str) -> bool {
        let docker = bollard::Docker::connect_with_local_defaults().unwrap();
        docker
            .inspect_container(
                name,
                None::<bollard::query_parameters::InspectContainerOptions>,
            )
            .await
            .is_ok()
    }

    #[allow(dead_code)]
    pub async fn docker_network_exists(&self, name: &str) -> bool {
        let docker = bollard::Docker::connect_with_local_defaults().unwrap();
        docker
            .inspect_network(
                name,
                None::<bollard::query_parameters::InspectNetworkOptions>,
            )
            .await
            .is_ok()
    }
}
