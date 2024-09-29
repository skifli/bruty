/// Sets up the console logger.
///
/// # Returns
/// * `Result<(), fern::InitError>` - The result of setting up the logger.
fn create_console_logger() -> fern::Dispatch {
    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{} \x1B[{}m{:<5}\x1B[0m {}", // \x1B[0m resets the color
                chrono::DateTime::<chrono::Local>::from(std::time::SystemTime::now())
                    .format("%H:%M:%S"), // Format time nicely
                fern::colors::ColoredLevelConfig::new()
                    .error(fern::colors::Color::Red)
                    .warn(fern::colors::Color::Yellow)
                    .info(fern::colors::Color::Blue)
                    .debug(fern::colors::Color::Magenta)
                    .trace(fern::colors::Color::White)
                    .get_color(&record.level())
                    .to_fg_str(), // Set colour based on log level
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Info)
        .chain(std::io::stdout())
}

/// Sets up the file logger.
///
/// # Arguments
/// * `log_file` - The file to log to.
///
/// # Returns
/// * `fern::Dispatch` - The file logger.
fn create_file_logger(log_file: String) -> fern::Dispatch {
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(log_file.clone())
        .unwrap(); // Truncate the file first

    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "{} {:<5} {}", // No colour formatting here
                chrono::DateTime::<chrono::Local>::from(std::time::SystemTime::now())
                    .format("%H:%M:%S"), // Format time nicely
                record.level(),
                message
            ))
        })
        .level(log::LevelFilter::Debug)
        .chain(
            std::fs::OpenOptions::new()
                .append(true)
                .open(log_file)
                .unwrap(),
        )
}

/// Sets up the logger.
///
/// # Arguments
/// * `console` - Whether to log to the console.
/// * `log_file` - The file to log to (and so whether to log to a file).
///
/// # Returns
/// * `Result<(), fern::InitError>` - The result of setting up the logger.
pub fn setup(console: bool, log_file: Option<String>) -> Result<(), fern::InitError> {
    let mut logger = fern::Dispatch::new();

    if console {
        logger = logger.chain(create_console_logger());
    }

    if let Some(log_file) = log_file {
        logger = logger.chain(create_file_logger(log_file));
    }

    logger
        .level_for("reqwest", log::LevelFilter::Warn)
        .level_for("tokio-tungstenite", log::LevelFilter::Warn)
        .level_for("warp", log::LevelFilter::Warn)
        .apply()?;

    Ok(())
}
