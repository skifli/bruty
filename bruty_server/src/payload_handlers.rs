use crate::{SplitSinkExt, WebSocketSender};
use futures_util::SinkExt;

const ALLOWED_CLIENT_VERSIONS: &[&str] = &["0.3.0"];

/// Checks if the connection is authenticated.
/// If not, it sends an InvalidSession OP code and closes the connection.
///
/// # Arguments
/// * `websocket_sender` - The WebSocket sender.
/// * `session` - The session of the connection.
///
/// # Returns
/// * `bool` - If the connection is authenticated.
async fn check_authenticated(
    websocket_sender: &mut WebSocketSender,
    session: &bruty_share::types::Session,
) -> bool {
    if !session.authenticated {
        log::warn!("Unauthenticated request, closing connection.");

        websocket_sender
            .send_payload(bruty_share::Payload {
                op_code: bruty_share::OperationCode::InvalidSession,
                data: bruty_share::Data::InvalidSession(
                    bruty_share::ErrorCode::NotAuthenticated.populate(),
                ),
            })
            .await
            .unwrap();

        websocket_sender.close().await.unwrap();

        return false;
    }

    return true;
}

/// Handles the Identify OP code.
///
/// # Arguments
/// * `websocket_sender` - The WebSocket sender.
/// * `payload` - The WebSocket payload.
/// * `session` - The session of the connection.
/// * `persist` - The database connection.
/// * `server_channels` - The server's channels, used for communication between threads.
pub async fn identify(
    websocket_sender: &mut WebSocketSender,
    payload: bruty_share::Payload,
    session: &mut bruty_share::types::Session,
    persist: &shuttle_persist::PersistInstance,
    server_channels: &bruty_share::types::ServerChannels,
) {
    let users = persist
        .load::<Vec<bruty_share::types::User>>("users")
        .unwrap();

    // Directly access the IdentifyData
    let identify_data = if let bruty_share::Data::Identify(data) = payload.data {
        data // Unwraps the IdentifyData directly
    } else {
        log::warn!("Invalid payload data for Identify OP code");
        websocket_sender
            .send_payload(bruty_share::Payload {
                op_code: bruty_share::OperationCode::InvalidSession,
                data: bruty_share::Data::InvalidSession(
                    bruty_share::ErrorCode::DecodeError.populate(),
                ),
            })
            .await
            .unwrap();
        websocket_sender.close().await.unwrap();

        return;
    };

    if !ALLOWED_CLIENT_VERSIONS.contains(&&*identify_data.client_version) {
        log::warn!(
            "Unsupported client version {}, closing connection.",
            identify_data.client_version,
        );

        websocket_sender
            .send_payload(bruty_share::Payload {
                op_code: bruty_share::OperationCode::InvalidSession,
                data: bruty_share::Data::InvalidSession(
                    bruty_share::ErrorCode::UnsupportedClientVersion.populate(),
                ),
            })
            .await
            .unwrap();
        websocket_sender.close().await.unwrap();

        return;
    }

    let mut authentication_failed = false;

    if let Some(user) = users.iter().find(|u| u.id == identify_data.id) {
        if user.secret == identify_data.secret {
            session.authenticated = true;
            session.user = user.clone(); // Clone the user into the session
        } else {
            log::warn!("User {} authentication failed.", user.name,);

            authentication_failed = true;
        }
    } else {
        log::warn!("User {} not found.", identify_data.id,);

        authentication_failed = true;
    }

    if authentication_failed {
        websocket_sender
            .send_payload(bruty_share::Payload {
                op_code: bruty_share::OperationCode::InvalidSession,
                data: bruty_share::Data::InvalidSession(
                    bruty_share::ErrorCode::AuthenticationFailed.populate(),
                ),
            })
            .await
            .unwrap();
        websocket_sender.close().await.unwrap();

        return;
    }

    log::info!(
        "User {} (ID {}) authenticated.",
        session.user.name,
        session.user.id,
    );

    session.authenticated = true;

    for _ in 0..identify_data.advanced_generations {
        test_request(websocket_sender, session, &server_channels).await;
    }
}

/// Run if the an ID to test is triggered.
///
/// # Arguments
/// * `websocket_sender` - The WebSocket sender.
/// * `session` - The session of the connection.
/// * `server_channels` - The server's channels, used for communication between threads.
pub async fn test_request(
    websocket_sender: &mut WebSocketSender,
    session: &mut bruty_share::types::Session,
    server_channels: &bruty_share::types::ServerChannels,
) {
    let id = server_channels.id_receiver.recv().unwrap(); // Get the ID

    session.awaiting_results.push(id.clone()); // Add the ID to the awaiting results

    websocket_sender
        .send_payload(bruty_share::Payload {
            op_code: bruty_share::OperationCode::TestRequestData,
            data: bruty_share::Data::TestRequestData(bruty_share::TestRequestData { id }),
        })
        .await
        .unwrap();
}

/// Run if the client sends the test results.
/// It checks if the results are what we are expecting.
///
/// # Arguments
/// * `websocket_sender` - The WebSocket sender.
/// * `payload` - The WebSocket payload.
/// * `session` - The session of the connection.
/// * `server_channels` - The server's channels, used for communication between threads.
pub async fn testing_result(
    websocket_sender: &mut WebSocketSender,
    payload: bruty_share::Payload,
    session: &mut bruty_share::types::Session,
    server_channels: &bruty_share::types::ServerChannels,
) {
    if !check_authenticated(websocket_sender, session).await {
        return;
    }

    if session.awaiting_results.is_empty() {
        // We aren't expecting any results

        websocket_sender
            .send_payload(bruty_share::Payload {
                op_code: bruty_share::OperationCode::InvalidSession,
                data: bruty_share::Data::InvalidSession(
                    bruty_share::ErrorCode::NotExpectingResults.populate(),
                ),
            })
            .await
            .unwrap();
    }

    let testing_result_data = if let bruty_share::Data::TestingResult(data) = payload.data {
        data
    } else {
        log::warn!(
            "Invalid payload data for TestingResult OP code from {} (ID {}).",
            session.user.name,
            session.user.id,
        );
        websocket_sender
            .send_payload(bruty_share::Payload {
                op_code: bruty_share::OperationCode::InvalidSession,
                data: bruty_share::Data::InvalidSession(
                    bruty_share::ErrorCode::DecodeError.populate(),
                ),
            })
            .await
            .unwrap();
        websocket_sender.close().await.unwrap();

        return;
    };

    if !session.awaiting_results.contains(&testing_result_data.id) {
        // The result string is not what we are expecting
        log::warn!(
            "Expected results for any of {:?}, got results for {:?} from {} (ID {}).",
            session.awaiting_results,
            testing_result_data.id,
            session.user.name,
            session.user.id
        );

        websocket_sender
            .send_payload(bruty_share::Payload {
                op_code: bruty_share::OperationCode::InvalidSession,
                data: bruty_share::Data::InvalidSession(
                    bruty_share::ErrorCode::WrongResultString.populate(),
                ),
            })
            .await
            .unwrap();
        websocket_sender.close().await.unwrap();

        return;
    }

    session
        .awaiting_results
        .retain(|id| id != &testing_result_data.id); // Remove the ID from the awaiting results

    server_channels
        .results_received_sender
        .send(testing_result_data.id)
        .unwrap(); // Say we received the results

    server_channels
        .results_sender
        .send(testing_result_data.positives)
        .unwrap(); // Send the results to the results handler
}
