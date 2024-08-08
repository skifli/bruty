use log;
use simplelog;
use std;
use tokio;

const VALID_CHARS: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L',
    'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '0', '1', '2', '3', '4',
    '5', '6', '7', '8', '9', '-', '_',
];

fn generate_permutations(id: &mut Vec<char>, tx_discovery: &flume::Sender<String>) {
    if id.len() == 10 {
        for &chr in VALID_CHARS {
            id.push(chr); // No need to clone here because it was cloned for us by the recursive call

            tx_discovery.send(id.iter().collect()).unwrap();

            id.pop();
        }
    } else {
        for &chr in VALID_CHARS {
            let mut new_id = id.clone();
            new_id.push(chr);

            generate_permutations(&mut new_id, tx_discovery);
        }
    }
}

async fn try_link(id: &str, tx_testing: &flume::Sender<String>, client: &reqwest::Client) {
    loop {
        let resp = client
            .get(
                "https://www.youtube.com/oembed?url=http://www.youtube.com/watch?v=".to_string()
                    + id,
            )
            .send()
            .await;

        match resp.unwrap().status().as_u16() {
            200 => {
                tx_testing.send(id.to_string()).unwrap(); // Found valid ID
                return;
            }
            429 => {
                tx_testing.send("rate".to_string()).unwrap(); // Rate limited
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await; // Rate limited, wait 1 second
            }
            _ => {
                tx_testing.send("".to_string()).unwrap(); // Invalid ID (usually 400 for not existing)
                return;
            }
        }
    }
}

#[tokio::main]
async fn main() {
    simplelog::CombinedLogger::init(vec![
        simplelog::TermLogger::new(
            simplelog::LevelFilter::Info,
            simplelog::Config::default(),
            simplelog::TerminalMode::Mixed,
            simplelog::ColorChoice::Auto,
        ),
        simplelog::WriteLogger::new(
            simplelog::LevelFilter::Info,
            simplelog::Config::default(),
            std::fs::File::create("bruty.log").unwrap(),
        ),
    ])
    .unwrap();

    let args: Vec<String> = std::env::args().collect();

    let starting_id = if args.len() >= 2 {
        args[1].clone()
    } else {
        "3qw99S3".to_string()
    };

    let testing_threads = if args.len() >= 3 {
        args[2].parse::<usize>().unwrap()
    } else {
        100
    };

    let discovery_bound = if args.len() >= 4 {
        args[3].parse::<usize>().unwrap()
    } else {
        10000000
    };

    let (tx_discovery, rx_discovery) = flume::bounded(discovery_bound);

    log::info!(
        "Starting with {}, using {} threads with a bound of {}.",
        starting_id,
        testing_threads,
        discovery_bound
    );

    tokio::spawn(async move {
        generate_permutations(&mut starting_id.chars().collect(), &tx_discovery);
        drop(tx_discovery) // Signal that all permutations have been generated
    });

    let (tx_testing, rx_testing) = flume::unbounded();

    for _ in 0..testing_threads {
        let rx_discovery_clone = rx_discovery.clone(); // Clone the receiver of permutations for each worker
        let tx_testing_clone = tx_testing.clone(); // Clone the sender of test results for each worker

        tokio::spawn(async move {
            let client = reqwest::Client::new();

            loop {
                match rx_discovery_clone.recv_async().await {
                    Ok(id) => {
                        try_link(&id, &tx_testing_clone, &client).await;
                    }
                    Err(_) => {
                        break; // Assume all permutations have been assigned a worker
                    }
                }
            }

            // Not going to drop the sender here because we need to wait for the results to be processed
        });
    }

    let mut total_count = 0;
    let mut total_ratelimit_count = 0;
    let mut total_valid_count = 0;

    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // This block executes every 10 seconds
                if rx_discovery.is_empty() && rx_discovery.is_disconnected() {  // All workers are done, so we should have checked all permutations
                    if !rx_testing.is_empty() {
                        continue; // There are still results to process, so wait for them
                    }

                    break; // Exit the loop when all permutations have been tested
                }

                if total_count > 0 { // At the start the values aren't matched - some permutations may have been testing but we haven't been able to count them yet. So just ignore the first interval
                    log::info!(
                        "Tested: {}, Valid: {}, To Test: {}, Rate Limited: {}.",
                        total_count,
                        total_valid_count,
                        rx_discovery.len(),
                        total_ratelimit_count
                    );
                }
            }
            Ok(id) = rx_testing.recv_async() => {
                total_count += 1; // Got a result

                if id == "rate" {
                    total_ratelimit_count += 1;
                } else if id != "" {
                    // Yoo we got one!
                    total_valid_count += 1;
                    log::info!("VALID ID: {}.", id);
                }
            }
        }
    }

    // Okay so what I had before was a list of tasks that was all the threads spawned above in that for loop for
    // testing the links. However tha code would hang, which I might know why but a fix is hard / this is easier.
    // My theory is on Line 120 (as of whatever the heck this commit is), when awaiting a message, the channel is
    // dropped. However since the code is already waiting, for some reason it never gets the message, because it
    // isn't a message. It's awaiting messages, but the channel has been dropped, but it doesn't know that. It's an
    // implementation problem, but this is just my guess. It might be something completely different causing this
    // bug. Either way, I have made the code not wait for those threads to finish here, because it to be honest is
    // better as a hang prevention method. By the time the code gets here every generated link HAS to have been
    // checked already as on Line 146 in the above for loop the ONLY way to break is by the channel of permutations
    // to test being empty, and as a further safeguard all senders of permutations have been dropped (in case we
    // access the channel at a weird race point inbetween permutations being added and being checked). tx_discovery
    // is dropped on Line 107 only after the permutations generator function has finished, so we know that it HAS
    // to have finished generating them all. That way, now we can break knowing everything has been checked. :`).

    // And it worked! Woo-hoo.

    log::info!(
        "Finished after testing {} permutations, with {} rate limit(s).",
        total_count,
        total_ratelimit_count
    );
}
