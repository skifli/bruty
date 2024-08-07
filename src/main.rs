use std::env;

const VALID_CHARS: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L',
    'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '0', '1', '2', '3', '4',
    '5', '6', '7', '8', '9', '-', '_',
];

fn generate_permutations(id: Vec<char>, tx_discovery: &flume::Sender<String>) {
    if id.len() == 10 {
        let mut new_id = id.clone();

        for &chr in VALID_CHARS {
            new_id.push(chr);

            tx_discovery.send(new_id.iter().collect()).unwrap();

            new_id.pop();
        }
    } else {
        for &chr in VALID_CHARS {
            let mut new_id = id.clone();
            new_id.push(chr);

            generate_permutations(new_id, tx_discovery);
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
    let (tx_discovery, rx_discovery) = flume::unbounded();

    let args: Vec<String> = env::args().collect();

    let starting_id = if args.len() == 2 {
        args[1].clone()
    } else {
        "3qw99S".to_string()
    };

    println!("[0s] Starting with ID: {}", starting_id);

    tokio::spawn(async move {
        generate_permutations(starting_id.chars().collect(), &tx_discovery);
        drop(tx_discovery) // Signal that all permutations have been generated
    });

    let (tx_testing, rx_testing) = flume::unbounded();

    let mut tasks = vec![];

    for _ in 0..5 {
        let rx_discovery_clone = rx_discovery.clone(); // Clone the receiver of permutations for each worker
        let tx_testing_clone = tx_testing.clone(); // Clone the sender of test results for each worker

        tasks.push(tokio::spawn(async move {
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

            drop(tx_testing_clone); // Signal that this worker is done
        }));
    }

    drop(tx_testing); // Signal that all permutations have been assigned a worker, and that no more will be sent

    let start = std::time::Instant::now();
    let mut last_update = start;
    let mut total_count = 0;

    loop {
        match rx_testing.recv_async().await {
            Ok(id) => {
                total_count += 1; // Got a result
                let elapsed = start.elapsed().as_secs();

                if id != "" {
                    // Yoo we got one!
                    println!("[{}s] VALID ID: {}", elapsed, id);
                }

                let elapsed_since_last_update = last_update.elapsed().as_secs();

                if elapsed_since_last_update > 0 && elapsed_since_last_update % 5 == 0 {
                    // So we know something is happening lol
                    println!("[{}s] Total count: {}", elapsed, total_count);

                    last_update = std::time::Instant::now();
                }
            }
            Err(_) => {
                break; // Assume all permutations have been tested
            }
        }
    }

    println!(
        "[{}s] FIN count: {}",
        start.elapsed().as_secs(),
        total_count
    );

    for task in tasks {
        task.await.unwrap(); // Wait for all workers to finish
    }
}
