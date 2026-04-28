use axum::Json;
use axum::http::StatusCode;
use serde::Serialize;
use tokio::sync::mpsc::Receiver;

use crate::global::ResponseType;
use crate::handlers::{CREATE_LOBBY_ERROR_MSG, INVALID_LOBBY_NAME_ERROR_MSG, InternalError, PlayerHandlerError, UserError};
use crate::web::game::lobby::{self, Lobby};
use crate::{
    global::{JeopardyGlobalState, RequestType},
    json_websocket::JsonWebSocket,
    web::game::{LobbyManagerError, lobby::LobbyError, player::Player},
};
use axum::{
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
};
use tokio::sync::mpsc;

pub async fn create_lobby(
    State(global_state): State<JeopardyGlobalState>,
    Json(create_lobby_request): Json<RequestType>,
) -> impl IntoResponse {
    let RequestType::CreateLobby {
        lobby_name,
        password,
    } = create_lobby_request
    else {
        return (StatusCode::BAD_REQUEST, CREATE_LOBBY_ERROR_MSG.to_string());
    };
    if !lobby::is_valid_lobby_name(&lobby_name) {
        return (
            StatusCode::BAD_REQUEST,
            INVALID_LOBBY_NAME_ERROR_MSG.to_string(),
        );
    }
    let new_lobby = Lobby::new(lobby_name, password);
    let mut lobby_wg = global_state.write().await;
    match lobby_wg.get_mut_manager().add(new_lobby) {
        Ok(ref lobby) => (
            StatusCode::OK,
            format!("Lobby '{}' created successfully.", lobby.get_name()),
        ),
        Err(e) => match e {
            LobbyManagerError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            user_error => (StatusCode::BAD_REQUEST, user_error.to_string()),
        },
    }
}

/// top level websocket handler
pub async fn websocket_upgrader(
    State(global_state): State<JeopardyGlobalState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // NOTE: a websocket is needed for players bc a live connection is needed for
    // bidirectional input (buzzer, live game state, etc.)
    ws.on_upgrade(|socket| websocket_handler(global_state, JsonWebSocket::new(socket)))
}

pub async fn websocket_handler(global_state: JeopardyGlobalState, mut json_ws: JsonWebSocket<RequestType, ResponseType>) {
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
                    .map_err(|e| PlayerHandlerError::from(e))
                    .map(|lobby| lobby.remove_player(&username))
                    .err(); // we don't care about the return value here, drop
                if let Some(internal_error) = e {
                    let prefix = format!("Failed to remove {username} from lobby: '{lobby_name}'");
                    handle_error(json_ws, &prefix, &prefix, internal_error).await
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

pub struct LoginResponse<T: Serialize> {
    username: String,
    lobby_name: String,
    receiver: Receiver<T>,
}

pub async fn login_handler(
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
        .map_err(|e| PlayerHandlerError::from(e))?
    else {
        return Err(PlayerHandlerError::User(UserError::ExpectedLoginRequest));
    };

    // 2. get lobby
    let mut global_wg = global_state.write().await;
    let lobby = global_wg.get_mut_manager().get_mut(&lobby_name)?;

    // 3. auth
    if !lobby.is_correct_password(&password) {
        return Err(PlayerHandlerError::User(UserError::IncorrectLobbyPassword(
            lobby_name,
        )));
    }
    // create channel to communicate with the frontend
    let (writer, receiver) = mpsc::channel(1);
    let player = Player::new(username.clone(), writer);
    // 4. add to lobby
    lobby.add_player(player).map_err(|e| match e {
        LobbyError::Internal(internal_error) => {
            PlayerHandlerError::Internal(InternalError::LobbyError(internal_error))
        }
        LobbyError::User(user_error) => PlayerHandlerError::User(UserError::LobbyError(user_error)),
    })?;

    Ok(LoginResponse {
        username,
        lobby_name,
        receiver,
    })
}

pub async fn handle_error(
    json_ws: JsonWebSocket<RequestType, ResponseType>,
    internal_error_log_msg: &str,
    user_error_log_msg: &str,
    error: PlayerHandlerError,
) {
    let (user_error, error_msg) = match error {
        PlayerHandlerError::Internal(e) => {
            tracing::error!("{internal_error_log_msg}: {e}");
            (false, "Internal Server Error".to_string())
        }
        other => {
            tracing::debug!("{user_error_log_msg}: {other}");
            (true, other.to_string())
        }
    };
    let _ = json_ws
        .disconnect(user_error, Some(&error_msg))
        .await
        .inspect_err(|e| tracing::error!("Failed to handle error with JsonWebSocket: {e}"));
}

pub async fn player_handler(
    global_state: &JeopardyGlobalState,
    json_ws: &mut JsonWebSocket<RequestType, ResponseType>,
    receiver: Receiver<ResponseType>,
    username: &str,
) -> Result<(), PlayerHandlerError> {
    Ok(())
}
