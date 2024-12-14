use crate::Payload;

/// Represents the server's state.
#[derive(Clone, std::fmt::Debug, serde::Deserialize, serde::Serialize)]
pub struct ServerState {
    pub current_id: Vec<char>,
    pub starting_id: Vec<char>,
}

// Represents when a user joins or leaves.
pub enum UserEventType {
    Join,
    Leave,
}

/// Represents a user event.
pub struct UserEvent {
    pub event: UserEventType,
    pub user: Session,
}

/// Represents what type of video event occurred.
#[derive(Debug, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum VideoEvent {
    Success,
    NotEmbeddable,
    NotFound,
}

/// Represents video data.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct VideoData {
    pub title: String,
    pub author_name: String,
    pub author_url: String,
}

// Represents a video event.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Video {
    pub event: VideoEvent,
    pub id: Vec<char>,                 // ID that was tested
    pub video_data: Option<VideoData>, // Only present if Event is Success
}

/// Represents the different types of events that can occur.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum ServerEvent {
    ResultsAwaiting(Vec<char>),
    ResultsReceived(Vec<char>),
    PositiveResultsReceived(Vec<Video>),
}

/// Represents the server's channels.
#[derive(Clone)]
pub struct ServerData {
    pub current_id: std::sync::Arc<tokio::sync::Mutex<Vec<char>>>,
    pub event_receiver: flume::Receiver<ServerEvent>,
    pub event_sender: flume::Sender<ServerEvent>,
    pub users: Vec<User>,
    pub users_connected_num: std::sync::Arc<std::sync::atomic::AtomicU8>,
}

#[derive(Clone)]
pub struct ClientChannels {
    pub base_id_receiver: flume::Receiver<Vec<char>>,
    pub base_id_sender: flume::Sender<Vec<char>>,
    pub id_receiver: flume::Receiver<Vec<char>>,
    pub id_sender: flume::Sender<Vec<char>>,
    pub payload_send_receiver: flume::Receiver<Payload>,
    pub payload_send_sender: flume::Sender<Payload>,
    pub results_receiver: flume::Receiver<Video>,
    pub results_sender: flume::Sender<Video>,
}

/// Represents a connected client's session.
#[derive(Clone)]
pub struct Session {
    pub authenticated: bool,
    pub awaiting_result: Vec<char>,
    pub heartbeat_received: bool,
    pub user: User,
}

/// Represents a user.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct User {
    pub id: u8,
    pub name: String,
    pub secret: String,
}
