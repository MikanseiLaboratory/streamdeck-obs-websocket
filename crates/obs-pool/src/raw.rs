//! Minimal OBS WebSocket v5 client for requests `obws` does not expose.
//!
//! Used by the raw request and raw batch actions. Typed calls stay on `obws`.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::auth::authentication_string;

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// One entry inside a `RequestBatch`.
#[derive(Clone, Debug)]
pub struct RawCall {
    pub request_type: String,
    pub request_data: Value,
}

#[derive(Debug, Error)]
pub enum RawError {
    #[error("websocket error: {0}")]
    WebSocket(String),
    #[error("OBS closed the connection: {0}")]
    Closed(String),
    #[error("unexpected OBS message")]
    Protocol,
    #[error("authentication failed")]
    Auth,
    #[error("request `{request_type}` failed ({code}): {comment}")]
    Request {
        request_type: String,
        code: i64,
        comment: String,
    },
}

/// A single identified OBS WebSocket session that can send op 6 and op 8.
pub struct RawSession {
    write: futures_util::stream::SplitSink<WsStream, Message>,
    read: futures_util::stream::SplitStream<WsStream>,
    next_id: u64,
}

impl RawSession {
    pub async fn connect(host: &str, port: u16, password: Option<&str>) -> Result<Self, RawError> {
        let url = format!("ws://{host}:{port}");
        let (stream, _) = tokio::time::timeout(Duration::from_secs(5), connect_async(&url))
            .await
            .map_err(|_| RawError::WebSocket("connection timed out".into()))?
            .map_err(|error| RawError::WebSocket(error.to_string()))?;
        let (mut write, mut read) = stream.split();

        let hello = next_data(&mut read).await?;
        if hello.op != 0 {
            return Err(RawError::Protocol);
        }
        let rpc_version = hello
            .data
            .get("rpcVersion")
            .and_then(Value::as_u64)
            .unwrap_or(1);
        let authentication = match (
            hello.data.get("authentication"),
            password.filter(|value| !value.is_empty()),
        ) {
            (Some(auth), Some(password)) => {
                let challenge = auth.get("challenge").and_then(Value::as_str).unwrap_or("");
                let salt = auth.get("salt").and_then(Value::as_str).unwrap_or("");
                Some(authentication_string(password, salt, challenge))
            }
            (Some(_), None) => return Err(RawError::Auth),
            _ => None,
        };

        let identify = json!({
            "op": 1,
            "d": {
                "rpcVersion": rpc_version,
                "authentication": authentication,
            }
        });
        send_text(&mut write, &identify).await?;
        let identified = next_data(&mut read).await?;
        if identified.op == 0 && identified.closed {
            return Err(RawError::Auth);
        }
        if identified.op != 2 {
            return Err(RawError::Protocol);
        }

        Ok(Self {
            write,
            read,
            next_id: 1,
        })
    }

    pub async fn request(
        &mut self,
        request_type: &str,
        request_data: Value,
    ) -> Result<Value, RawError> {
        let request_id = self.alloc_id();
        send_text(
            &mut self.write,
            &json!({
                "op": 6,
                "d": {
                    "requestType": request_type,
                    "requestId": request_id,
                    "requestData": request_data,
                }
            }),
        )
        .await?;
        let response = self.recv_op(7, &request_id).await?;
        request_result(request_type, &response)
    }

    pub async fn batch(
        &mut self,
        requests: &[RawCall],
        halt_on_failure: bool,
    ) -> Result<Value, RawError> {
        let request_id = self.alloc_id();
        let requests: Vec<Value> = requests
            .iter()
            .map(|call| {
                json!({
                    "requestType": call.request_type,
                    "requestData": call.request_data,
                })
            })
            .collect();
        send_text(
            &mut self.write,
            &json!({
                "op": 8,
                "d": {
                    "requestId": request_id,
                    "haltOnFailure": halt_on_failure,
                    "executionType": 0,
                    "requests": requests,
                }
            }),
        )
        .await?;
        let response = self.recv_op(9, &request_id).await?;
        Ok(response)
    }

    fn alloc_id(&mut self) -> String {
        let id = self.next_id.to_string();
        self.next_id += 1;
        id
    }

    async fn recv_op(&mut self, op: u8, request_id: &str) -> Result<Value, RawError> {
        loop {
            let message = next_data(&mut self.read).await?;
            if message.op == op
                && message.data.get("requestId").and_then(Value::as_str) == Some(request_id)
            {
                return Ok(message.data);
            }
        }
    }
}

struct Incoming {
    op: u8,
    data: Value,
    closed: bool,
}

async fn next_data(
    read: &mut futures_util::stream::SplitStream<WsStream>,
) -> Result<Incoming, RawError> {
    let message = read
        .next()
        .await
        .ok_or_else(|| RawError::Closed("connection closed".into()))?
        .map_err(|error| RawError::WebSocket(error.to_string()))?;
    if message.is_close() {
        return Ok(Incoming {
            op: 0,
            data: Value::Null,
            closed: true,
        });
    }
    let text = message
        .to_text()
        .map_err(|error| RawError::WebSocket(error.to_string()))?;
    let value: Value = serde_json::from_str(text).map_err(|_| RawError::Protocol)?;
    Ok(Incoming {
        op: value.get("op").and_then(Value::as_u64).unwrap_or(255) as u8,
        data: value.get("d").cloned().unwrap_or(Value::Null),
        closed: false,
    })
}

async fn send_text(
    write: &mut futures_util::stream::SplitSink<WsStream, Message>,
    value: &Value,
) -> Result<(), RawError> {
    write
        .send(Message::text(value.to_string()))
        .await
        .map_err(|error| RawError::WebSocket(error.to_string()))
}

fn request_result(request_type: &str, data: &Value) -> Result<Value, RawError> {
    let status = data.get("requestStatus").cloned().unwrap_or(Value::Null);
    let ok = status
        .get("result")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !ok {
        return Err(RawError::Request {
            request_type: request_type.to_string(),
            code: status.get("code").and_then(Value::as_i64).unwrap_or(0),
            comment: status
                .get("comment")
                .and_then(Value::as_str)
                .unwrap_or("request failed")
                .to_string(),
        });
    }
    Ok(data
        .get("responseData")
        .cloned()
        .unwrap_or_else(|| json!({})))
}
