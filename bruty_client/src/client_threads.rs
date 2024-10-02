use ahash;
use sonic_rs;

struct IDCheckingStats {
    total_checked: u16,
    positives: Vec<bruty_share::types::Video>,
}

pub async fn results_handler(client_channels: &bruty_share::types::ClientChannels) {
    let mut results_map: ahash::AHashMap<Vec<char>, IDCheckingStats> = ahash::AHashMap::new();
    let mut completed_checks: u64 = 0;
    let start_time = std::time::Instant::now();

    loop {
        let video = client_channels.results_receiver.recv_async().await;

        if video.is_err() {
            // Client has probably disconnected.
            return;
        }

        let video = video.unwrap();

        let base_id: Vec<char> = video.id.iter().take(9).cloned().collect();
        let base_id_clone = base_id.clone();

        let stats = results_map.entry(base_id).or_insert(IDCheckingStats {
            total_checked: 0,
            positives: Vec::new(),
        });

        stats.total_checked += 1;

        if video.event != bruty_share::types::VideoEvent::NotFound {
            stats.positives.push(video);
        }

        if stats.total_checked == 4096 {
            completed_checks += 1;

            let base_id_clone_clone = base_id_clone.clone();

            let positives_len = stats.positives.len();

            log::info!(
                "Tested {} with {} hit{}, client @{}/s",
                base_id_clone.iter().collect::<String>(),
                positives_len,
                if positives_len == 1 { "" } else { "s" },
                completed_checks as f64 * 4069.0 / start_time.elapsed().as_secs_f64(),
            );

            client_channels
                .payload_send_sender
                .send(bruty_share::Payload {
                    op_code: bruty_share::OperationCode::TestingResult,
                    data: bruty_share::Data::TestingResult(bruty_share::TestingResultData {
                        id: base_id_clone,
                        positives: results_map.remove(&base_id_clone_clone).unwrap().positives,
                    }),
                })
                .unwrap();
        }
    }
}

/// Checks the IDs for validity.
///
/// # Arguments
/// * `reqwest_client` - The reqwest client, used for checking the IDs.
/// * `client_channels` - The client's channels, used for communication between threads.
pub async fn id_checker(
    reqwest_client: &reqwest::Client,
    client_channels: &bruty_share::types::ClientChannels,
) {
    let url_base = "https://www.youtube.com/oembed?url=http://www.youtube.com/watch?v=".to_string();

    loop {
        let id = client_channels.id_receiver.recv_async().await;

        if id.is_err() {
            // Client has probably disconnected.
            return;
        }

        let id = id.unwrap();

        let id_vec = id.clone();
        let id_str = id.iter().collect::<String>();

        loop {
            let response = reqwest_client
                .get(format!("{}{}", url_base, id_str))
                .send()
                .await;

            if response.is_err() {
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                continue;
            }

            let response = response.unwrap();

            match response.status().as_u16() {
                200 => {
                    let video_data: bruty_share::types::VideoData =
                        sonic_rs::from_str(&response.text().await.unwrap()).unwrap();

                    client_channels
                        .results_sender
                        .send(bruty_share::types::Video {
                            event: bruty_share::types::VideoEvent::Success,
                            id: id_vec,
                            video_data: Some(video_data),
                        })
                        .unwrap();

                    break;
                }
                401 => {
                    client_channels
                        .results_sender
                        .send(bruty_share::types::Video {
                            event: bruty_share::types::VideoEvent::NotEmbeddable,
                            id: id_vec,
                            video_data: None,
                        })
                        .unwrap();

                    break;
                }
                404 | 400 => {
                    client_channels
                        .results_sender
                        .send(bruty_share::types::Video {
                            event: bruty_share::types::VideoEvent::NotFound,
                            id: id_vec,
                            video_data: None,
                        })
                        .unwrap();

                    break;
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
    }
}

/// Generates the IDs to be checked based on the base ID.
///
/// # Arguments
/// * `base_id` - The base ID, which will be used to generate the IDs to be checked.
/// * `client_channels` - The client's channels, used for communication between threads.
pub fn generate_ids(mut base_id: Vec<char>, client_channels: &bruty_share::types::ClientChannels) {
    if base_id.len() == 10 {
        for &chr in bruty_share::VALID_CHARS {
            base_id.push(chr); // No need to clone here because it was cloned for us by the recursive call

            client_channels.id_sender.send(base_id.clone()).unwrap();

            base_id.pop();
        }
    } else {
        for &chr in bruty_share::VALID_CHARS {
            let mut new_id: Vec<char> = base_id.clone();
            new_id.push(chr);

            generate_ids(new_id, client_channels);
        }
    }
}

/// Gets the base IDs to be checked, and generates the IDs to be checked based on them.
///
/// # Arguments
/// * `client_channels` - The client's channels, used for communication between threads.
pub async fn generate_all_ids(client_channels: &bruty_share::types::ClientChannels) {
    loop {
        let base_id = client_channels.base_id_receiver.recv_async().await;

        if base_id.is_err() {
            // Client has probably disconnected.
            return;
        }

        let base_id = base_id.unwrap();

        generate_ids(base_id.clone(), client_channels);

        log::info!("Generated IDs for {}", base_id.iter().collect::<String>());
    }
}
