use futures_util::SinkExt;

use crate::client_threads;
use crate::WebSocketSender;

/// Run when the server has requested us to test video IDs.
///
/// # Arguments
/// * `websocket_sender` - The sender for sending WebSocket messages.
/// * `payload_send_sender` - The sender for sending payloads.
/// * `reqwest_client` - The reqwest client.
/// * `payload` - The payload.
pub async fn test_request_data(
    websocket_sender: &mut WebSocketSender,
    payload_send_sender: &flume::Sender<bruty_share::Payload>,
    id_sender: &flume::Sender<Vec<char>>,
    positives_receiver: &flume::Receiver<bruty_share::types::Video>,
    payload: bruty_share::Payload,
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
        "Received request to test: {}",
        test_request_data.id.iter().collect::<String>()
    );

    let id_sender_clone = id_sender.clone();
    let payload_send_sender_clone = payload_send_sender.clone();
    let positives_receiver_clone = positives_receiver.clone();

    tokio::spawn(async move {
        let start_time = tokio::time::Instant::now();

        client_threads::generate_ids(&id_sender_clone, test_request_data.id.clone()).await;

        let mut positives = Vec::new();

        for _ in 0..4096 {
            let video = positives_receiver_clone.recv().unwrap();

            if video.event != bruty_share::types::VideoEvent::NotFound {
                // If it wasn't a not found
                positives.push(video);
            }
        }

        let elapsed_time = start_time.elapsed().as_secs_f64();

        log::info!(
            "Sending {} positive{} for {} in {}s ({}/s)",
            positives.len(),
            if positives.len() == 1 { "" } else { "s" },
            test_request_data.id.iter().collect::<String>(),
            elapsed_time,
            4096.0 / elapsed_time
        );

        payload_send_sender_clone
            .send(bruty_share::Payload {
                op_code: bruty_share::OperationCode::TestingResult,
                data: bruty_share::Data::TestingResult(bruty_share::TestingResultData {
                    id: test_request_data.id,
                    positives,
                }),
            })
            .unwrap();

        // Request the next test
        payload_send_sender_clone
            .send(bruty_share::Payload {
                op_code: bruty_share::OperationCode::TestRequest,
                data: bruty_share::Data::TestRequest,
            })
            .unwrap();
    });
}
