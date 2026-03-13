//! Medium テスト: DeployEngine の Docker 操作テスト
//!
//! これらのテストは Docker デーモンが起動している環境で実行する必要がある。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use fleetflow_container::{DeployEngine, DeployEvent, DeployRequest};
use fleetflow_core::{Flow, Service, Stage};

fn make_flow(services: Vec<(&str, Service)>, stage_services: Vec<&str>) -> Flow {
    let mut svc_map = HashMap::new();
    for (name, svc) in services {
        svc_map.insert(name.to_string(), svc);
    }
    let mut stages = HashMap::new();
    stages.insert(
        "test".to_string(),
        Stage {
            services: stage_services.iter().map(|s| s.to_string()).collect(),
            servers: vec![],
            variables: HashMap::new(),
            registry: None,
        },
    );
    Flow {
        name: "engine-test".to_string(),
        services: svc_map,
        stages,
        providers: HashMap::new(),
        servers: HashMap::new(),
        registry: None,
        variables: HashMap::new(),
    }
}

/// 存在しないコンテナの停止が 404 エラーにならない
#[tokio::test]
async fn test_engine_stop_nonexistent() {
    let docker = match bollard::Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(_) => {
            eprintln!("Docker 未接続、テストをスキップ");
            return;
        }
    };
    if docker.ping().await.is_err() {
        eprintln!("Docker 未接続、テストをスキップ");
        return;
    }

    let flow = make_flow(
        vec![(
            "nonexistent-svc",
            Service {
                image: Some("alpine:latest".into()),
                ..Default::default()
            },
        )],
        vec!["nonexistent-svc"],
    );

    let engine = DeployEngine::new(docker);
    let request = DeployRequest {
        flow,
        stage_name: "test".into(),
        target_services: vec!["nonexistent-svc".into()],
        no_pull: true,
        no_prune: true,
    };

    // execute should not panic or error on nonexistent containers
    // It will fail at create/start since we skip pull and the image might not exist,
    // but stop/remove should succeed silently
    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();
    let _ = engine
        .execute(&request, move |event| {
            events_clone.lock().unwrap().push(event);
        })
        .await;

    // Step 1 (stop/remove) should complete without errors
    let collected = events.lock().unwrap();
    assert!(collected
        .iter()
        .any(|e| matches!(e, DeployEvent::StepCompleted { step: 1 })));
}

/// alpine コンテナの作成・起動・削除が動作する
#[tokio::test]
async fn test_engine_deploy_single_service() {
    let docker = match bollard::Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(_) => {
            eprintln!("Docker 未接続、テストをスキップ");
            return;
        }
    };
    if docker.ping().await.is_err() {
        eprintln!("Docker 未接続、テストをスキップ");
        return;
    }

    let flow = make_flow(
        vec![(
            "test-alpine",
            Service {
                image: Some("alpine:latest".into()),
                command: Some("sleep 10".into()),
                ..Default::default()
            },
        )],
        vec!["test-alpine"],
    );

    let engine = DeployEngine::new(docker.clone());
    let request = DeployRequest {
        flow,
        stage_name: "test".into(),
        target_services: vec!["test-alpine".into()],
        no_pull: false,
        no_prune: true,
    };

    let result = engine.execute(&request, |_event| {}).await;
    assert!(result.is_ok(), "デプロイが成功すること: {:?}", result.err());

    let result = result.unwrap();
    assert!(result.success);
    assert_eq!(result.services_deployed, vec!["test-alpine"]);

    // Cleanup: stop and remove the test container
    let container_name = "engine-test-test-test-alpine";
    docker
        .stop_container(
            container_name,
            None::<bollard::query_parameters::StopContainerOptions>,
        )
        .await
        .ok();
    docker
        .remove_container(
            container_name,
            Some(bollard::query_parameters::RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await
        .ok();

    // Cleanup network
    docker
        .remove_network("engine-test-test")
        .await
        .ok();
}

/// イベントが正しい順序で発行される
#[tokio::test]
async fn test_engine_events_order() {
    let docker = match bollard::Docker::connect_with_local_defaults() {
        Ok(d) => d,
        Err(_) => {
            eprintln!("Docker 未接続、テストをスキップ");
            return;
        }
    };
    if docker.ping().await.is_err() {
        eprintln!("Docker 未接続、テストをスキップ");
        return;
    }

    let flow = make_flow(
        vec![(
            "event-test",
            Service {
                image: Some("alpine:latest".into()),
                command: Some("sleep 5".into()),
                ..Default::default()
            },
        )],
        vec!["event-test"],
    );

    let engine = DeployEngine::new(docker.clone());
    let request = DeployRequest {
        flow,
        stage_name: "test".into(),
        target_services: vec!["event-test".into()],
        no_pull: false,
        no_prune: true,
    };

    let events = Arc::new(Mutex::new(Vec::new()));
    let events_clone = events.clone();
    let result = engine
        .execute(&request, move |event| {
            events_clone.lock().unwrap().push(event);
        })
        .await;
    assert!(result.is_ok());

    let collected = events.lock().unwrap();

    // StepStarted events should appear in order 1..5
    let step_starts: Vec<u8> = collected
        .iter()
        .filter_map(|e| {
            if let DeployEvent::StepStarted { step, .. } = e {
                Some(*step)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(step_starts, vec![1, 2, 3, 4, 5]);

    // StepCompleted events should appear in order 1..5
    let step_completions: Vec<u8> = collected
        .iter()
        .filter_map(|e| {
            if let DeployEvent::StepCompleted { step } = e {
                Some(*step)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(step_completions, vec![1, 2, 3, 4, 5]);

    // Should end with Completed event
    assert!(matches!(
        collected.last(),
        Some(DeployEvent::Completed { .. })
    ));

    // Cleanup
    let container_name = "engine-test-test-event-test";
    docker
        .stop_container(
            container_name,
            None::<bollard::query_parameters::StopContainerOptions>,
        )
        .await
        .ok();
    docker
        .remove_container(
            container_name,
            Some(bollard::query_parameters::RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await
        .ok();
    docker
        .remove_network("engine-test-test")
        .await
        .ok();
}
