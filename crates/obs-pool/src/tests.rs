use std::time::Duration;

use serde_json::json;

use crate::{
    mock::MockObs, resolve_targets, ConnectionStatus, ObsInstanceConfig, ObsPool, PoolEvent,
    PoolOptions, RawCall, TargetGroup, TargetSelector,
};

fn instance(id: &str, port: u16, password: &str) -> ObsInstanceConfig {
    ObsInstanceConfig {
        id: id.into(),
        name: id.into(),
        host: "127.0.0.1".into(),
        port,
        password: password.into(),
        color: "#4c8dff".into(),
        enabled: true,
    }
}

async fn wait_connected(pool: &ObsPool, id: &str) {
    let mut events = pool.subscribe();
    if pool
        .status(id)
        .await
        .is_some_and(|status| status.is_connected())
    {
        return;
    }
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            match events.recv().await {
                Ok(PoolEvent::Status(status))
                    if status.id == id && status.status.is_connected() =>
                {
                    return;
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(error) => panic!("event stream closed: {error}"),
            }
        }
    })
    .await
    .expect("timed out waiting for OBS connection");
}

#[test]
fn resolves_targets_in_configuration_order() {
    let instances = vec![
        instance("a", 1, ""),
        instance("b", 1, ""),
        ObsInstanceConfig {
            enabled: false,
            ..instance("c", 1, "")
        },
    ];
    let groups = vec![TargetGroup {
        id: "g".into(),
        name: "Both".into(),
        members: vec!["c".into(), "a".into()],
    }];

    assert_eq!(
        resolve_targets(&TargetSelector::All, &instances, &groups),
        vec!["a".to_string(), "b".to_string()]
    );
    assert_eq!(
        resolve_targets(
            &TargetSelector::Group { id: "g".into() },
            &instances,
            &groups
        ),
        vec!["a".to_string(), "c".to_string()]
    );
    assert_eq!(
        resolve_targets(
            &TargetSelector::Instances {
                ids: vec!["b".into(), "missing".into(), "a".into()]
            },
            &instances,
            &groups
        ),
        vec!["a".to_string(), "b".to_string()]
    );
}

#[tokio::test]
async fn connects_toggles_and_receives_events() {
    let server = MockObs::spawn(None).await;
    let pool = ObsPool::new(PoolOptions::for_tests());
    let mut events = pool.subscribe();
    pool.reconcile(vec![instance("main", server.port, "")], vec![])
        .await;
    wait_connected(&pool, "main").await;

    let client = pool.client("main").await.expect("client");
    let active = client.stream().toggle_stream().await.expect("toggle");
    assert!(active.output_active);

    server.push_event(
        "StreamStateChanged",
        json!({"outputActive": true, "outputState": "OBS_WEBSOCKET_OUTPUT_STARTED"}),
    );
    let event = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Ok(PoolEvent::Obs { id, event }) = events.recv().await {
                if id == "main" {
                    return event;
                }
            }
        }
    })
    .await
    .expect("event");
    assert!(matches!(
        event,
        obs_websocket::Event::StreamStateChanged(event) if event.output_active
    ));

    let listed = pool
        .raw_request("main", "GetSceneList", json!({}))
        .await
        .expect("raw");
    assert_eq!(listed["currentProgramSceneName"], "Live");

    let batch = pool
        .raw_batch("main", &[RawCall::new("GetVersion", json!({}))], false)
        .await
        .expect("batch");
    let data = batch
        .first()
        .and_then(|item| item.result.as_ref().ok())
        .expect("batch item");
    assert_eq!(data["obsVersion"], "31.0.0");
}

#[tokio::test]
async fn reports_authentication_failure_then_reconnects_after_drop() {
    let server = MockObs::spawn(Some("secret")).await;
    let pool = ObsPool::new(PoolOptions::for_tests());
    pool.reconcile(vec![instance("locked", server.port, "nope")], vec![])
        .await;

    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if matches!(
                pool.status("locked").await,
                Some(ConnectionStatus::AuthFailed { .. })
            ) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    })
    .await
    .expect("auth failure was not reported");

    let open = MockObs::spawn(None).await;
    let mut events = pool.subscribe();
    pool.reconcile(vec![instance("live", open.port, "")], vec![])
        .await;
    wait_connected(&pool, "live").await;
    open.drop_clients();

    tokio::time::timeout(Duration::from_secs(4), async {
        loop {
            if matches!(
                pool.status("live").await,
                Some(ConnectionStatus::Unreachable { .. } | ConnectionStatus::Connecting)
            ) {
                return;
            }
            let _ = events.recv().await;
        }
    })
    .await
    .expect("disconnect was not observed");

    wait_connected(&pool, "live").await;
}
