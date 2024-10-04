/// Generates permutations of the ID.
///
/// # Arguments
/// * `id` - The (base) ID to generate permutations for.
/// * `current_id` - The ID we are on right now.
/// * `id_sender` - The sender to send the current ID to (so it can be passed to client(s)).
/// * `results_awaiting_sender` - The sender to send the current ID to (so we can make sure we get all the results we asked for).
/// * `current_id_sender` - The sender to send the current ID to when we get to a new ID of length 8 (so it is saved).
pub fn permutation_generator(
    starting_id: &mut Vec<char>,
    current_id: &Vec<char>,
    id_sender: &flume::Sender<Vec<char>>,
    results_awaiting_sender: &flume::Sender<Vec<char>>,
    current_id_sender: &flume::Sender<Vec<char>>,
) {
    if starting_id.len() == 9 {
        while id_sender.len() > 2 {
            // Wait for IDs
        }

        id_sender.send(starting_id.clone()).unwrap(); // Send the ID to the client
        results_awaiting_sender.send(starting_id.clone()).unwrap(); // Say we've asked for these results
    } else {
        for &chr in bruty_share::VALID_CHARS {
            if current_id.len() > starting_id.len() {
                // Check if current_id starts with starting_id
                // If not, we've moved on to a new ID
                if current_id.starts_with(starting_id) {
                    // If the character is before where we left off, skip it
                    if bruty_share::VALID_CHARS
                        .iter()
                        .position(|&x| x == chr)
                        .unwrap()
                        < bruty_share::VALID_CHARS
                            .iter()
                            .position(|&x| x == current_id[starting_id.len()])
                            .unwrap()
                    {
                        continue; // Skip this character because it's before the current character
                    }
                }
            }

            let mut new_id = starting_id.clone();
            new_id.push(chr);

            if new_id.len() == 8 {
                // Save the current ID
                current_id_sender.send(new_id.clone()).unwrap();
            }

            permutation_generator(
                &mut new_id,
                current_id,
                id_sender,
                results_awaiting_sender,
                current_id_sender,
            );
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

    let mut checked_ids = 0;
    let start_time = std::time::Instant::now();

    loop {
        let current_id_receiver_try = current_id_receiver.try_recv();

        if let Ok(id) = current_id_receiver_try {
            awaiting_current_id_update.push(id.clone());
        }

        let results_awaiting_receiver_try = results_awaiting_receiver.try_recv();

        if let Ok(id) = results_awaiting_receiver_try {
            if !awaiting_results.contains(&id) {
                awaiting_results.push(id); // Add the ID to the list of awaiting results, if it's not already there
                                           // It may be already there if a client disconnected before sending the results
            }
        }

        let results_received_receiver_try = results_received_receiver.try_recv();

        if let Ok(id) = results_received_receiver_try {
            awaiting_results.retain(|x| x != &id); // Remove the ID from the list of awaiting results
        }

        if awaiting_current_id_update.len() > 0 {
            for id in awaiting_current_id_update.clone() {
                // Only want to update current ID when all awaiting IDs start with current ID.
                // This means that we are not waiting for any results from the previous current ID.

                if awaiting_results.iter().all(|testing_id| {
                    for (index, chr) in id.iter().enumerate() {
                        let awaiting_char_position = bruty_share::VALID_CHARS
                            .iter()
                            .position(|&checking_chr| checking_chr == *chr)
                            .unwrap();

                        let testing_char_position = bruty_share::VALID_CHARS
                            .iter()
                            .position(|&checking_chr| checking_chr == testing_id[index])
                            .unwrap();

                        if awaiting_char_position > testing_char_position {
                            return false;
                        } else if awaiting_char_position != testing_char_position {
                            return true;
                        }
                    }

                    return true;
                }) {
                    state.current_id = awaiting_current_id_update.remove(0);

                    persist.save("server_state", state.clone()).unwrap(); // Save the current ID to the database

                    checked_ids += 1;

                    log::info!(
                        "Finished checking {} @{}/s",
                        state.current_id.iter().collect::<String>(),
                        (checked_ids as f64 * 262144.0) / start_time.elapsed().as_secs_f64()
                    );
                }
            }
        }
    }
}

/// Handles the results.
///
/// # Arguments
/// * `results_receiver` - The receiver for the results (of checking videos).
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
                _ => {}
            }
        }
    }
}
