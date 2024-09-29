/// Generates permutations of the ID.
///
/// # Arguments
/// * `id` - The current ID.
/// * `starting_id` - The ID to start from.
/// * `id_sender` - The sender to send the current ID to (so it can be passed to client(s)).
/// * `results_awaiting_sender` - The sender to send the current ID to (so we can make sure we get all the results we asked for).
/// * `current_id_sender` - The sender to send the current ID to when we get to a new ID of length 8.
pub fn permutation_generator(
    id: &mut Vec<char>,
    starting_id: Vec<char>,
    id_sender: &flume::Sender<Vec<char>>,
    results_awaiting_sender: &flume::Sender<Vec<char>>,
    current_id_sender: &flume::Sender<Vec<char>>,
) {
    if id.len() == 9 {
        id_sender.send(id.clone()).unwrap(); // Send the ID to the client
        results_awaiting_sender.send(id.clone()).unwrap(); // Say we've asked for these results
    } else {
        for &chr in bruty_share::VALID_CHARS {
            if starting_id.len() > id.len() {
                // If the character is before where we left off, skip it
                if bruty_share::VALID_CHARS
                    .iter()
                    .position(|&x| x == chr)
                    .unwrap()
                    < bruty_share::VALID_CHARS
                        .iter()
                        .position(|&x| x == starting_id[id.len()])
                        .unwrap()
                {
                    continue; // Skip this character because it's before the current character
                }
            }

            let mut new_id = id.clone();
            new_id.push(chr);

            permutation_generator(
                &mut new_id,
                starting_id.clone(),
                id_sender,
                results_awaiting_sender,
                current_id_sender,
            );
        }

        if id.len() == 8 {
            // Save the current ID
            current_id_sender.send(id.clone()).unwrap();
        }
    }
}

/// Handles the progress of the results.
///
/// # Arguments
/// * `results_awaiting_receiver` - The receiver for results that are awaiting.
/// * `results_received_receiver` - The receiver for results that have been received.
/// * `current_id_receiver` - The receiver for the current ID.
/// * `persist` - The database connection.
/// * `state` - The server's state.
pub async fn results_progress_handler(
    results_awaiting_receiver: &flume::Receiver<Vec<char>>,
    results_received_receiver: &flume::Receiver<Vec<char>>,
    current_id_receiver: &flume::Receiver<Vec<char>>,
    persist: shuttle_persist::PersistInstance,
    state: &mut bruty_share::types::ServerState,
) {
    let mut awaiting_results = Vec::new();
    let mut awaiting_current_id_update = Vec::new();

    loop {
        tokio::select! {
            id = results_awaiting_receiver.recv_async() => {
                if !awaiting_results.contains(&id) {
                    awaiting_results.push(id); // Add the ID to the list of awaiting results, if it's not already there
                    // It may be already there if a client disconnected before sending the results
                }
            }
            id = results_received_receiver.recv_async() => {
                awaiting_results.retain(|x| x != &id); // Remove the ID from the list of awaiting results
            }
            id = current_id_receiver.recv_async() => {
                if let Ok(id) = id {
                    awaiting_current_id_update = id // Set the current ID to the ID we received
                }
            }
        }

        if !awaiting_current_id_update.is_empty() {
            // Only want to update current ID when all awaiting IDs start with current ID.
            // This means that we are not waiting for any results from the previous current ID.

            if awaiting_results.iter().all(|x| {
                x.as_ref()
                    .map_or(false, |v| v.starts_with(&awaiting_current_id_update))
                // Doesn't work with later IDs though. E.g., if the list has 'Aca' and we are trying to update to 'Ab', it won't work.
            }) {
                state.current_id = awaiting_current_id_update.clone(); // Update the current ID
                persist.save("state", &mut *state).unwrap();

                log::info!("Current ID updated to {:?}", state.current_id);
                awaiting_current_id_update.clear(); // Clear the current ID
            }
        }
    }
}

/// Handles the results.
///
/// # Arguments
/// * `results_receiver` - The receiver for the results.
pub async fn results_handler(results_receiver: flume::Receiver<Vec<bruty_share::types::Video>>) {
    loop {
        let results = results_receiver.recv().unwrap();

        for result in results {
            match result.event {
                bruty_share::types::VideoEvent::Success => {
                    let video_data = result.video_data.unwrap();

                    log::info!(
                        "Found https://youtu.be/{} ({}) by {} ({})",
                        result.id.iter().collect::<String>(),
                        video_data.title,
                        video_data.author_name,
                        video_data.author_url
                    );
                }
                bruty_share::types::VideoEvent::NotEmbeddable => {
                    log::info!(
                        "Found https://youtu.be/{}",
                        result.id.iter().collect::<String>()
                    );
                }
            }
        }
    }
}
