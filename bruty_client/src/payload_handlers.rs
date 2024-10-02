use futures_util::SinkExt;

use crate::WebSocketSender;

/// Run when the server has requested us to test video IDs.
///
/// # Arguments
/// * `websocket_sender` - The WebSocket sender.
/// * `payload` - The WebSocket payload.
/// * `client_channels` - The client's channels, used for communication between threads.
pub async fn test_request_data(
    websocket_sender: &mut WebSocketSender,
    payload: bruty_share::Payload,
    client_channels: &bruty_share::types::ClientChannels,
) {
    let test_request_data = match payload.data {
        bruty_share::Data::TestRequestData(test_request_data) => test_request_data,
        _ => {
            log::error!("Invalid payload data for TestRequestData OP code");

            websocket_sender.close().await.unwrap();

            return;
        }
    };

    log::info!(
        "Generating IDs for {}",
        test_request_data.id.iter().collect::<String>()
    );

    client_channels
        .base_id_sender
        .send(test_request_data.id)
        .unwrap();
}
