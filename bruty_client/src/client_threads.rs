use sonic_rs;

pub async fn id_checker(
    id_receiver: &flume::Receiver<Vec<char>>,
    reqwest_client: &reqwest::Client,
    positives_sender: &flume::Sender<bruty_share::types::Video>,
) {
    let url_base = "https://www.youtube.com/oembed?url=http://www.youtube.com/watch?v=".to_string();

    loop {
        let id = id_receiver.recv_async().await;

        if id.is_err() {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            continue;
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

                    positives_sender
                        .send(bruty_share::types::Video {
                            event: bruty_share::types::VideoEvent::Success,
                            id: id_vec,
                            video_data: Some(video_data),
                        })
                        .unwrap();

                    break;
                }
                401 => {
                    positives_sender
                        .send(bruty_share::types::Video {
                            event: bruty_share::types::VideoEvent::NotEmbeddable,
                            id: id_vec,
                            video_data: None,
                        })
                        .unwrap();

                    break;
                }
                404 | 400 => {
                    positives_sender
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

/// Checks the video IDs.
///
/// # Arguments
/// * `id_sender` - The sender for sending video IDs.
/// * `id` - The ID to check.
pub async fn generate_ids(id_sender: &flume::Sender<Vec<char>>, mut id: Vec<char>) {
    if id.len() == 10 {
        for &chr in bruty_share::VALID_CHARS {
            id.push(chr); // No need to clone here because it was cloned for us by the recursive call

            id_sender.send(id.clone()).unwrap();

            id.pop();
        }
    } else {
        for &chr in bruty_share::VALID_CHARS {
            let mut new_id = id.clone();
            new_id.push(chr);

            Box::pin(generate_ids(id_sender, new_id)).await;
        }
    }
}
