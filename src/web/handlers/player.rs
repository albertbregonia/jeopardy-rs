use axum::extract::ws::close_code;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::mpsc::Receiver;

use crate::{global::ResponseType, web::game::LobbyManager};
use crate::{
    global::{JeopardyGlobalState, RequestType},
    json_websocket::{JsonWebSocket, JsonWebsocketError},
    web::game::{LobbyManagerError, lobby::LobbyError, player::Player},
};
use axum::{
    extract::{State, WebSocketUpgrade, ws::WebSocket},
    response::IntoResponse,
};
use tokio::sync::mpsc;

#[derive(Debug, Error)]
// top level error type for player events
// anything that isn't of the `Internal` variant is a user error
pub enum PlayerHandlerError {
    #[error("Incorrect password for the desired lobby: {0}")]
    IncorrectLobbyPassword(String),
    #[error("Expected a login request from the client that was not received.")]
    ExpectedLoginRequest,
    #[error("Lobby '{0}' does not exist.")]
    LobbyNotFound(String),
    #[error("Internal Server Error: {0}")]
    Internal(#[from] InternalServerError),
}

// enum rewrap simply to distinguish between user error and internal failure
#[derive(Debug, Error)]
pub enum InternalServerError {
    #[error("{0}")]
    WebSocket(#[from] JsonWebsocketError),
    #[error("{0}")]
    LobbyManager(#[from] LobbyManagerError),
    #[error("{0}")]
    LobbyError(#[from] LobbyError),
}

/// top level websocket handler
pub async fn websocket_upgrader(
    State(global_state): State<JeopardyGlobalState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // NOTE: a websocket is needed for players bc a live connection is needed for
    // bidirectional input (buzzer, live game state, etc.)
    ws.on_upgrade(|socket| websocket_handler(global_state, socket))
}

async fn websocket_handler(global_state: JeopardyGlobalState, socket: WebSocket) {
    let mut json_ws = JsonWebSocket::new(socket);
    // attempt to login and handle events, upon error, send it on websocket
    match login_handler(&global_state, &mut json_ws).await {
        Ok(LoginResponse {
            username,
            lobby_name,
            receiver,
        }) => {
            let player_result =
                player_handler(&global_state, &mut json_ws, receiver, &username).await;
            if let Err(e) = player_result {
                tracing::error!("Error during connection handling for {username}: {e}");
                let e = global_state
                    .write()
                    .await
                    .get_mut_manager()
                    .get_mut(&lobby_name)
                    .map_err(|e| InternalServerError::LobbyManager(e))
                    .map(|lobby| {
                        lobby
                            .remove_player(&username)
                            .map_err(|e| InternalServerError::LobbyError(e))
                    })
                    .flatten()
                    .err(); // we don't care about the player here, they will simply get dropped
                if let Some(internal_error) = e {
                    let prefix = format!("Failed to remove {username} from lobby: '{lobby_name}'");
                    handle_error(
                        json_ws,
                        &prefix,
                        &prefix,
                        PlayerHandlerError::Internal(internal_error),
                    )
                    .await
                }
            }
        }
        Err(e) => {
            handle_error(
                json_ws,
                "Failed to login user for internal failure",
                "User error during login",
                e,
            )
            .await
        }
    }
}

struct LoginResponse<T: Serialize> {
    username: String,
    lobby_name: String,
    receiver: Receiver<T>,
}

async fn login_handler(
    global_state: &JeopardyGlobalState,
    json_ws: &mut JsonWebSocket<RequestType, ResponseType>,
) -> Result<LoginResponse<ResponseType>, PlayerHandlerError> {
    // 1. parse request
    let RequestType::Login {
        username,
        lobby_name,
        password,
    } = json_ws
        .read_json()
        .await
        .map_err(|e| InternalServerError::WebSocket(e))?
    else {
        return Err(PlayerHandlerError::ExpectedLoginRequest);
    };

    // 2. get lobby
    let mut global_wg = global_state.write().await;
    let lobby = global_wg
        .get_mut_manager()
        .get_mut(&lobby_name)
        .map_err(|e| {
            tracing::error!("{e}");
            PlayerHandlerError::LobbyNotFound(lobby_name.clone())
        })?;

    // 3. auth
    if !lobby.is_correct_password(&password) {
        return Err(PlayerHandlerError::IncorrectLobbyPassword(lobby_name));
    }
    // create channel to communicate with the frontend
    let (writer, receiver) = mpsc::channel(1);
    let player = Player::new(username.clone(), writer);
    // 4. add to lobby
    lobby
        .add_player(player)
        .map_err(|e| InternalServerError::LobbyError(e))?;

    Ok(LoginResponse {
        username,
        lobby_name,
        receiver,
    })
}

async fn handle_error(
    json_ws: JsonWebSocket<RequestType, ResponseType>,
    internal_error_log_msg: &str,
    user_error_log_msg: &str,
    error: PlayerHandlerError,
) {
    let (code, error_msg) = match error {
        PlayerHandlerError::Internal(e) => {
            tracing::error!("{internal_error_log_msg}: {e}");
            (close_code::ERROR, "Internal Server Error".to_string())
        }
        other => {
            tracing::debug!("{user_error_log_msg}: {other}");
            (close_code::INVALID, other.to_string())
        }
    };
    let _ = json_ws
        .disconnect(code, Some(&error_msg))
        .await
        .inspect_err(|e| tracing::error!("Failed to handle error with JsonWebSocket: {e}"));
}

async fn player_handler(
    global_state: &JeopardyGlobalState,
    json_ws: &mut JsonWebSocket<RequestType, ResponseType>,
    receiver: Receiver<ResponseType>,
    username: &str,
) -> Result<(), PlayerHandlerError> {
    Ok(())
}
