/// Represents the server's state.
#[derive(Clone, serde::Deserialize, serde::Serialize)]
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
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub enum VideoEvent {
    Success,
    NotEmbeddable,
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

/// Represents the server's channels.
#[derive(Clone)]
pub struct ServerChannels {
    pub id_receiver: flume::Receiver<Vec<char>>,
    pub id_sender: flume::Sender<Vec<char>>,
    pub results_sender: flume::Sender<Vec<Video>>,
    pub results_received_sender: flume::Sender<Vec<char>>,
    pub results_awaiting_sender: flume::Sender<Vec<char>>,
}

/// Represents a connected client's session.
#[derive(Clone)]
pub struct Session {
    pub authenticated: bool,
    pub awaiting_results: Vec<char>,
    pub heartbeat_received: bool,
    pub heartbeat_timer: tokio::time::Instant,
    pub ip: String,
    pub user: User,
}

/// Represents a user.
#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
pub struct User {
    pub id: i16,
    pub name: String,
    pub secret: String,
}
