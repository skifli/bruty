use sonic_rs;

pub async fn id_checker(
    id_receiver: &flume::Receiver<Vec<char>>,
    reqwest_client: &reqwest::Client,
    positives_sender: &flume::Sender<bruty_share::types::Video>,
) {
    loop {
        let id = id_receiver.recv_async().await.unwrap(); // Await an ID to check

        let response = reqwest_client
            .get(
                "https://www.youtube.com/oembed?url=http://www.youtube.com/watch?v=".to_string()
                    + &id.iter().collect::<String>(),
            )
            .send()
            .await
            .unwrap();

        match response.status().as_u16() {
            200 => {
                let video_data: bruty_share::types::VideoData =
                    sonic_rs::from_str(&response.text().await.unwrap()).unwrap();

                positives_sender
                    .send(bruty_share::types::Video {
                        event: bruty_share::types::VideoEvent::Success,
                        id: id.clone(),
                        video_data: Some(video_data),
                    })
                    .unwrap();
            }
            401 => {
                positives_sender
                    .send(bruty_share::types::Video {
                        event: bruty_share::types::VideoEvent::NotEmbeddable,
                        id: id.clone(),
                        video_data: None,
                    })
                    .unwrap();
            }
            _ => {}
        };
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
