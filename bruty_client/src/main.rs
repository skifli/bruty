use bruty_share;
use chrono;
use clap::Parser;
use futures_util::SinkExt;
use futures_util::StreamExt;
use log;
use tokio;
use tokio_tungstenite;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

const AUTHOR: &str = env!("CARGO_PKG_AUTHORS");
const VERSION: &str = env!("CARGO_PKG_VERSION");

// Type aliases to save my sanity
pub type Message = tokio_tungstenite::tungstenite::Message;
pub type Result = tokio_tungstenite::tungstenite::Result<()>;

// Mainly for this one lol
pub type WebSocketSender = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tokio_tungstenite::tungstenite::Message,
>;

/// An extension trait for `SplitSink` that adds methods for sending payloads and closing the connection.
pub trait SplitSinkExt {
    /// Closes the WebSocket connection.
    ///
    /// # Returns
    /// * `Result` - A `Result` indicating success or failure.
    fn close(&mut self) -> impl std::future::Future<Output = Result> + Send;

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
    async fn close(&mut self) -> Result {
        self.send(tokio_tungstenite::tungstenite::Message::Close(None))
            .await
    }

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

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;

            websocket_sender
                .send_payload(bruty_share::Payload {
                    op_code: bruty_share::OperationCode::Heartbeat,
                    data: bruty_share::Data::Heartbeat,
                })
                .await
                .unwrap(); // Send a heartbeat to the server

            log::debug!("Sent heartbeat");
        }
    }); // Create heartbeat sender

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
    let remote_url = "ws://bruty.shuttle.dev/ivocord";

    log::info!("Bruty Client v{} by {}.", VERSION, AUTHOR);

    bruty_share::logger::setup(
        true,
        Some(
            chrono::DateTime::<chrono::Local>::from(std::time::SystemTime::now())
                .format("bruty_%d-%m-%Y_%H:%M:%S.log")
                .to_string(),
        ),
    )
    .unwrap();

    create_connection(remote_url, args.id, args.secret).await;
}
