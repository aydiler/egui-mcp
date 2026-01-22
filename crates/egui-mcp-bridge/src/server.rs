//! TCP server for bridge communication.

use crate::protocol::{
    ClickParams, FocusParams, GetValueParams, HoverParams, Request, RequestId, Response,
    ScrollParams, SetValueParams, SnapshotResponse, SuccessResponse, TypeTextParams, ValueResponse,
    INTERNAL_ERROR, INVALID_PARAMS, METHOD_NOT_FOUND, NODE_NOT_FOUND,
};
use egui::accesskit::NodeId;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

/// Command sent from the server to the bridge.
#[derive(Debug)]
pub enum BridgeCommand {
    GetSnapshot {
        respond: oneshot::Sender<SnapshotResponse>,
    },
    Click {
        node_id: NodeId,
        respond: oneshot::Sender<Result<(), String>>,
    },
    Focus {
        node_id: NodeId,
        respond: oneshot::Sender<Result<(), String>>,
    },
    SetValue {
        node_id: NodeId,
        value: String,
        respond: oneshot::Sender<Result<(), String>>,
    },
    TypeText {
        node_id: NodeId,
        text: String,
        respond: oneshot::Sender<Result<(), String>>,
    },
    Hover {
        node_id: NodeId,
        respond: oneshot::Sender<Result<(), String>>,
    },
    GetValue {
        node_id: NodeId,
        respond: oneshot::Sender<Result<ValueResponse, String>>,
    },
    Scroll {
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
        respond: oneshot::Sender<Result<(), String>>,
    },
}

/// Simple oneshot channel implementation.
pub mod oneshot {
    use std::fmt;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    pub struct Sender<T> {
        inner: Arc<Mutex<Option<T>>>,
    }

    impl<T> fmt::Debug for Sender<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Sender").finish_non_exhaustive()
        }
    }

    pub struct Receiver<T> {
        inner: Arc<Mutex<Option<T>>>,
    }

    impl<T> fmt::Debug for Receiver<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Receiver").finish_non_exhaustive()
        }
    }

    pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
        let inner = Arc::new(Mutex::new(None));
        (
            Sender {
                inner: inner.clone(),
            },
            Receiver { inner },
        )
    }

    impl<T> Sender<T> {
        pub async fn send(self, value: T) {
            let mut guard = self.inner.lock().await;
            *guard = Some(value);
        }
    }

    impl<T> Receiver<T> {
        pub async fn recv(self) -> Option<T> {
            // Poll until we get a value (simple implementation)
            loop {
                {
                    let mut guard = self.inner.lock().await;
                    if guard.is_some() {
                        return guard.take();
                    }
                }
                tokio::task::yield_now().await;
            }
        }
    }
}

/// Bridge server state.
pub struct BridgeServer {
    command_tx: mpsc::Sender<BridgeCommand>,
}

impl BridgeServer {
    /// Create a new bridge server.
    pub fn new(command_tx: mpsc::Sender<BridgeCommand>) -> Self {
        Self { command_tx }
    }

    /// Start the TCP server on the given port.
    pub async fn run(self, port: u16) -> std::io::Result<()> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
        tracing::info!("Bridge server listening on port {}", port);

        loop {
            match listener.accept().await {
                Ok((stream, addr)) => {
                    tracing::info!("Client connected from {}", addr);
                    let command_tx = self.command_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_client(stream, command_tx).await {
                            tracing::error!("Client error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                }
            }
        }
    }
}

async fn handle_client(
    stream: TcpStream,
    command_tx: mpsc::Sender<BridgeCommand>,
) -> std::io::Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        if n == 0 {
            // Connection closed
            break;
        }

        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => handle_request(request, &command_tx).await,
            Err(e) => Response::error(
                RequestId::Number(0),
                crate::protocol::PARSE_ERROR,
                format!("Parse error: {}", e),
            ),
        };

        let response_json = serde_json::to_string(&response).unwrap() + "\n";
        writer.write_all(response_json.as_bytes()).await?;
    }

    Ok(())
}

async fn handle_request(request: Request, command_tx: &mpsc::Sender<BridgeCommand>) -> Response {
    let id = request.id.clone();

    match request.method.as_str() {
        "get_snapshot" => {
            let (tx, rx) = oneshot::channel();
            if command_tx
                .send(BridgeCommand::GetSnapshot { respond: tx })
                .await
                .is_err()
            {
                return Response::error(id, INTERNAL_ERROR, "Bridge disconnected");
            }
            match rx.recv().await {
                Some(snapshot) => Response::success(id, snapshot),
                None => Response::error(id, INTERNAL_ERROR, "No response from bridge"),
            }
        }

        "click" => {
            let params: ClickParams = match parse_params(&request) {
                Ok(p) => p,
                Err(e) => return Response::error(id, INVALID_PARAMS, e),
            };
            let (tx, rx) = oneshot::channel();
            if command_tx
                .send(BridgeCommand::Click {
                    node_id: NodeId(params.node_id),
                    respond: tx,
                })
                .await
                .is_err()
            {
                return Response::error(id, INTERNAL_ERROR, "Bridge disconnected");
            }
            match rx.recv().await {
                Some(Ok(())) => Response::success(
                    id,
                    SuccessResponse {
                        success: true,
                        message: None,
                    },
                ),
                Some(Err(e)) => Response::error(id, NODE_NOT_FOUND, e),
                None => Response::error(id, INTERNAL_ERROR, "No response from bridge"),
            }
        }

        "focus" => {
            let params: FocusParams = match parse_params(&request) {
                Ok(p) => p,
                Err(e) => return Response::error(id, INVALID_PARAMS, e),
            };
            let (tx, rx) = oneshot::channel();
            if command_tx
                .send(BridgeCommand::Focus {
                    node_id: NodeId(params.node_id),
                    respond: tx,
                })
                .await
                .is_err()
            {
                return Response::error(id, INTERNAL_ERROR, "Bridge disconnected");
            }
            match rx.recv().await {
                Some(Ok(())) => Response::success(
                    id,
                    SuccessResponse {
                        success: true,
                        message: None,
                    },
                ),
                Some(Err(e)) => Response::error(id, NODE_NOT_FOUND, e),
                None => Response::error(id, INTERNAL_ERROR, "No response from bridge"),
            }
        }

        "set_value" => {
            let params: SetValueParams = match parse_params(&request) {
                Ok(p) => p,
                Err(e) => return Response::error(id, INVALID_PARAMS, e),
            };
            let (tx, rx) = oneshot::channel();
            if command_tx
                .send(BridgeCommand::SetValue {
                    node_id: NodeId(params.node_id),
                    value: params.value,
                    respond: tx,
                })
                .await
                .is_err()
            {
                return Response::error(id, INTERNAL_ERROR, "Bridge disconnected");
            }
            match rx.recv().await {
                Some(Ok(())) => Response::success(
                    id,
                    SuccessResponse {
                        success: true,
                        message: None,
                    },
                ),
                Some(Err(e)) => Response::error(id, NODE_NOT_FOUND, e),
                None => Response::error(id, INTERNAL_ERROR, "No response from bridge"),
            }
        }

        "type_text" => {
            let params: TypeTextParams = match parse_params(&request) {
                Ok(p) => p,
                Err(e) => return Response::error(id, INVALID_PARAMS, e),
            };
            let (tx, rx) = oneshot::channel();
            if command_tx
                .send(BridgeCommand::TypeText {
                    node_id: NodeId(params.node_id),
                    text: params.text,
                    respond: tx,
                })
                .await
                .is_err()
            {
                return Response::error(id, INTERNAL_ERROR, "Bridge disconnected");
            }
            match rx.recv().await {
                Some(Ok(())) => Response::success(
                    id,
                    SuccessResponse {
                        success: true,
                        message: None,
                    },
                ),
                Some(Err(e)) => Response::error(id, NODE_NOT_FOUND, e),
                None => Response::error(id, INTERNAL_ERROR, "No response from bridge"),
            }
        }

        "hover" => {
            let params: HoverParams = match parse_params(&request) {
                Ok(p) => p,
                Err(e) => return Response::error(id, INVALID_PARAMS, e),
            };
            let (tx, rx) = oneshot::channel();
            if command_tx
                .send(BridgeCommand::Hover {
                    node_id: NodeId(params.node_id),
                    respond: tx,
                })
                .await
                .is_err()
            {
                return Response::error(id, INTERNAL_ERROR, "Bridge disconnected");
            }
            match rx.recv().await {
                Some(Ok(())) => Response::success(
                    id,
                    SuccessResponse {
                        success: true,
                        message: None,
                    },
                ),
                Some(Err(e)) => Response::error(id, NODE_NOT_FOUND, e),
                None => Response::error(id, INTERNAL_ERROR, "No response from bridge"),
            }
        }

        "get_value" => {
            let params: GetValueParams = match parse_params(&request) {
                Ok(p) => p,
                Err(e) => return Response::error(id, INVALID_PARAMS, e),
            };
            let (tx, rx) = oneshot::channel();
            if command_tx
                .send(BridgeCommand::GetValue {
                    node_id: NodeId(params.node_id),
                    respond: tx,
                })
                .await
                .is_err()
            {
                return Response::error(id, INTERNAL_ERROR, "Bridge disconnected");
            }
            match rx.recv().await {
                Some(Ok(value)) => Response::success(id, value),
                Some(Err(e)) => Response::error(id, NODE_NOT_FOUND, e),
                None => Response::error(id, INTERNAL_ERROR, "No response from bridge"),
            }
        }

        "scroll" => {
            let params: ScrollParams = match parse_params(&request) {
                Ok(p) => p,
                Err(e) => return Response::error(id, INVALID_PARAMS, e),
            };
            let (tx, rx) = oneshot::channel();
            if command_tx
                .send(BridgeCommand::Scroll {
                    x: params.x,
                    y: params.y,
                    delta_x: params.delta_x,
                    delta_y: params.delta_y,
                    respond: tx,
                })
                .await
                .is_err()
            {
                return Response::error(id, INTERNAL_ERROR, "Bridge disconnected");
            }
            match rx.recv().await {
                Some(Ok(())) => Response::success(
                    id,
                    SuccessResponse {
                        success: true,
                        message: None,
                    },
                ),
                Some(Err(e)) => Response::error(id, INTERNAL_ERROR, e),
                None => Response::error(id, INTERNAL_ERROR, "No response from bridge"),
            }
        }

        _ => Response::error(id, METHOD_NOT_FOUND, "Method not found"),
    }
}

fn parse_params<T: serde::de::DeserializeOwned>(request: &Request) -> Result<T, String> {
    let params = request
        .params
        .as_ref()
        .ok_or_else(|| "Missing params".to_string())?;
    serde_json::from_value(params.clone()).map_err(|e| format!("Invalid params: {}", e))
}
