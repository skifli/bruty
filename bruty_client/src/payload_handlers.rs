use futures::StreamExt;
use futures_util::SinkExt;

use crate::WebSocketSender;

pub struct IdGenerator {
    current_id: Vec<char>,
    tenth_index: usize,
    eleventh_index: usize,
}

impl IdGenerator {
    pub fn new(start_id: Vec<char>) -> Self {
        Self {
            current_id: start_id,
            tenth_index: 0,
            eleventh_index: 0,
        }
    }
}

impl Iterator for IdGenerator {
    type Item = Vec<char>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_id.len() == 9 {
            self.current_id.extend(vec!['a', 'a']);

            return Some(self.current_id.clone());
        } else if self.current_id.len() == 11 {
            if self.eleventh_index + 1 < bruty_share::VALID_CHARS.len() {
                self.eleventh_index += 1;
                self.current_id.pop();
                self.current_id
                    .push(bruty_share::VALID_CHARS[self.eleventh_index]);

                return Some(self.current_id.clone());
            } else {
                self.current_id.pop();
            }
        }

        if self.tenth_index + 1 < bruty_share::VALID_CHARS.len() {
            self.tenth_index += 1;

            self.current_id.pop();
            self.current_id
                .extend(vec![bruty_share::VALID_CHARS[self.tenth_index], 'a']);

            self.eleventh_index = 0;

            return Some(self.current_id.clone());
        } else {
            None
        }
    }
}

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
    reqwest_client: &reqwest::Client,
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

    let payload_send_sender_clone = payload_send_sender.clone();
    let reqwest_client_clone = reqwest_client.clone();

    let url_base = "https://www.youtube.com/oembed?url=http://www.youtube.com/watch?v=";

    tokio::spawn(async move {
        let start_time = tokio::time::Instant::now();

        let id_generator = IdGenerator::new(test_request_data.id.clone());

        let futures = futures::stream::FuturesUnordered::new();

        for id in id_generator {
            let client = &reqwest_client_clone;
    
            futures.push(async move {
                let id_vec = id.clone();
                let id_str = id.iter().collect::<String>();

                loop {
                    let response = client
                        .get(
                            format!(
                                "{}{}",
                                url_base,
                                id_str
                            )
                        )
                        .send()
                        .await;
    
                    if response.is_err() {
                        log::warn!(
                            "Error occurred while requesting {} check ({}). Retrying in 1 second...",
                            id_str,
                            response.as_ref().unwrap_err()
                        );
                        continue;
                    }
    
                    let response = response.unwrap();
    
                    match response.status().as_u16() {
                        200 => {
                            let video_data: bruty_share::types::VideoData =
                                sonic_rs::from_str(&response.text().await.unwrap()).unwrap();
    
                            return Some(bruty_share::types::Video {
                                event: bruty_share::types::VideoEvent::Success,
                                id: id_vec,
                                video_data: Some(video_data),
                            });
                        }
                        401 => {
                            return Some(bruty_share::types::Video {
                                event: bruty_share::types::VideoEvent::NotEmbeddable,
                                id: id_vec,
                                video_data: None,
                            });
                        }
                        404 | 400 => {
                            return None;
                        }
                        _ => {
                            log::warn!(
                                "Error occurred while checking ID: {} ({}). Retrying...",
                                id_str,
                                response.status().as_u16()
                            );
                        }
                    }
                }
            });
        }
    
        let positives: Vec<_> = futures.filter_map(|result| async {
            if let Some(video) = result {
                match video.event {
                    bruty_share::types::VideoEvent::Success | bruty_share::types::VideoEvent::NotEmbeddable => Some(video),
                }
            } else {
                None
            }
        }).collect().await;        
    

        let elapsed_time = start_time.elapsed().as_secs_f64();

        log::info!(
            "Sending results for {} in {}s ({}/s)",
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
