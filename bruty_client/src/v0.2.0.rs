use chrono;
use clap::Parser;
use fern;
use reqwest;
use sonic_rs;
use std;
use std::io::Seek;
use std::io::Write;
use tokio;

const AUTHOR: &str = env!("CARGO_PKG_AUTHORS");
const VERSION: &str = env!("CARGO_PKG_VERSION");

const VALID_CHARS: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L',
    'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '0', '1', '2', '3', '4',
    '5', '6', '7', '8', '9', '-', '_',
];

#[derive(Parser, Debug)]
#[command(
    author = AUTHOR,
    version = VERSION,
    about = "Brute-forces the rest of a YouTube video ID when you have part of it"
)]
struct Args {
    /// YouTube ID to start brute-forcing from
    id: String,

    #[arg(
        short = 't',
        long = "threads",
        help = "Number of threads to use",
        default_value_t = 100
    )]
    threads: u8,

    #[arg(
        short = 'b',
        long = "bound",
        help = "Bound for permutations channel before blocking more generation",
        default_value_t = 100000000
    )]
    bound: usize,

    #[arg(
        short = 's',
        long = "save",
        help = "File to save the current ID to (will be overwritten)",
        default_value = "bruty_save.txt"
    )]
    save: String,

    #[arg(
        short = 'l',
        long = "log",
        help = "Log file to write to (will be overwritten)",
        default_value = "bruty.log"
    )]
    log: String,

    #[arg(
        short = 'i',
        long = "log-interval",
        help = "How long to wait between info logs (in seconds)",
        default_value_t = 10
    )]
    log_interval: u64,

    #[arg(
        short = 'r',
        long = "start-from-saved",
        help = "Start from the saved ID instead of the provided one"
    )]
    start_from_saved: bool,
}

// Represents a YT video's data
#[derive(sonic_rs::Deserialize)]
struct VideoData {
    title: String,
    author_name: String,
    author_url: String,
}

#[derive(PartialEq)]
// Represents an event that can occur during the testing process
enum MessageEvent {
    Success,       // Found in the embed API
    NotEmbeddable, // Not found in the embed API, but still exists
    NotFound,      // Not found in any API
    RateLimited,   // Rate limited by the API
}

// Represents a message sent from a tester to the main thread
struct Message {
    event: MessageEvent,
    id: Box<str>,                  // ID that was tested
    video_data: Option<VideoData>, // Only present if Event is Success
}

fn setup_logger(log_file: String) -> Result<(), fern::InitError> {
    // configure fern::colors::Colors for the whole line
    let level_colors = fern::colors::ColoredLevelConfig::new()
        .error(fern::colors::Color::Red)
        .warn(fern::colors::Color::Yellow)
        .info(fern::colors::Color::Blue)
        .debug(fern::colors::Color::Magenta)
        .trace(fern::colors::Color::White);

    // Create a dispatch for stdout with coloured output
    let stdout_dispatch = fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{} \x1B[{}m{:<5}\x1B[0m {}", // \x1B[0m resets the color
                chrono::DateTime::<chrono::Local>::from(std::time::SystemTime::now())
                    .format("%H:%M:%S"), // Format time nicely
                level_colors.get_color(&record.level()).to_fg_str(), // Set color based on log level
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(std::io::stdout());

    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(log_file.clone())
        .unwrap(); // Truncate the file first

    // Create a dispatch for the log file without coloured output
    let file_dispatch = fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{} {:<5} {}", // No color formatting here
                chrono::DateTime::<chrono::Local>::from(std::time::SystemTime::now())
                    .format("%H:%M:%S"), // Format time nicely
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Trace)
        .chain(
            std::fs::OpenOptions::new()
                .append(true)
                .open(log_file)
                .unwrap(),
        );

    // Combine both dispatchers
    fern::Dispatch::new()
        .chain(stdout_dispatch)
        .chain(file_dispatch)
        .level_for("reqwest", log::LevelFilter::Warn)
        .apply()?;

    Ok(())
}

fn generate_permutations(
    id: &mut Vec<char>,
    generator_sender: &flume::Sender<Box<str>>,
    split_id: Vec<char>,
) {
    if id.len() == 10 {
        for &chr in VALID_CHARS {
            if split_id.len() > id.len() {
                if VALID_CHARS.iter().position(|&x| x == chr).unwrap()
                    < VALID_CHARS
                        .iter()
                        .position(|&x| x == split_id[id.len()])
                        .unwrap()
                {
                    continue; // Skip this character because it's before the split
                }
            }

            id.push(chr); // No need to clone here because it was cloned for us by the recursive call

            generator_sender
                .send(id.iter().collect::<String>().into_boxed_str())
                .unwrap();

            id.pop();
        }
    } else {
        for &chr in VALID_CHARS {
            if split_id.len() > id.len() {
                // if chr position in VALID_CHARS is less than the split_id position
                if VALID_CHARS.iter().position(|&x| x == chr).unwrap()
                    < VALID_CHARS
                        .iter()
                        .position(|&x| x == split_id[id.len()])
                        .unwrap()
                {
                    continue; // Skip this character because it's before the split
                }
            }

            let mut new_id = id.clone();
            new_id.push(chr);

            while generator_sender.is_full() {} /* Otherwise the program gets terminated on larger IDs */

            generate_permutations(&mut new_id, generator_sender, split_id.clone());
        }
    }
}

async fn try_link(client: &reqwest::Client, id: &str, testing_sender: &flume::Sender<Message>) {
    loop {
        let response = client
            .get(
                "https://www.youtube.com/oembed?url=http://www.youtube.com/watch?v=".to_string()
                    + id,
            )
            .send()
            .await;

        let response = response.unwrap();

        match response.status().as_u16() {
            200 => {
                // Found valid ID
                testing_sender
                    .send(Message {
                        event: MessageEvent::Success,
                        id: id.into(),
                        video_data: Some(
                            sonic_rs::from_str(&response.text().await.unwrap()).unwrap(),
                        ),
                    })
                    .unwrap();
            }
            401 => {
                // Not embeddable
                testing_sender
                    .send(Message {
                        event: MessageEvent::NotEmbeddable,
                        id: id.into(),
                        video_data: None,
                    })
                    .unwrap();
            }
            429 => {
                // Rate limited
                testing_sender
                    .send(Message {
                        event: MessageEvent::RateLimited,
                        id: id.into(),
                        video_data: None,
                    })
                    .unwrap();

                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                continue;
            }
            _ => {
                // Invalid ID (usually 400 for not existing)
                testing_sender
                    .send(Message {
                        event: MessageEvent::NotFound,
                        id: id.into(),
                        video_data: None,
                    })
                    .unwrap();
            }
        }

        break; // If we are here we got an acceptable response
    }
}

fn get_statistics(
    total_checked_count: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
    total_ratelimited_count: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
    elapsed: u64,
) -> (usize, usize, usize) {
    let checked_count = total_checked_count.load(std::sync::atomic::Ordering::Relaxed);
    let ratelimited_count = total_ratelimited_count.load(std::sync::atomic::Ordering::Relaxed);

    let average_checked_count = if elapsed == 0 {
        checked_count
    } else {
        (checked_count as f64 / elapsed as f64).round() as usize
    };

    (checked_count, ratelimited_count, average_checked_count)
}

fn terminal_link(url: &str, text: &str) -> String {
    format!("\x1B]8;;{}\x1B\\{}\x1B]8;;\x1B\\", url, text)
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let args_save = args.save.clone();

    setup_logger(args.log).unwrap();

    if args.threads == 1 {
        // Seriously, what? Why?
        log::warn!("Running with a single thread is not recommended. Consider increasing the thread count.");
    }

    log::debug!("Bruty v{} by {}", VERSION, AUTHOR);

    log::info!(
        "ID: {}; Threads: {}; Bound: {}",
        args.id,
        args.threads,
        args.bound
    );

    let (generator_sender, generator_receiver) = flume::bounded(args.bound);

    let mut split_id = "".to_string();

    if args.start_from_saved {
        split_id = std::fs::read_to_string(args_save.clone()).expect("Failed to read save file");
    }

    if !args.start_from_saved && !split_id.is_empty() {
        log::error!("Save file is not empty, but --start-from-saved was not provided. Exiting.");

        std::process::exit(1);
    }

    let permutation_generator = tokio::spawn(async move {
        generate_permutations(
            &mut args.id.chars().collect(),
            &generator_sender,
            split_id.chars().collect(),
        );
        drop(generator_sender); // Signal that all permutations have been generated
        log::trace!("All permutations generated");
    });

    let (testing_sender, testing_receiver) = flume::unbounded();
    let mut testing_tasks = vec![]; // Stores the tasks which are testing permutations

    let client = reqwest::Client::new();

    for _ in 0..args.threads {
        let client_clone = client.clone(); // Clone for each worker as it's moved into the worker thread
        let generator_receiver_clone = generator_receiver.clone(); // Same reason as above
        let testing_sender_clone = testing_sender.clone(); // Same reason as above above ;`)

        testing_tasks.push(tokio::spawn(async move {
            loop {
                match generator_receiver_clone.try_recv() {
                    Ok(id) => {
                        try_link(&client_clone, &id, &testing_sender_clone).await;
                    }
                    Err(_) => {
                        if generator_receiver_clone.is_disconnected() {
                            // No more permutations are being generated
                            drop(testing_sender_clone); // Signal that this worker is done

                            log::trace!("No more permutations to test, ending worker");
                            return;
                        }

                        // Otherwise, wait for more permutations to test
                        continue;
                    }
                }
            }
        }));
    }

    drop(testing_sender); // Signal that all workers have been spawned

    let testing_receiver_clone = testing_receiver.clone(); // Clone for the task_counter

    let total_checked_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let total_checked_count_writer = total_checked_count.clone();

    let total_ratelimited_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let total_ratelimited_count_writer = total_ratelimited_count.clone();

    let results_handler = tokio::spawn(async move {
        let mut save_file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(args.save)
            .unwrap();

        let mut timer_interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
        let mut last_id = Box::from("");

        loop {
            tokio::select! {
                message = testing_receiver_clone.recv_async() => {
                    match message {
                        Ok(message) => {
                            if message.event == MessageEvent::RateLimited {
                                // If we were ratelimited we are going to try again, so don't count it (yet)
                                total_ratelimited_count_writer
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                                log::trace!("Rate limited on {}", message.id);

                                continue; // Don't output this message
                            } else {
                                total_checked_count_writer
                                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            }

                            if message.event == MessageEvent::Success {
                                let video_data = message.video_data.unwrap();

                                log::info!(
                                    "Found {} by {}",
                                    terminal_link(
                                        &format!("https://youtu.be/{}", &message.id),
                                        &video_data.title
                                    ),
                                    terminal_link(&video_data.author_url, &video_data.author_name)
                                );
                            } else if message.event == MessageEvent::NotEmbeddable {
                                log::info!(
                                    "Found {}",
                                    terminal_link(
                                        &format!("https://youtu.be/{}", &message.id),
                                        &message.id
                                    )
                                );
                            }

                            last_id = message.id;
                        }
                        Err(_) => {
                            if testing_receiver_clone.is_disconnected() {
                                // No more permutations are being tested

                                log::trace!("All permutations tested");
                                return; // So there's nothing more for this worker to do
                            }

                            // Otherwise, wait for more messages to be received
                            continue;
                        }
                    }
                }
                _ = timer_interval.tick() => {
                    save_file.set_len(0).unwrap(); // Clear the file
                    save_file.seek(std::io::SeekFrom::Start(0)).unwrap(); // Reset the cursor
                    save_file.write(last_id.as_bytes()).unwrap(); /* We would have received the first ID in the channel, which is the earliest in
                                                                    the sequence (since the generate_permutations function goes in order, luckily).
                                                                    This means that is the earliest ID that we have not checked yet. */
                    save_file.flush().unwrap(); // Write the changes to the file
                }
            }
        }
    });

    let interval = tokio::time::Duration::from_secs(args.log_interval as u64);
    let start = tokio::time::Instant::now();
    let mut last_tick = start.clone();

    while !testing_receiver.is_disconnected() {
        // If the sender of the testing channel is dropped, all workers are done
        if last_tick.elapsed() >= interval {
            let elapsed = start.elapsed().as_secs();

            let (checked_count, ratelimited_count, average_checked_count) =
                get_statistics(&total_checked_count, &total_ratelimited_count, elapsed);

            log::debug!(
                "{}/{} checked @{}/s{}",
                checked_count,
                generator_receiver.len(),
                average_checked_count,
                if ratelimited_count > 0 {
                    format!(" with {} ratelimit(s)", ratelimited_count)
                } else {
                    "".to_string()
                }
            );

            last_tick = tokio::time::Instant::now();
        }
    }

    for _ in 0..500 {
        if permutation_generator.is_finished()
            && testing_tasks.iter().all(|task| task.is_finished())
            && results_handler.is_finished()
        {
            break;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    if !permutation_generator.is_finished() {
        log::error!("assumed end but permutation generator is still running after 5 seconds");
    }

    if !testing_tasks.iter().all(|task| task.is_finished()) {
        log::error!("assumed end but testing tasks are still running after 5 seconds");
    }

    if !results_handler.is_finished() {
        log::error!("assumed end but results handler is still running after 5 seconds");
    }

    let elapsed = start.elapsed().as_secs();

    let (checked_count, ratelimited_count, average_checked_count) =
        get_statistics(&total_checked_count, &total_ratelimited_count, elapsed);

    log::info!(
        "Checked {} ID{} in {} second{} @{}/s{}",
        checked_count,
        if checked_count == 1 { "" } else { "s" },
        elapsed,
        if elapsed == 1 { "" } else { "s" },
        average_checked_count,
        if ratelimited_count > 0 {
            format!(" with {} ratelimit(s)", ratelimited_count)
        } else {
            "".to_string()
        }
    );

    std::fs::remove_file(args_save).unwrap(); // Remove the save file
}
