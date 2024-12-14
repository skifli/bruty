/// Generates permutations of the ID.
///
/// # Arguments
/// * `id` - The (base) ID to generate permutations for.
/// * `current_id` - The ID we are on right now.
/// * `id_sender` - The sender to send the current ID to (so it can be passed to client(s)).
/// * `results_awaiting_sender` - The sender to send the current ID to (so we can make sure we get all the results we asked for).
pub fn permutation_generator(
    starting_id: &mut Vec<char>,
    current_id: &Vec<char>,
    id_sender: &flume::Sender<Vec<char>>,
    results_awaiting_sender: &flume::Sender<Vec<char>>,
) {
    if starting_id.len() == 8 {
        while id_sender.len() > 1 {
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

            permutation_generator(&mut new_id, current_id, id_sender, results_awaiting_sender);
        }
    }
}

/// Handles the progress of the results.
///
/// # Arguments
/// * `results_awaiting_receiver` - The receiver for results that are awaiting.
/// * `results_received_receiver` - The receiver for results that have been received.
/// * `persist` - The database connection.
/// * `state` - The server's state.
pub async fn results_progress_handler(
    results_awaiting_receiver: &flume::Receiver<Vec<char>>,
    results_received_receiver: &flume::Receiver<Vec<char>>,
    /* persist: shuttle_persist::PersistInstance, */
    state: &mut bruty_share::types::ServerState,
) {
    let mut awaiting_results = Vec::new();

    loop {
        tokio::select! {
            results_awaiting_result = results_awaiting_receiver.recv_async() => {
                if let Ok(id) = results_awaiting_result {
                    if !awaiting_results.contains(&id) {
                        awaiting_results.push(id); // Add the ID to the list of awaiting results, if it's not already there
                    }
                }
            },
            results_received_result = results_received_receiver.recv_async() => {
                if let Ok(id) = results_received_result {
                    let current_id = awaiting_results.remove(awaiting_results.iter().position(|x| x == &id).unwrap()); // Remove the ID from the list of awaiting results

                    if awaiting_results.iter().all(|testing_id| {
                        for (index, chr) in id.iter().enumerate() {
                            let just_tested_chr_position = bruty_share::VALID_CHARS
                                .iter()
                                .position(|&checking_chr| checking_chr == *chr)
                                .unwrap(); // Get the index of the chr in the just tested ID

                            let testing_chr_position = bruty_share::VALID_CHARS
                                .iter()
                                .position(|&checking_chr| checking_chr == testing_id[index])
                                .unwrap(); // Get the index of the char in the being checked ID

                            if just_tested_chr_position > testing_chr_position {
                                return false;
                            } else if just_tested_chr_position != testing_chr_position {
                                // On this character we are already behind, no need to check the rest
                                return true;
                            }
                        }

                        return true;
                    }){
                        state.current_id = current_id.clone(); // Set the current ID to the ID we just finished checking

                        /* persist.save("server_state", state.clone()).unwrap(); // Save the current ID to the database

                        log::info!(
                            "Finished checking {}",
                            state.current_id.iter().collect::<String>()
                        );  */

                        log::info!("New server state is {:?}", state); // !REMOVE AFTER PERSIST IS RE-ENABLED
                    }
                }
            },
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
