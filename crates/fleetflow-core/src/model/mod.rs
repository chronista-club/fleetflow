//! モデル定義
//!
//! FleetFlowで使用されるデータモデルを定義します。
//! 各モデルは機能ごとにモジュールに分離されています。

mod cloud;
mod flow;
mod port;
mod process;
mod service;
mod stage;
mod volume;

// Re-exports
pub use cloud::*;
pub use flow::*;
pub use port::*;
pub use process::*;
pub use service::*;
pub use stage::*;
pub use volume::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_flow_creation() {
        let mut services = HashMap::new();
        services.insert(
            "api".to_string(),
            Service {
                image: Some("myapp:1.0.0".to_string()),
                ..Default::default()
            },
        );

        let mut stages = HashMap::new();
        stages.insert(
            "local".to_string(),
            Stage {
                services: vec!["api".to_string()],
                servers: vec![],
                variables: HashMap::new(),
                registry: None,
            },
        );

        let flow = Flow {
            name: "my-project".to_string(),
            services,
            stages,
            providers: HashMap::new(),
            servers: HashMap::new(),
            registry: None,
        };

        assert_eq!(flow.name, "my-project");
        assert_eq!(flow.services.len(), 1);
        assert_eq!(flow.stages.len(), 1);
        assert!(flow.services.contains_key("api"));
        assert!(flow.stages.contains_key("local"));
    }

    #[test]
    fn test_flow_to_flowconfig_conversion() {
        let mut services = HashMap::new();
        services.insert("db".to_string(), Service::default());

        let mut stages = HashMap::new();
        stages.insert("dev".to_string(), Stage::default());

        let flow = Flow {
            name: "test-flow".to_string(),
            services: services.clone(),
            stages: stages.clone(),
            providers: HashMap::new(),
            servers: HashMap::new(),
            registry: None,
        };

        assert_eq!(flow.services.len(), 1);
        assert_eq!(flow.stages.len(), 1);
        assert!(flow.services.contains_key("db"));
        assert!(flow.stages.contains_key("dev"));
    }

    #[test]
    fn test_process_creation() {
        let process = Process {
            id: "proc-123".to_string(),
            flow_name: "my-flow".to_string(),
            stage_name: "local".to_string(),
            service_name: "api".to_string(),
            container_id: Some("container-abc".to_string()),
            pid: Some(1234),
            state: ProcessState::Running,
            started_at: 1704067200,
            stopped_at: None,
            image: "myapp:1.0.0".to_string(),
            memory_usage: Some(256_000_000),
            cpu_usage: Some(5.5),
            ports: vec![],
        };

        assert_eq!(process.id, "proc-123");
        assert_eq!(process.flow_name, "my-flow");
        assert_eq!(process.state, ProcessState::Running);
        assert_eq!(process.pid, Some(1234));
        assert!(process.stopped_at.is_none());
    }

    #[test]
    fn test_process_state_transitions() {
        let states = vec![
            ProcessState::Starting,
            ProcessState::Running,
            ProcessState::Stopping,
            ProcessState::Stopped,
            ProcessState::Paused,
            ProcessState::Failed,
            ProcessState::Restarting,
        ];

        for state in states {
            let process = Process {
                id: "test".to_string(),
                flow_name: "test".to_string(),
                stage_name: "test".to_string(),
                service_name: "test".to_string(),
                container_id: None,
                pid: None,
                state: state.clone(),
                started_at: 0,
                stopped_at: None,
                image: "test".to_string(),
                memory_usage: None,
                cpu_usage: None,
                ports: vec![],
            };

            assert_eq!(process.state, state);
        }
    }

    #[test]
    fn test_process_with_resource_usage() {
        let process = Process {
            id: "proc-456".to_string(),
            flow_name: "resource-test".to_string(),
            stage_name: "local".to_string(),
            service_name: "db".to_string(),
            container_id: Some("container-xyz".to_string()),
            pid: Some(5678),
            state: ProcessState::Running,
            started_at: 1704067200,
            stopped_at: None,
            image: "postgres:16".to_string(),
            memory_usage: Some(512_000_000), // 512MB
            cpu_usage: Some(10.5),           // 10.5%
            ports: vec![Port {
                host: 5432,
                container: 5432,
                protocol: Protocol::Tcp,
                host_ip: None,
            }],
        };

        assert_eq!(process.memory_usage, Some(512_000_000));
        assert_eq!(process.cpu_usage, Some(10.5));
        assert_eq!(process.ports.len(), 1);
        assert_eq!(process.ports[0].host, 5432);
    }

    #[test]
    fn test_process_serialization() {
        let process = Process {
            id: "proc-789".to_string(),
            flow_name: "serialize-test".to_string(),
            stage_name: "local".to_string(),
            service_name: "api".to_string(),
            container_id: Some("container-123".to_string()),
            pid: Some(9999),
            state: ProcessState::Running,
            started_at: 1704067200,
            stopped_at: None,
            image: "myapp:latest".to_string(),
            memory_usage: Some(128_000_000),
            cpu_usage: Some(2.5),
            ports: vec![],
        };

        // JSON シリアライズ
        let json = serde_json::to_string(&process).unwrap();
        assert!(json.contains("proc-789"));
        assert!(json.contains("serialize-test"));

        // JSON デシリアライズ
        let deserialized: Process = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, process.id);
        assert_eq!(deserialized.flow_name, process.flow_name);
        assert_eq!(deserialized.state, process.state);
    }
}
