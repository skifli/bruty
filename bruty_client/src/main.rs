use bruty_share;
use chrono;
use clap::Parser;
use flume;
use futures_util::SinkExt;
use futures_util::StreamExt;
use log;
use tokio;
use tokio_tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

mod payload_handlers;

const AUTHOR: &str = env!("CARGO_PKG_AUTHORS");
const VERSION: &str = env!("CARGO_PKG_VERSION");

// Type aliases to save my sanity, lol
pub type Message = tokio_tungstenite::tungstenite::Message;
pub type Result = tokio_tungstenite::tungstenite::Result<()>;

// Mainly for this one, lol
pub type WebSocketSender = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tokio_tungstenite::tungstenite::Message,
>;

/// An extension trait for `SplitSink` that adds methods for sending payloads, and closing the connection.
pub trait SplitSinkExt {
    /// Sends a payload to the WebSocket connection.
    ///
    /// # Arguments
    /// * `payload` - The payload to send.
    ///
    /// # Returns
    /// * `Result` - A `Result` indicating success or failure.
    fn send_payload(
        &mut self,
        payload: bruty_share::Payload,
    ) -> impl std::future::Future<Output = Result> + Send;
}

impl SplitSinkExt for WebSocketSender {
    async fn send_payload(&mut self, payload: bruty_share::Payload) -> Result {
        // Convert payload to binary and send it
        self.send(tokio_tungstenite::tungstenite::Message::Binary(
            rmp_serde::to_vec(&payload).expect("Failed to serialize payload"),
        ))
        .await
    }
}

#[derive(Parser, Debug)]
#[command(
    author = AUTHOR,
    version = VERSION,
    about = "Brute-forces the rest of a YouTube video ID when you have part of it"
)]
struct Args {
    /// The id used for authentication.
    id: i16,

    /// The secret used for authentication.
    secret: String,
}

/// Handles a WebSocket message.
///
/// # Arguments
/// * `websocket_sender` - The WebSocket sender.
/// * `payload_send_sender` - The sender for sending payloads.
/// * `reqwest_client` - The reqwest client.
/// * `msg` - The WebSocket message.
///
/// # Returns
/// * `bool` - Whether the connection shouldn't be closed.
async fn handle_msg(
    websocket_sender: &mut WebSocketSender,
    payload_send_sender: &flume::Sender<bruty_share::Payload>,
    reqwest_client: &reqwest::Client,
    msg: Message,
) -> bool {
    let payload: bruty_share::Payload = match rmp_serde::from_read(msg.into_data().as_slice()) {
        Ok(payload) => payload,
        Err(err) => {
            log::error!("Failed to deserialize payload: {}", err);

            return false;
        }
    };

    #[allow(unreachable_patterns)]
    match payload.op_code {
        bruty_share::OperationCode::InvalidSession => {
            // The session is invalid
            let error_code = match payload.data {
                bruty_share::Data::InvalidSession(error_code) => error_code,
                _ => {
                    log::error!("InvalidSession payload data is not an error code");

                    return false;
                }
            };

            log::error!(
                "Invalid session: {} - {}",
                error_code.description,
                error_code.explanation
            );

            if error_code.code == bruty_share::ErrorCode::UnsupportedClientVersion {
                log::error!("Unsupported client version, please update");

                std::process::exit(1);
            } else if error_code.code == bruty_share::ErrorCode::AuthenticationFailed {
                log::error!("Authentication failed, please check your ID and secret");

                std::process::exit(1);
            }

            return false;
        }
        bruty_share::OperationCode::TestRequestData => {
            payload_handlers::test_request_data(
                websocket_sender,
                payload_send_sender,
                reqwest_client,
                payload,
            )
            .await;
        }
        bruty_share::OperationCode::Identify
        | bruty_share::OperationCode::TestRequest
        | bruty_share::OperationCode::TestingResult => {
            // We should never receive these from the client
            // Likely a bug, so we are going to close the connection
            log::warn!("Received unexpected OP code, closing connection");

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

/// Handles a WebSocket connection.
///
/// # Arguments
/// * `websocket_stream` - The WebSocket stream.
/// * `user` - The user.
async fn handle_connection(
    websocket_stream: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    user: bruty_share::types::User,
) {
    let (mut websocket_sender, mut websocket_receiver) = websocket_stream.split();
    let (payload_send_sender, payload_send_receiver) = flume::unbounded();

    let reqwest_client = reqwest::Client::new();

    websocket_sender
        .send_payload(bruty_share::Payload {
            op_code: bruty_share::OperationCode::Identify,
            data: bruty_share::Data::Identify(bruty_share::IdentifyData {
                client_version: VERSION.to_string(),
                id: user.id,
                secret: user.secret,
            }),
        })
        .await
        .unwrap(); // Identify to the server

    let payload_send_sender_clone = payload_send_sender.clone();

    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        payload_send_sender_clone
            .send(bruty_share::Payload {
                op_code: bruty_share::OperationCode::TestRequest,
                data: bruty_share::Data::TestRequest,
            })
            .unwrap();
    }); // In another thread so we terminate before if we are on an old client version.

    loop {
        tokio::select! {
            result = websocket_receiver.next() => {
                let msg = match result {
                    Some(Ok(msg)) => msg,
                    Some(Err(err)) => {
                        log::error!("Error receiving message from WebSocket: {}", err);

                        return;
                    },
                    None => {
                        log::error!("Connection closed");

                        return;
                    }
                };

                if msg.is_binary() {
                    // Binary WebSocket message received
                    if !handle_msg(&mut websocket_sender, &payload_send_sender, &reqwest_client, msg).await {
                        return;
                    }
                } else if msg.is_close() {
                    // Client requested to close the connection
                    break;
                } else {
                    // Invalid WebSocket message type received
                    // We don't care about text, ping, pong, etc.

                    log::warn!("Invalid WebSocket message type received (expected binary)");
                    continue;
                }
            }
            msg = payload_send_receiver.recv_async() => {
                if let Ok(payload) = msg {
                    websocket_sender.send_payload(payload).await.unwrap();
                }
            }
        }
    }
}

/// Creates a WebSocket connection to the server.
///
/// # Arguments
/// * `remote_url` - The URL of the server.
/// * `id` - The id used for authentication.
/// * `secret` - The secret used for authentication.
async fn create_connection(remote_url: &str, id: i16, secret: String) {
    let mut request = remote_url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("User-Agent", "bruty".parse().unwrap());

    let (websocket_stream, _) = tokio_tungstenite::connect_async(request)
        .await
        .unwrap_or_else(|err| {
            log::error!("Failed to connect to WebSocket: {}", err);

            std::process::exit(1);
        });

    log::info!("WebSocket handshake completed");

    handle_connection(
        websocket_stream,
        bruty_share::types::User {
            id,
            name: "unknown".to_string(),
            secret,
        },
    )
    .await;
}

#[tokio::main]
async fn main() {
    let args: Args = Args::parse();
    let remote_url = "wss://bruty.shuttleapp.rs/ivocord";

    bruty_share::logger::setup(
        true,
        Some(
            chrono::DateTime::<chrono::Local>::from(std::time::SystemTime::now())
                .format("bruty_%d-%m-%Y_%H-%M-%S.log")
                .to_string(),
        ),
    )
    .unwrap();

    log::info!("Bruty Client v{} by {}.", VERSION, AUTHOR);

    let server_status_client = reqwest::Client::new();

    loop {
        let req = server_status_client
            .get("https://bruty.shuttleapp.rs/status")
            .send()
            .await;

        let mut failed = false;

        if req.is_err() {
            failed = true;
        } else {
            let req = req.unwrap();

            if req.status().as_u16() != 200 {
                failed = true;
            }
        }

        if failed {
            log::warn!("Server status was not 200 (I am probably updating the logins). Retrying in 5 seconds.");

            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        } else {
            create_connection(remote_url, args.id, args.secret.clone()).await;

            log::warn!("Connection to server was lost, trying to connect again in 5 seconds.");
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }
    }
}
