use axum::Json;
use axum::http::StatusCode;
use serde::Serialize;
use tokio::sync::mpsc::Receiver;
use tokio::time::timeout;

use crate::global::ResponseType;
use crate::handlers::{
    CREATE_LOBBY_ERROR_MSG, INVALID_LOBBY_NAME_ERROR_MSG, LOGIN_TIMEOUT, PlayerHandlerError,
    UserError,
};
use crate::web::game::lobby::{self, Lobby};
use crate::web::game::lobby_manager;
use crate::{
    global::{JeopardyGlobalState, RequestType},
    json_websocket::JsonWebSocket,
    web::game::{LobbyManagerError, player::Player},
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
    if !lobby_manager::is_valid_lobby_name(&lobby_name) {
        return (
            StatusCode::BAD_REQUEST,
            INVALID_LOBBY_NAME_ERROR_MSG.to_string(),
        );
    }
    let new_lobby = Lobby::new(lobby_name.clone(), password);
    let mut lobby_wg = global_state.write().await;
    match lobby_wg.get_mut_manager().add(new_lobby) {
        Ok(_) => (
            StatusCode::OK,
            format!("Lobby '{}' created successfully.", &lobby_name),
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
    ws.on_upgrade(|socket| async {
        let _ = timeout(
            LOGIN_TIMEOUT,
            websocket_handler(global_state, JsonWebSocket::new(socket)),
        );
    })
}

pub async fn websocket_handler(
    global_state: JeopardyGlobalState,
    mut json_ws: JsonWebSocket<RequestType, ResponseType>,
) {
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

/// internal struct to return the
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
    lobby.add_player(player)?;

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

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use std::sync::Arc;

    use crate::{
        global::{GlobalState, RequestType},
        handlers::player::{LoginResponse, login_handler},
        json_websocket::{self, JsonWebSocket, JsonWebsocketError, TextTransport},
        web::game::lobby::Lobby,
    };
    use bytes::Bytes;
    use tokio::sync::RwLock;

    const TEST_LOBBY_NAME: &str = "test_lobby";
    const TEST_LOBBY_PASSWORD: &str = "test_password";
    const TEST_USERNAME: &str = "test_username";

    pub struct LoginMockWebSocket {}

    #[async_trait::async_trait]
    impl TextTransport for LoginMockWebSocket {
        async fn read_text(&mut self) -> Result<Bytes, JsonWebsocketError> {
            let login_request = RequestType::Login {
                username: TEST_USERNAME.to_string(),
                lobby_name: TEST_LOBBY_NAME.to_string(),
                password: TEST_LOBBY_PASSWORD.to_string(),
            };
            let serialized = serde_json::to_vec(&login_request)
                .map_err(|e| json_websocket::InternalError::Json(e))?;
            Ok(Bytes::from(serialized))
        }
        async fn send_text(&mut self, _msg: &str) -> Result<(), JsonWebsocketError> {
            Ok(())
        }
        async fn disconnect(
            self: Box<Self>,
            _user_error: bool,
            _msg: Option<&str>,
        ) -> Result<(), JsonWebsocketError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn GIVEN_json_ws_WHEN_login_handler_THEN_ok() {
        // GIVEN
        let mut json_ws = JsonWebSocket::new(LoginMockWebSocket {});
        let global_state = Arc::new(RwLock::new(GlobalState::new()));

        // create test lobby for the test player to join
        global_state
            .write()
            .await
            .get_mut_manager()
            .add(Lobby::new(
                TEST_LOBBY_NAME.to_string(),
                TEST_LOBBY_PASSWORD.to_string(),
            ))
            .unwrap();

        // WHEN
        let result = login_handler(&global_state, &mut json_ws).await;

        // THEN
        assert!(matches!(
            result,
            Ok(LoginResponse{ username, lobby_name, .. })
                if username == TEST_USERNAME && lobby_name == TEST_LOBBY_NAME
        ));
        let player_found_in_lobby = global_state
            .write()
            .await
            .get_manager()
            .get(TEST_LOBBY_NAME)
            .unwrap()
            .get_player(TEST_USERNAME)
            .is_ok_and(|player| player.get_name() == TEST_USERNAME);
        assert!(player_found_in_lobby)
    }

    // TODO: negative login tests,
}
