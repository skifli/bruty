use bruty_share;
use flume;
use futures_util::SinkExt;
use futures_util::StreamExt;
use log;
use shuttle_persist;
use tokio;
use warp;
use warp::Filter;

mod payload_handlers;
mod server_threads;

const AUTHOR: &str = env!("CARGO_PKG_AUTHORS");
const VERSION: &str = env!("CARGO_PKG_VERSION");

type WebSocketSender = futures_util::stream::SplitSink<warp::ws::WebSocket, warp::ws::Message>;

/// An extension trait for `SplitSink` that adds methods for sending payloads.
pub trait SplitSinkExt {
    /// Sends a payload to the WebSocket connection.
    ///
    /// # Arguments
    /// * `payload` - The payload to send.
    ///
    /// # Returns
    /// * `Result<(), warp::Error>` - The result of sending the payload.
    fn send_payload(
        &mut self,
        payload: bruty_share::Payload,
    ) -> impl std::future::Future<Output = std::result::Result<(), warp::Error>>;
}

/// An extension trait for `SplitStream` that adds methods for receiving payloads.
impl SplitSinkExt for WebSocketSender {
    async fn send_payload(
        &mut self,
        payload: bruty_share::Payload,
    ) -> std::result::Result<(), warp::Error> {
        self.send(warp::ws::Message::binary(
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
/// * `server_channels` - The server's channels.
///
/// # Returns
/// * `bool` - Whether the connection shouldn't be closed.
async fn handle_msg(
    websocket_sender: &mut WebSocketSender,
    msg: warp::ws::Message,
    session: &mut bruty_share::types::Session,
    persist: shuttle_persist::PersistInstance,
    server_channels: bruty_share::types::ServerChannels,
) -> bool {
    let payload: bruty_share::Payload = match rmp_serde::from_slice(&msg.as_bytes()) {
        Ok(payload) => payload,
        Err(err) => {
            log::error!(
                "Failed to deserialize message from {} (ID {}), sent from {}: {}.",
                session.user.name,
                session.user.id,
                session.ip,
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
            // Resets heartbeat timer
            log::debug!(
                "Received heartbeat from {} (ID {}), sent from {}.",
                session.user.name,
                session.user.id,
                session.ip
            );
            payload_handlers::heartbeat(session).await;
        }
        bruty_share::OperationCode::Identify => {
            // Identifies the client
            payload_handlers::identify(websocket_sender, payload, session, persist).await;
        }
        bruty_share::OperationCode::TestRequest => {
            // Requests a test
            payload_handlers::test_request(websocket_sender, session, server_channels).await;
        }
        bruty_share::OperationCode::TestingResult => {
            // Process the test results
            payload_handlers::testing_result(websocket_sender, payload, session, server_channels)
                .await;
        }
        bruty_share::OperationCode::TestRequestData
        | bruty_share::OperationCode::InvalidSession => {
            // We should never receive these from the client
            // Likely a bug, so we are going to close the connection
            log::warn!(
                "Received unexpected OP code from {} (ID {}), sent from {}.",
                session.user.name,
                session.user.id,
                session.ip
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
/// * `server_channels` - The server's channels.
async fn handle_websocket(
    websocket: warp::ws::WebSocket,
    session: &mut bruty_share::types::Session,
    persist: shuttle_persist::PersistInstance,
    server_channels: bruty_share::types::ServerChannels,
) {
    log::info!("Established WebSocket connection with {}.", session.ip);

    let (mut websocket_sender, mut websocket_receiver) = websocket.split();

    let mut abruptly_closed = false;
    let mut manually_closed = false;

    let mut heartbeat_timer = Box::pin(tokio::time::sleep_until(session.heartbeat_timer));

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

                if msg.is_binary() {
                    // Binary WebSocket message received
                    if !handle_msg(
                        &mut websocket_sender,
                        msg,
                        session,
                        persist.clone(),
                        server_channels.clone(),
                    )
                    .await
                    {
                        manually_closed = true;
                        break;
                    }
                } else if msg.is_close() {
                    // Client requested to close the connection
                    break;
                } else {
                    // Invalid WebSocket message type received
                    // We don't care about text, ping, pong, etc.

                    log::warn!("Invalid WebSocket message type received from {} (ID {}), sent from {}.", session.user.name, session.user.id, session.ip);
                    continue;
                }

                if session.heartbeat_received { // If a heartbeat was received this iteration
                    heartbeat_timer = Box::pin(tokio::time::sleep_until(session.heartbeat_timer)); // Reset the heartbeat timer

                    session.heartbeat_received = false;
                }
            }
            _ = &mut heartbeat_timer => {
                // Heartbeat timer expired
                // Close the connection
                // We could, like Discord, request a heartbeat from the client, but that's more complex
                // Anyway, we require one every 5 seconds and the client SHOULD send one every 3 seconds, so that's plenty of time

                log::warn!("Heartbeat timer expired for {} (ID {}), connected to {}.", session.user.name, session.user.id, session.ip);

                websocket_sender
                .send_payload(bruty_share::Payload {
                    op_code: bruty_share::OperationCode::InvalidSession,
                    data: bruty_share::Data::InvalidSession(
                        bruty_share::ErrorCode::SessionTimeout.populate(),
                    ),
                })
                .await
                .unwrap();
                websocket_sender.close().await.unwrap();

                manually_closed = true;

                break;
            }
        }
    }

    if abruptly_closed {
        log::warn!(
            "WebSocket connection with {} (ID {}), connected from {} was abruptly closed.",
            session.user.name,
            session.user.id,
            session.ip
        );

        websocket_sender.close().await.unwrap();
    } else if manually_closed {
        log::info!(
            "Manually closed WebSocket connection with {} (ID {}), connected from {}.",
            session.user.name,
            session.user.id,
            session.ip
        );
    } else {
        log::info!(
            "Gracefully closed WebSocket connection with {} (ID {}), connected from {}.",
            session.user.name,
            session.user.id,
            session.ip
        );
    }

    if !session.awaiting_results.is_empty() {
        log::info!(
            "Cleaning up session for {} (ID {}), connected from {}.",
            session.user.name,
            session.user.id,
            session.ip
        );

        // We are awaiting results from this session, but it's gone. So, send the results to the next session.
        server_channels
            .id_sender
            .send(session.awaiting_results.clone())
            .unwrap();

        server_channels
            .results_awaiting_sender
            .send(session.awaiting_results.clone())
            .unwrap();

        log::info!(
            "Forwaded awaiting result from {} (ID {}) to the next session.",
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
/// * `server_channels` - The server's channels.
/// * `headers` - The headers of the request.
/// * `addr` - The originator's IP address.
///
/// # Returns
/// * `impl warp::Reply` - The result of handling the connection.
fn handle_connection(
    ws: warp::ws::Ws,
    persist: shuttle_persist::PersistInstance,
    server_channels: bruty_share::types::ServerChannels,
    headers: warp::http::HeaderMap,
    addr: Option<std::net::SocketAddr>,
) -> impl warp::Reply {
    ws.on_upgrade(move |websocket| async move {
        let ip = addr
            .map(|a| a.ip().to_string())
            .unwrap_or("unknown".to_string());

        let user_agent = headers
            .get("user-agent")
            .map(|ua| ua.to_str().unwrap())
            .unwrap_or("unknown");

        if user_agent != "bruty" {
            log::info!(
                "Rejected TCP connection from {} with user agent {}.",
                ip,
                user_agent
            );
            websocket.close().await.unwrap();
        } else {
            handle_websocket(
                websocket,
                &mut bruty_share::types::Session {
                    authenticated: false,
                    awaiting_results: Vec::new(),
                    ip,
                    heartbeat_received: true,
                    heartbeat_timer: tokio::time::Instant::now()
                        + tokio::time::Duration::from_secs(5),
                    user: bruty_share::types::User {
                        id: 0,
                        name: "unknown".to_string(),
                        secret: "".to_string(),
                    },
                },
                persist,
                server_channels,
            )
            .await;
        }
    })
}

#[shuttle_runtime::main]
async fn main(
    #[shuttle_persist::Persist] persist: shuttle_persist::PersistInstance,
) -> shuttle_warp::ShuttleWarp<(impl warp::Reply,)> {
    bruty_share::logger::setup(true, None).unwrap(); // Setup logger without a file because we are in a server environment

    log::info!("Bruty Server v{} by {}.", VERSION, AUTHOR);

    persist
        .save(
            "users",
            vec![
                bruty_share::types::User {
                    id: 0,
                    name: "skifli".to_string(),
                    secret: "1cwkj3".to_string(),
                },
                bruty_share::types::User {
                    id: 1,
                    name: "duvox".to_string(),
                    secret: "acw34".to_string(),
                },
                bruty_share::types::User {
                    id: 1,
                    name: "gost".to_string(),
                    secret: "32a;^%".to_string(),
                },
                bruty_share::types::User {
                    id: 1,
                    name: "finsanity".to_string(),
                    secret: "wev;23".to_string(),
                },
            ],
        )
        .unwrap();

    let mut state: bruty_share::types::ServerState = persist.load("server_state").unwrap();

    let (id_sender, id_receiver) = flume::unbounded(); // Create a channel for when the current project ID changes.
    let (results_sender, results_receiver) = flume::unbounded(); // Create a channel for when the server receives results, to send to the result handler.
    let (results_awaiting_sender, results_awaiting_receiver) = flume::unbounded(); // Create a channel for when the server is awaiting results.
    let (results_received_sender, results_received_receiver) = flume::unbounded(); // Create a channel for when the server receives results.
    let (current_id_sender, current_id_receiver) = flume::bounded(1); // Create a channel for when the current project ID changes.

    let id_sender_clone = id_sender.clone();
    let results_awaiting_sender_clone = results_awaiting_sender.clone();

    let server_channels = bruty_share::types::ServerChannels {
        id_receiver,
        id_sender: id_sender_clone,
        results_sender,
        results_received_sender,
        results_awaiting_sender: results_awaiting_sender_clone,
    }; // Bundle the server's sender channels for the websocket

    let mut starting_id_clone = state.starting_id.clone(); // Clone both for the permutation generator
    let current_id_clone = state.current_id.clone();

    // Start the permutation generator
    tokio::spawn(async move {
        server_threads::permutation_generator(
            &mut starting_id_clone,
            current_id_clone,
            &id_sender,
            &results_awaiting_sender,
            &current_id_sender,
        );
    });

    let persist_clone = persist.clone(); // Clone for the results handler

    // Start the results progress handler
    tokio::spawn(async move {
        server_threads::results_progress_handler(
            &results_awaiting_receiver,
            &results_received_receiver,
            &current_id_receiver,
            persist_clone,
            &mut state,
        )
        .await;
    });

    // Start the results handler
    tokio::spawn(async move {
        server_threads::results_handler(results_receiver).await;
    });

    // Creates the WebSocket route
    let routes = warp::path("ivocord")
        .and(warp::ws()) // Make the route a WebSocket route
        .and(warp::any().map(move || persist.clone())) // Clone the persist instance
        .and(warp::any().map(move || server_channels.clone())) // Clone the server's sender channels
        .and(warp::header::headers_cloned()) // Get the headers of the request
        .and(warp::addr::remote()) // Get the originator's IP address
        .map(
            |ws: warp::ws::Ws,
             persist: shuttle_persist::PersistInstance,
             server_channels: bruty_share::types::ServerChannels,
             headers: warp::http::HeaderMap,
             addr: Option<std::net::SocketAddr>| {
                handle_connection(ws, persist, server_channels, headers, addr)
            },
        ); // Handle the connection

    Ok(routes.boxed().into())
}
