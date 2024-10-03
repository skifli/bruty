use serde;

pub mod logger;
pub mod types;

pub const VALID_CHARS: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', 'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L',
    'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Y', 'Z', '0', '1', '2', '3', '4',
    '5', '6', '7', '8', '9', '-', '_',
];

/// Error codes sent with an InvalidSession OP code
#[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
pub enum ErrorCode {
    UnknownError,
    UnexpectedOP,
    DecodeError,
    AuthenticationFailed,
    UnsupportedClientVersion,
    NotAuthenticated,
    NotExpectingResults,
    WrongResultString,
    SessionTimeout,
}

impl ErrorCode {
    /// Populates an `InvalidSessionData` struct with the error code's data
    ///
    /// # Returns
    /// * `InvalidSessionData` - The populated `InvalidSessionData` struct
    pub fn populate(&self) -> InvalidSessionData {
        match self {
            ErrorCode::UnknownError => InvalidSessionData {
                code: ErrorCode::UnknownError,
                description: "Unknown error".to_string(),
                explanation: "We're not sure what went wrong.".to_string(),
            },
            ErrorCode::UnexpectedOP => InvalidSessionData {
                code: ErrorCode::UnexpectedOP,
                description: "Unexpected OP code".to_string(),
                explanation: "The server received an OP code it should not."
                    .to_string(),
            },
            ErrorCode::DecodeError => InvalidSessionData {
                code: ErrorCode::DecodeError,
                description: "Decode error".to_string(),
                explanation: "The server received an invalid payload."
                    .to_string(),
            },
            ErrorCode::AuthenticationFailed => InvalidSessionData {
                code: ErrorCode::AuthenticationFailed,
                description: "Authentication failed".to_string(),
                explanation: "The server received an invalid passphrase."
                    .to_string(),
            },
            ErrorCode::UnsupportedClientVersion => InvalidSessionData {
                code: ErrorCode::UnsupportedClientVersion,
                description: "Unsupported client version".to_string(),
                explanation: "The server received a client version it doesn't support. Try updating your client from https://github.com/skifli/bruty/releases."
                    .to_string(),
            },
            ErrorCode::NotAuthenticated => InvalidSessionData {
                code: ErrorCode::NotAuthenticated,
                description: "Not authenticated".to_string(),
                explanation: "You need to authenticate first.".to_string(),
            },
            ErrorCode::NotExpectingResults => InvalidSessionData {
                code: ErrorCode::NotExpectingResults,
                description: "Not expecting results".to_string(),
                explanation: "You didn't request a test.".to_string(),
            },
            ErrorCode::WrongResultString => InvalidSessionData {
                code: ErrorCode::WrongResultString,
                description: "Wrong result string".to_string(),
                explanation: "Your results don't start from the ID we are expecting.".to_string(),
            },
            ErrorCode::SessionTimeout => InvalidSessionData {
                code: ErrorCode::SessionTimeout,
                description: "Session timeout".to_string(),
                explanation: "You didn't send a heartbeat in time.".to_string(),
            },
        }
    }
}

/// WebSocket OP codes. Comments show client action and description.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum OperationCode {
    /// Send | Heartbeat
    Heartbeat,
    /// Send | Starts a new session
    Identify,
    /// Receive | Requests a test
    TestRequestData,
    /// Send | Sends the test results
    TestingResult,
    /// Receive | The session is invalid
    InvalidSession,
}

/// Data sent with an Identify OP code
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct IdentifyData {
    pub advanced_generations: u16,
    pub client_version: String,
    pub id: u8,
    pub secret: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TestRequestData {
    pub id: Vec<char>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TestingResultData {
    pub id: Vec<char>,
    pub positives: Vec<types::Video>,
}

/// Data sent with an InvalidSession OP code
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct InvalidSessionData {
    pub code: ErrorCode,
    pub description: String,
    pub explanation: String,
}

/// WebSocket payload data
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum Data {
    Heartbeat,
    Identify(IdentifyData),
    TestRequestData(TestRequestData),
    TestingResult(TestingResultData),
    InvalidSession(InvalidSessionData),
}

/// Represents a WebSocket payload
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct Payload {
    pub op_code: OperationCode,
    pub data: Data,
}
