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
pub struct MockObs {
    pub port: u16,
    requests: Arc<Mutex<Vec<Value>>>,
    events: broadcast::Sender<String>,
    drop_tx: broadcast::Sender<()>,
    password: Option<String>,
}

impl MockObs {
    pub async fn spawn(password: Option<&str>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock OBS");
        let port = listener.local_addr().expect("addr").port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let (events, _) = broadcast::channel(32);
        let (drop_tx, _) = broadcast::channel(8);
        let server = Self {
            port,
            requests: Arc::clone(&requests),
            events: events.clone(),
            drop_tx: drop_tx.clone(),
            password: password.map(str::to_string),
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
                tokio::spawn(async move {
                    if let Err(error) = handle(stream, addr, password, requests, events, &mut drops).await
                    {
                        tracing::debug!(%error, "mock OBS connection ended");
                    }
                });
            }
        });
        server
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
                    6 => Some(answer_request(&data)),
                    8 => Some(answer_batch(&data)),
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

fn answer_request(data: &Value) -> Value {
    let request_type = data.get("requestType").and_then(Value::as_str).unwrap_or("");
    let request_id = data.get("requestId").cloned().unwrap_or(Value::Null);
    json!({
        "op": 7,
        "d": {
            "requestType": request_type,
            "requestId": request_id,
            "requestStatus": {"result": true, "code": 100},
            "responseData": response_data(request_type, data.get("requestData")),
        }
    })
}

fn answer_batch(data: &Value) -> Value {
    let request_id = data.get("requestId").cloned().unwrap_or(Value::Null);
    let results: Vec<Value> = data
        .get("requests")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|request| {
            let request_type = request.get("requestType").and_then(Value::as_str).unwrap_or("");
            json!({
                "requestType": request_type,
                "requestStatus": {"result": true, "code": 100},
                "responseData": response_data(request_type, request.get("requestData")),
            })
        })
        .collect();
    json!({
        "op": 9,
        "d": { "requestId": request_id, "results": results }
    })
}

fn response_data(request_type: &str, _request_data: Option<&Value>) -> Value {
    match request_type {
        "GetVersion" => json!({
            "obsStudioVersion": "31.0.0",
            "obsWebSocketVersion": "5.5.4",
            "rpcVersion": 1,
            "availableRequests": ["GetVersion"],
            "supportedImageFormats": ["png"],
            "platform": "macos",
            "platformDescription": "mock"
        }),
        "GetStreamStatus" => json!({
            "outputActive": false,
            "outputReconnecting": false,
            "outputTimecode": "00:00:00.000",
            "outputDuration": 0,
            "outputCongestion": 0.0,
            "outputBytes": 0,
            "outputSkippedFrames": 0,
            "outputTotalFrames": 0
        }),
        "ToggleStream" | "StartStream" | "StopStream" => json!({"outputActive": true}),
        "GetSceneList" => json!({
            "currentProgramSceneName": "Live",
            "currentProgramSceneUuid": "11111111-1111-1111-1111-111111111111",
            "scenes": [{
                "sceneName": "Live",
                "sceneUuid": "11111111-1111-1111-1111-111111111111",
                "sceneIndex": 0
            }]
        }),
        _ => json!({}),
    }
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
