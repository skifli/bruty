/// Generates permutations of the ID.
///
/// # Arguments
/// * `id` - The (base) ID to generate permutations for.
/// * `current_id` - The ID we are on right now.
/// * `server_data` - The server's data.
pub async fn permutation_generator(
    starting_id: &Vec<char>,
    current_id: &Vec<char>,
    server_data: &bruty_share::types::ServerData,
) {
    while server_data
        .users_connected_num
        .load(std::sync::atomic::Ordering::SeqCst)
        == 0
    {
        // Sleep for users to connect
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    if current_id.len() == 8 {
        let mut current_id_shared = server_data.current_id.lock().await; // Lock the current ID

        while !current_id_shared.is_empty() {
            drop(current_id_shared); // Drop the lock

            std::thread::sleep(std::time::Duration::from_secs(1)); // Sleep for the current ID to be consumed

            current_id_shared = server_data.current_id.lock().await; // Lock the current ID
        }

        server_data
            .event_sender
            .send(bruty_share::types::ServerEvent::ResultsAwaiting(
                current_id.clone(),
            ))
            .unwrap(); // Send the ID so the results handler knows we are awaiting it

        *current_id_shared = current_id.clone(); // Set the current ID to the starting ID
        drop(current_id_shared); // Drop the lock
    } else {
        for &chr in bruty_share::VALID_CHARS {
            if starting_id.len() > current_id.len() {
                // Check if starting_id starts with current_id
                // If not, we've moved on to a new ID
                if starting_id.starts_with(current_id) {
                    // If the character is before where we left off, skip it
                    if bruty_share::VALID_CHARS
                        .iter()
                        .position(|&x| x == chr)
                        .unwrap()
                        < bruty_share::VALID_CHARS
                            .iter()
                            .position(|&x| x == starting_id[current_id.len()])
                            .unwrap()
                    {
                        continue; // Skip this character because it's before the current character
                    }
                }
            }

            let mut new_id = current_id.clone();
            new_id.push(chr);

            Box::pin(permutation_generator(starting_id, &mut new_id, server_data)).await;
        }
    }
}

/// Handles the progress of the results.
///
/// # Arguments
/// * `state` - The server's state.
/// * `server_data` - The server's data.
pub async fn results_progress_handler(
    state: &mut bruty_share::types::ServerState,
    server_data: &bruty_share::types::ServerData,
) {
    let mut awaiting_results = Vec::new();

    while let Ok(event) = server_data.event_receiver.recv_async().await {
        match event {
            bruty_share::types::ServerEvent::ResultsAwaiting(id) => {
                if !awaiting_results.contains(&id) {
                    awaiting_results.push(id); // Add the ID to the list of awaiting results, if it's not already there
                }
            }
            bruty_share::types::ServerEvent::ResultsReceived(id) => {
                let current_id = awaiting_results
                    .remove(awaiting_results.iter().position(|x| x == &id).unwrap()); // Remove the ID from the list of awaiting results

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
                }) {
                    state
                        .operator
                        .write_serialized(
                            "current_id",
                            &bruty_share::types::ServerStateInner {
                                inner: current_id.clone(),
                            },
                        )
                        .await
                        .unwrap(); // Set the current ID to the ID we just finished checking

                    log::info!(
                        "Finished checking {}",
                        current_id.iter().collect::<String>()
                    );
                }
            }
            _ => {}
        }
    }
}

/// Handles the results.
///
/// # Arguments
/// * `server_data` - The server's data.
pub async fn results_handler(server_data: &bruty_share::types::ServerData) {
    while let Ok(event) = server_data.event_receiver.recv_async().await {
        match event {
            bruty_share::types::ServerEvent::PositiveResultsReceived(results) => {
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
            _ => continue,
        };
    }
}
