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
        .send(test_request_data.id.clone())
        .unwrap(); // Send of the base ID so permutations can be generated

    let client_channels_clone = client_channels.clone();

    tokio::spawn(async move {
        let starting_time = std::time::Instant::now();
        let mut positives = vec![];

        for _ in 0..262144 {
            let video = client_channels_clone.results_receiver.recv().unwrap();

            if video.event != bruty_share::types::VideoEvent::NotFound {
                positives.push(video);
            }
        }

        let elapsed_time = starting_time.elapsed().as_secs();

        log::info!(
            "Sending {} positive{} for {} in {}s ({}/s)",
            positives.len(),
            if positives.len() == 1 { "" } else { "s" },
            test_request_data.id.iter().collect::<String>(),
            elapsed_time,
            262144 / elapsed_time
        );

        client_channels_clone
            .payload_send_sender
            .send(bruty_share::Payload {
                op_code: bruty_share::OperationCode::TestingResult,
                data: bruty_share::Data::TestingResult(bruty_share::TestingResultData {
                    id: test_request_data.id,
                    positives,
                }),
            })
            .unwrap();
    });
}
