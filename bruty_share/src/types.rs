use crate::Payload;

/// Represents the server's state operator.
#[derive(Debug)]
pub struct ServerState {
    pub pool: sqlx::PgPool,
}

// Represents an inner value in the server's state.
#[derive(serde::Deserialize, serde::Serialize)]
pub struct ServerStateInner {
    pub inner: Vec<char>,
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
#[derive(PartialEq, serde::Deserialize, serde::Serialize)]
pub enum VideoEvent {
    Success,
    NotEmbeddable,
    NotFound,
}

/// Represents video data.
#[derive(serde::Deserialize, serde::Serialize)]
pub struct VideoData {
    pub title: String,
    pub author_name: String,
    pub author_url: String,
}

// Represents a video event.
#[derive(serde::Deserialize, serde::Serialize)]
pub struct Video {
    pub event: VideoEvent,
    pub id: Vec<char>,                 // ID that was tested
    pub video_data: Option<VideoData>, // Only present if Event is Success
}

/// Represents the different types of events that can occur.
#[derive(serde::Deserialize, serde::Serialize)]
pub enum ServerEvent {
    ResultsAwaiting(Vec<char>),
    ResultsReceived(Vec<char>),
    PositiveResultsReceived(Vec<Video>),
}

/// Represents the server's channels.
#[derive(Clone)]
pub struct ServerData {
    pub current_id_receiver: async_channel::Receiver<Vec<char>>, // Only here because it's a bit easier than putting in ClientChannels and also having to pass that around everywhere
    pub current_id_sender: async_channel::Sender<Vec<char>>,
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
