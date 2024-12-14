use axum;
use bruty_share;
use flume;
use futures_util::SinkExt;
use futures_util::StreamExt;
use log;
/* use shuttle_persist; */
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

/// An extension trait for `SplitStream` that adds methods for receiving payloads.
impl SplitSinkExt for WebSocketSender {
    async fn send_payload(
        &mut self,
        payload: bruty_share::Payload,
    ) -> std::result::Result<(), axum::Error> {
        self.send(axum::extract::ws::Message::Binary(
            rmp_serde::to_vec(&payload).unwrap_or_else(|err| {
                log::error!("Failed to serialize payload: {}.", err);
                vec![]
            }),
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
/// * `persist` - The database connection.
/// * `server_data` - The server's data, with channels used for communication between threads.
///
/// # Returns
/// * `bool` - Whether the connection shouldn't be closed.
async fn handle_msg(
    websocket_sender: &mut WebSocketSender,
    binary_msg: Vec<u8>,
    session: &mut bruty_share::types::Session,
    /* persist: &shuttle_persist::PersistInstance, */
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
            payload_handlers::identify(
                websocket_sender,
                payload,
                session,
                /* persist, */
                server_data,
            )
            .await;
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
/// * `persist` - The database connection.
/// * `server_data` - The server's data, with channels used for communication between threads.
async fn handle_websocket(
    websocket: axum::extract::ws::WebSocket,
    session: &mut bruty_share::types::Session,
    /* persist: shuttle_persist::PersistInstance, */
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
                            binary_msg,
                            session,
                            /* &persist, */
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
            .id_sender
            .send(session.awaiting_result.clone())
            .unwrap();

        server_data
            .results_awaiting_sender
            .send(session.awaiting_result.clone())
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
/// * `persist` - The database connection.
/// * `server_data` - The server's data, with channels used for communication between threads.
/// * `headers` - The headers of the request.
///
/// # Returns
/// * `impl axum::response::IntoResponse` - The response to send.
async fn handle_connection(
    ws: axum::extract::WebSocketUpgrade,
    headers: axum::http::HeaderMap,
    /* persist: axum::extract::Extension<shuttle_persist::PersistInstance>, */
    server_data: axum::extract::Extension<bruty_share::types::ServerData>,
) -> impl axum::response::IntoResponse {
    ws.on_upgrade(move |websocket| async move {
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
                /* persist.0, */
                server_data.0,
            )
            .await;
        }
    })
}

#[shuttle_runtime::main]
async fn main(
    /* #[shuttle_persist::Persist] persist: shuttle_persist::PersistInstance, */
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

    /*
    let mut state: bruty_share::types::ServerState =
        persist.load("server_state").unwrap_or_else(|_| {
            log::warn!("No server state found in the database, creating a new one.");

            let state = bruty_share::types::ServerState {
                starting_id: vec!['M', 'w', 'b', 'C'],
                current_id: vec!['M', 'w', 'b', 'C'],
            };

            persist.save("server_state", state.clone()).unwrap();

            state
        });
    */

    let mut state = bruty_share::types::ServerState {
        starting_id: vec!['M', 'w', 'b', 'C'],
        current_id: vec!['M', 'w', 'b', 'C'],
    }; // !REMOVE AFTER PERSIST IS RE-ENABLED

    log::info!("Users: {:?}", users);
    log::info!("Server State: {:?}", state);

    let (id_sender, id_receiver) = flume::unbounded(); // Create a channel for when the current project ID changes.
    let (results_sender, results_receiver) = flume::unbounded(); // Create a channel for when the server receives results, to send to the result handler.
    let (results_awaiting_sender, results_awaiting_receiver) = flume::unbounded(); // Create a channel for when the server is awaiting results.
    let (results_received_sender, results_received_receiver) = flume::unbounded(); // Create a channel for when the server receives results.

    let id_sender_clone = id_sender.clone();
    let results_awaiting_sender_clone = results_awaiting_sender.clone();

    let server_data = bruty_share::types::ServerData {
        id_receiver,
        id_sender: id_sender_clone,
        results_sender,
        results_received_sender,
        results_awaiting_sender: results_awaiting_sender_clone,
        users,
        users_connected_num: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
    }; // Bundle the server's sender channels for the websocket

    let mut starting_id_clone = state.starting_id.clone(); // Clone both for the permutation generator
    let current_id_clone = state.current_id.clone();

    let users_connected_num_clone = server_data.users_connected_num.clone();

    // Start the permutation generator
    tokio::spawn(async move {
        server_threads::permutation_generator(
            &mut starting_id_clone,
            &current_id_clone,
            &id_sender,
            &results_awaiting_sender,
            &users_connected_num_clone,
        );
    });

    /* let persist_clone = persist.clone(); // Clone for the results handler */

    // Start the results progress handler
    tokio::spawn(async move {
        server_threads::results_progress_handler(
            &results_awaiting_receiver,
            &results_received_receiver,
            /* persist_clone, */
            &mut state,
        )
        .await;
    });

    // Start the results handler
    tokio::spawn(async move {
        server_threads::results_handler(results_receiver).await;
    });

    let router = axum::Router::new()
        .route("/ivocord", axum::routing::get(handle_connection))
        /* .layer(axum::Extension(persist)) */
        .layer(axum::Extension(server_data))
        .route("/status", axum::routing::get(|| async { "OK" }));

    Ok(router.into())
}
