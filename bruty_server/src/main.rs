use axum;
use bruty_share;
use flume;
use futures_util::SinkExt;
use futures_util::StreamExt;
use log;
use shuttle_runtime::SecretStore;
use tokio;

mod payload_handlers;
mod server_threads;

const AUTHOR: &str = env!("CARGO_PKG_AUTHORS");
const VERSION: &str = env!("CARGO_PKG_VERSION");

type WebSocketSender =
    futures_util::stream::SplitSink<axum::extract::ws::WebSocket, axum::extract::ws::Message>;

/// An extension trait for `SplitSink` that adds methods for sending payloads.
pub trait SplitSinkExt {
    /// Sends a payload to the WebSocket connection.
    ///
    /// # Arguments
    /// * `payload` - The payload to send.
    ///
    /// # Returns
    /// * `Result<(), axum::Error>` - The result of sending the payload.
    fn send_payload(
        &mut self,
        payload: bruty_share::Payload,
    ) -> impl std::future::Future<Output = std::result::Result<(), axum::Error>>;
}

impl SplitSinkExt for WebSocketSender {
    async fn send_payload(
        &mut self,
        payload: bruty_share::Payload,
    ) -> std::result::Result<(), axum::Error> {
        self.send(axum::extract::ws::Message::Binary(
            rmp_serde::to_vec(&payload)
                .unwrap_or_else(|err| {
                    log::error!("Failed to serialize payload: {}.", err);

                    vec![]
                })
                .into(),
        ))
        .await
    }
}

/// Handles a WebSocket message.
///
/// # Arguments
///
/// * `websocket_sender` - The WebSocket sender.
/// * `msg` - The WebSocket message.
/// * `session` - The session of the connection.
/// * `server_data` - The server's data, with channels used for communication between threads.
///
/// # Returns
/// * `bool` - Whether the connection shouldn't be closed.
async fn handle_msg(
    websocket_sender: &mut WebSocketSender,
    binary_msg: Vec<u8>,
    session: &mut bruty_share::types::Session,
    server_data: &bruty_share::types::ServerData,
) -> bool {
    let payload: bruty_share::Payload = match rmp_serde::from_slice(&binary_msg) {
        Ok(payload) => payload,
        Err(err) => {
            log::error!(
                "Failed to deserialize message from {} (ID {}): {}.",
                session.user.name,
                session.user.id,
                err
            );

            websocket_sender
                .send_payload(bruty_share::Payload {
                    op_code: bruty_share::OperationCode::InvalidSession,
                    data: bruty_share::Data::InvalidSession(
                        bruty_share::ErrorCode::DecodeError.populate(),
                    ),
                })
                .await
                .unwrap();

            websocket_sender.close().await.unwrap();

            return true;
        }
    };

    #[allow(unreachable_patterns)]
    match payload.op_code {
        bruty_share::OperationCode::Heartbeat => {
            session.heartbeat_received = true;
        }
        bruty_share::OperationCode::Identify => {
            // Identifies the client
            payload_handlers::identify(websocket_sender, payload, session, server_data).await;
        }
        bruty_share::OperationCode::TestingResult => {
            // Process the test results
            payload_handlers::testing_result(websocket_sender, payload, session, server_data).await;
        }
        bruty_share::OperationCode::TestRequestData
        | bruty_share::OperationCode::InvalidSession => {
            // We should never receive these from the client
            // Likely a bug, so we are going to close the connection
            log::warn!(
                "Received unexpected OP code from {} (ID {}).",
                session.user.name,
                session.user.id,
            );

            websocket_sender
                .send_payload(bruty_share::Payload {
                    op_code: bruty_share::OperationCode::InvalidSession,
                    data: bruty_share::Data::InvalidSession(
                        bruty_share::ErrorCode::UnexpectedOP.populate(),
                    ),
                })
                .await
                .unwrap();

            websocket_sender.close().await.unwrap();

            return false;
        }
        _ => {
            // Invalid OP code received
            // However, since it's in the enum, we probably should be handling it... so it's likely TODO

            todo!("Invalid OP code received");
        }
    }

    return true;
}

/// Handles a WebSocket.
///
/// # Arguments
/// * `websocket` - The WebSocket connection.
/// * `session` - The session of the connection.
/// * `server_data` - The server's data, with channels used for communication between threads.
async fn handle_websocket(
    websocket: axum::extract::ws::WebSocket,
    session: &mut bruty_share::types::Session,
    server_data: bruty_share::types::ServerData,
) {
    log::info!("Established WebSocket connection");

    let (mut websocket_sender, mut websocket_receiver) = websocket.split();

    let mut abruptly_closed = false;
    let mut manually_closed = false;

    let mut heartbeat_timer = Box::pin(tokio::time::sleep_until(
        tokio::time::Instant::now() + tokio::time::Duration::from_secs(10),
    ));

    server_data
        .users_connected_num
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    loop {
        tokio::select! {
            result = websocket_receiver.next() => {
                let msg = match result {
                    Some(Ok(msg)) => msg,
                    Some(Err(_)) => {
                        // Assume the connection was abruptly closed.
                        // Usually this happens when the client executable was terminated.
                        // Notably, not if somehow the connection was lost.
                        // (I think lol)
                        abruptly_closed = true;

                        break;
                    },
                    None => {
                        // Assume the connection was closed by the client.
                        break;
                    }
                };

                match msg {
                    axum::extract::ws::Message::Binary(binary_msg) => {
                        // Binary WebSocket message received
                        if !handle_msg(
                            &mut websocket_sender,
                            binary_msg.to_vec(),
                            session,
                            &server_data,
                        )
                        .await
                        {
                            manually_closed = true;
                            break;
                        }
                    },
                    axum::extract::ws::Message::Close(_) => {
                        // Client requested to close the connection
                        break;
                    },
                    _ => {
                        // Invalid WebSocket message type received
                        // We don't care about text, ping, pong, etc.

                        log::warn!("Invalid WebSocket message type received from {} (ID {}).", session.user.name, session.user.id);
                        continue;
                    }
                }
            }
            _ = &mut heartbeat_timer => {
                if !session.heartbeat_received {
                    // The client didn't send a heartbeat in time
                    log::warn!("Heartbeat not received from {} (ID {}).", session.user.name, session.user.id);

                    websocket_sender
                        .send_payload(bruty_share::Payload {
                            op_code: bruty_share::OperationCode::InvalidSession,
                            data: bruty_share::Data::InvalidSession(
                                bruty_share::ErrorCode::SessionTimeout.populate(),
                            ),
                        })
                        .await
                        .unwrap();

                    manually_closed = true;

                    websocket_sender.close().await.unwrap();

                    break;
                }

                session.heartbeat_received = false;

                heartbeat_timer = Box::pin(tokio::time::sleep_until(
                    tokio::time::Instant::now() + tokio::time::Duration::from_secs(10),
                ));
            }
        }
    }

    if abruptly_closed {
        log::warn!(
            "WebSocket connection with {} (ID {}) was abruptly closed.",
            session.user.name,
            session.user.id,
        );

        websocket_sender.close().await.unwrap();
    } else if manually_closed {
        log::info!(
            "Manually closed WebSocket connection with {} (ID {}).",
            session.user.name,
            session.user.id,
        );
    } else {
        log::info!(
            "Gracefully closed WebSocket connection with {} (ID {}).",
            session.user.name,
            session.user.id,
        );
    }

    server_data
        .users_connected_num
        .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);

    if !session.awaiting_result.is_empty() {
        // We are awaiting results from this session, but it's gone. So, send the results to the next session.
        server_data
            .event_sender
            .send(bruty_share::types::ServerEvent::ResultsAwaiting(
                session.awaiting_result.clone(),
            ))
            .unwrap();

        log::info!(
            "Forwarded awaiting result from {} (ID {}) to the next session.",
            session.user.name,
            session.user.id
        );
    }
}

/// Handles a WebSocket connection.
///
/// # Arguments
/// * `ws` - The WebSocket upgrade.
/// * `server_data` - The server's data, with channels used for communication between threads.
/// * `headers` - The headers of the request.
///
/// # Returns
/// * `impl axum::response::IntoResponse` - The response to send.
async fn handle_connection(
    ws: axum::extract::WebSocketUpgrade,
    headers: axum::http::HeaderMap,
    server_data: axum::extract::Extension<bruty_share::types::ServerData>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |mut websocket| async move {
        let user_agent = headers
            .get("user-agent")
            .map(|ua| ua.to_str().unwrap())
            .unwrap_or("unknown");

        if user_agent != "bruty" {
            log::info!("Rejected TCP connection with user agent {}.", user_agent);

            websocket.close().await.unwrap();
        } else {
            handle_websocket(
                websocket,
                &mut bruty_share::types::Session {
                    authenticated: false,
                    awaiting_result: vec![],
                    heartbeat_received: false,
                    user: bruty_share::types::User {
                        id: 0,
                        name: "unknown".to_string(),
                        secret: "".to_string(),
                    },
                },
                server_data.0,
            )
            .await;
        }
    })
}

#[shuttle_runtime::main]
async fn main(
    #[shuttle_shared_db::Postgres] operator: shuttle_shared_db::SerdeJsonOperator,
    #[shuttle_runtime::Secrets] secrets: SecretStore,
) -> shuttle_axum::ShuttleAxum {
    log::info!("Bruty Server v{} by {}.", VERSION, AUTHOR);

    let users_vec: Vec<char> = secrets.get("USERS").unwrap().chars().collect();
    let mut users = vec![];

    for (index, user_id) in users_vec.iter().enumerate() {
        let user_name = secrets.get(&format!("USER_{}_NAME", user_id)).unwrap();
        let user_secret = secrets.get(&format!("USER_{}_SECRET", user_id)).unwrap();

        users.push(bruty_share::types::User {
            id: index as u8,
            name: user_name,
            secret: user_secret,
        });
    }

    // Create the struct holding the operator for the server state
    let mut server_state = bruty_share::types::ServerState { operator };
    // Get the starting ID we should start from this time (if its a new run)
    let secrets_starting_id: Vec<char> = secrets.get("STARTING_ID").unwrap().chars().collect();

    // Get the starting ID we started from last time
    let server_state_starting_id: Vec<char> = match server_state
        .operator
        .read_serialized::<bruty_share::types::ServerStateInner>("starting_id")
        .await
    {
        Ok(state_inner) => state_inner.inner,
        Err(err) => match err.kind() {
            opendal::ErrorKind::NotFound => {
                log::warn!("Starting ID not found in the database. Defaulting to [].");

                vec![]
            }
            _ => {
                log::error!(
                    "Unexpected error while reading the starting ID from server state: {}.",
                    err
                );

                std::process::exit(1);
            }
        },
    };

    if server_state_starting_id.is_empty() || secrets_starting_id != server_state_starting_id {
        // We are starting a new run for the first time || The starting ID has changed
        server_state
            .operator
            .write_serialized(
                "starting_id",
                &bruty_share::types::ServerStateInner {
                    inner: secrets_starting_id.clone(),
                },
            )
            .await
            .unwrap();

        server_state
            .operator
            .write_serialized(
                "current_id",
                &bruty_share::types::ServerStateInner { inner: vec![] },
            )
            .await
            .unwrap();
    }

    let server_state_current_id = server_state
        .operator
        .read_serialized::<bruty_share::types::ServerStateInner>("current_id")
        .await
        .unwrap_or_else(|err| {
            match err.kind() {
                opendal::ErrorKind::NotFound => {
                    log::warn!("Current ID not found in the database, which should be impossible at this point. Exiting.");
                }
                _ => {
                    log::error!(
                        "Unexpected error while reading the current ID from server state: {}.",
                        err
                    );
                }
            }

            std::process::exit(1);
        })
        .inner;

    log::info!("Users: {:?}", users);
    log::info!(
        "Current ID: {:?}, Starting ID: {:?}",
        server_state_current_id,
        server_state_starting_id
    );

    let (event_sender, event_receiver) = flume::unbounded();

    let server_data = bruty_share::types::ServerData {
        current_id: std::sync::Arc::new(tokio::sync::Mutex::new(vec![])),
        event_receiver,
        event_sender,
        users,
        users_connected_num: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
    }; // Bundle the server's sender channels for the websocket

    let server_data_clone = server_data.clone();
    let server_data_clone_clone = server_data.clone();
    let server_data_clone_clone_clone = server_data.clone();

    // Start the permutation generator
    tokio::spawn(async move {
        server_threads::permutation_generator(
            &server_state_starting_id,
            &server_state_current_id,
            &server_data_clone_clone_clone,
        )
        .await;
    });

    // Start the results progress handler
    tokio::spawn(async move {
        server_threads::results_progress_handler(&mut server_state, &server_data_clone_clone).await;
    });

    // Start the results handler
    tokio::spawn(async move { server_threads::results_handler(&server_data_clone).await });

    let router = axum::Router::new()
        .route("/ivocord", axum::routing::get(handle_connection))
        .layer(axum::Extension(server_data))
        .route("/status", axum::routing::get(|| async { "OK" }));

    Ok(router.into())
}
