//! In-process OBS WebSocket v5 server for tests.
//!
//! Speaks just enough of the protocol for `obws` to identify, call `GetVersion`,
//! and for the raw client to send requests and batches.

use std::net::SocketAddr;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Mutex};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;

use crate::auth::authentication_string;

#[derive(Clone)]
struct MockScenes {
    program: String,
    preview: Option<String>,
    fail_program: bool,
}

impl Default for MockScenes {
    fn default() -> Self {
        Self {
            program: "Live".into(),
            preview: None,
            fail_program: false,
        }
    }
}

#[derive(Clone)]
pub struct MockObs {
    pub port: u16,
    requests: Arc<Mutex<Vec<Value>>>,
    events: broadcast::Sender<String>,
    drop_tx: broadcast::Sender<()>,
    password: Option<String>,
    scenes: Arc<Mutex<MockScenes>>,
}

impl MockObs {
    pub async fn spawn(password: Option<&str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock OBS");
        let port = listener.local_addr().expect("addr").port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (events, _) = broadcast::channel(32);
        let (drop_tx, _) = broadcast::channel(8);
        let scenes = Arc::new(Mutex::new(MockScenes::default()));
        let password = password.map(str::to_string);
        let server = Self {
            port,
            requests: Arc::clone(&requests),
            events: events.clone(),
            drop_tx: drop_tx.clone(),
            password: password.clone(),
            scenes: Arc::clone(&scenes),
        };
        let password = server.password.clone();
        tokio::spawn(async move {
            loop {
                let Ok((stream, addr)) = listener.accept().await else {
                    break;
                };
                let requests = Arc::clone(&requests);
                let events = events.clone();
                let mut drops = drop_tx.subscribe();
                let password = password.clone();
                let scenes = Arc::clone(&scenes);
                tokio::spawn(async move {
                    if let Err(error) =
                        handle(stream, addr, password, requests, events, &mut drops, scenes).await
                    {
                        tracing::debug!(%error, "mock OBS connection ended");
                    }
                });
            }
        });
        server
    }

    pub async fn set_scenes(&self, program: &str, preview: Option<&str>) {
        let mut scenes = self.scenes.lock().await;
        scenes.program = program.to_string();
        scenes.preview = preview.map(str::to_string);
        scenes.fail_program = false;
    }

    pub async fn fail_program_scene(&self) {
        self.scenes.lock().await.fail_program = true;
    }

    pub fn push_event(&self, event_type: &str, event_data: Value) {
        let message = json!({
            "op": 5,
            "d": {
                "eventType": event_type,
                "eventIntent": 1,
                "eventData": event_data,
            }
        });
        let _ = self.events.send(message.to_string());
    }

    pub async fn requests(&self) -> Vec<Value> {
        self.requests.lock().await.clone()
    }

    pub fn drop_clients(&self) {
        let _ = self.drop_tx.send(());
    }
}

async fn handle(
    stream: tokio::net::TcpStream,
    _addr: SocketAddr,
    password: Option<String>,
    requests: Arc<Mutex<Vec<Value>>>,
    events: broadcast::Sender<String>,
    drops: &mut broadcast::Receiver<()>,
    scenes: Arc<Mutex<MockScenes>>,
) -> Result<(), String> {
    let stream = tokio_tungstenite::accept_async(stream)
        .await
        .map_err(|error| error.to_string())?;
    let (mut write, mut read) = stream.split();
    let mut events = events.subscribe();

    let hello = if password.is_some() {
        json!({
            "op": 0,
            "d": {
                "obsWebSocketVersion": "5.5.4",
                "rpcVersion": 1,
                "authentication": { "challenge": "challenge", "salt": "salt" }
            }
        })
    } else {
        json!({
            "op": 0,
            "d": { "obsWebSocketVersion": "5.5.4", "rpcVersion": 1 }
        })
    };
    write
        .send(Message::text(hello.to_string()))
        .await
        .map_err(|error| error.to_string())?;

    let identify = next_json(&mut read).await?;
    if identify.get("op").and_then(Value::as_u64) != Some(1) {
        return Err("expected Identify".into());
    }
    if let Some(password) = &password {
        let expected = authentication_string(password, "salt", "challenge");
        let given = identify
            .pointer("/d/authentication")
            .and_then(Value::as_str)
            .unwrap_or("");
        if given != expected {
            let _ = write
                .send(Message::Close(Some(CloseFrame {
                    code: CloseCode::from(4009),
                    reason: "Authentication failed".into(),
                })))
                .await;
            return Err("authentication failed".into());
        }
    }
    write
        .send(Message::text(
            json!({"op": 2, "d": {"negotiatedRpcVersion": 1}}).to_string(),
        ))
        .await
        .map_err(|error| error.to_string())?;

    loop {
        tokio::select! {
            _ = drops.recv() => {
                let _ = write.send(Message::Close(None)).await;
                return Ok(());
            }
            event = events.recv() => {
                if let Ok(event) = event {
                    write.send(Message::text(event)).await.map_err(|error| error.to_string())?;
                }
            }
            incoming = read.next() => {
                let Some(incoming) = incoming else { return Ok(()); };
                let incoming = incoming.map_err(|error| error.to_string())?;
                if incoming.is_close() || incoming.is_ping() {
                    if incoming.is_ping() {
                        continue;
                    }
                    return Ok(());
                }
                let text = incoming.to_text().map_err(|error| error.to_string())?;
                let value: Value = serde_json::from_str(text).map_err(|error| error.to_string())?;
                requests.lock().await.push(value.clone());
                let op = value.get("op").and_then(Value::as_u64).unwrap_or(0);
                let data = value.get("d").cloned().unwrap_or(Value::Null);
                let response = match op {
                    6 => Some(answer_request(&data, &scenes).await),
                    8 => Some(answer_batch(&data, &scenes).await),
                    _ => None,
                };
                if let Some(response) = response {
                    write
                        .send(Message::text(response.to_string()))
                        .await
                        .map_err(|error| error.to_string())?;
                }
            }
        }
    }
}

async fn answer_request(data: &Value, scenes: &Mutex<MockScenes>) -> Value {
    let request_type = data
        .get("requestType")
        .and_then(Value::as_str)
        .unwrap_or("");
    let request_id = data.get("requestId").cloned().unwrap_or(Value::Null);
    let (ok, body) = response_data(request_type, data.get("requestData"), scenes).await;
    json!({
        "op": 7,
        "d": {
            "requestType": request_type,
            "requestId": request_id,
            "requestStatus": {"result": ok, "code": if ok { 100 } else { 604 }},
            "responseData": body,
        }
    })
}

async fn answer_batch(data: &Value, scenes: &Mutex<MockScenes>) -> Value {
    let request_id = data.get("requestId").cloned().unwrap_or(Value::Null);
    let requests = data
        .get("requests")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut results = Vec::new();
    for request in requests {
        let request_type = request
            .get("requestType")
            .and_then(Value::as_str)
            .unwrap_or("");
        let (ok, body) = response_data(request_type, request.get("requestData"), scenes).await;
        results.push(json!({
            "requestType": request_type,
            "requestStatus": {"result": ok, "code": if ok { 100 } else { 604 }},
            "responseData": body,
        }));
    }
    json!({
        "op": 9,
        "d": { "requestId": request_id, "results": results }
    })
}

async fn response_data(
    request_type: &str,
    request_data: Option<&Value>,
    scenes: &Mutex<MockScenes>,
) -> (bool, Value) {
    match request_type {
        "GetVersion" => (
            true,
            json!({
                "obsStudioVersion": "31.0.0",
                "obsWebSocketVersion": "5.5.4",
                "rpcVersion": 1,
                "availableRequests": ["GetVersion"],
                "supportedImageFormats": ["png"],
                "platform": "macos",
                "platformDescription": "mock"
            }),
        ),
        "GetStreamStatus" => (
            true,
            json!({
                "outputActive": false,
                "outputReconnecting": false,
                "outputTimecode": "00:00:00.000",
                "outputDuration": 0,
                "outputCongestion": 0.0,
                "outputBytes": 0,
                "outputSkippedFrames": 0,
                "outputTotalFrames": 0
            }),
        ),
        "ToggleStream" | "StartStream" | "StopStream" => (true, json!({"outputActive": true})),
        "GetSceneList" => {
            let scenes = scenes.lock().await;
            let mut body = json!({
                "currentProgramSceneName": scenes.program,
                "currentProgramSceneUuid": "11111111-1111-1111-1111-111111111111",
                "scenes": [{
                    "sceneName": scenes.program,
                    "sceneUuid": "11111111-1111-1111-1111-111111111111",
                    "sceneIndex": 0
                }]
            });
            if let Some(preview) = &scenes.preview {
                body["currentPreviewSceneName"] = json!(preview);
                body["currentPreviewSceneUuid"] = json!("22222222-2222-2222-2222-222222222222");
            }
            (true, body)
        }
        "GetCurrentProgramScene" => {
            let scenes = scenes.lock().await;
            if scenes.fail_program {
                (false, json!({}))
            } else {
                (true, scene_body(&scenes.program))
            }
        }
        "SetCurrentProgramScene" => {
            if let Some(name) = request_data
                .and_then(|data| data.get("sceneName"))
                .and_then(Value::as_str)
            {
                scenes.lock().await.program = name.to_string();
            }
            (true, json!({}))
        }
        "GetCurrentPreviewScene" => {
            let scenes = scenes.lock().await;
            match &scenes.preview {
                Some(name) => (true, scene_body(name)),
                None => (false, json!({})),
            }
        }
        "SetCurrentPreviewScene" => {
            if let Some(name) = request_data
                .and_then(|data| data.get("sceneName"))
                .and_then(Value::as_str)
            {
                scenes.lock().await.preview = Some(name.to_string());
            }
            (true, json!({}))
        }
        _ => (true, json!({})),
    }
}

fn scene_body(name: &str) -> Value {
    json!({
        "sceneName": name,
        "sceneUuid": "11111111-1111-1111-1111-111111111111"
    })
}

async fn next_json(
    read: &mut futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    >,
) -> Result<Value, String> {
    loop {
        let message = read
            .next()
            .await
            .ok_or_else(|| "closed".to_string())?
            .map_err(|error| error.to_string())?;
        if message.is_ping() || message.is_pong() {
            continue;
        }
        let text = message.to_text().map_err(|error| error.to_string())?;
        return serde_json::from_str(text).map_err(|error| error.to_string());
    }
}
