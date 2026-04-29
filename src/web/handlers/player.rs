use axum::Json;
use axum::http::StatusCode;
use tokio::sync::mpsc::{self, Receiver};
use tokio::time::timeout;

use crate::global::ResponseType;
use crate::handlers::{
    CREATE_LOBBY_ERROR_MSG, INVALID_LOBBY_NAME_ERROR_FORMAT_MSG, InternalError, LOGIN_TIMEOUT,
    MAX_NAME_LENGTH, PlayerHandlerError, UserError,
};
use crate::json_websocket::JsonWebsocketError;
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

pub async fn create_lobby(
    State(global_state): State<JeopardyGlobalState>,
    Json(create_lobby_request): Json<RequestType>,
) -> impl IntoResponse {
    let RequestType::CreateLobby {
        lobby_name,
        password,
    } = create_lobby_request
    else {
        tracing::warn!("Invalid JSON received in place of create lobby request");
        return (StatusCode::BAD_REQUEST, CREATE_LOBBY_ERROR_MSG.to_string());
    };
    if !lobby_manager::is_valid_lobby_name(&lobby_name, MAX_NAME_LENGTH) {
        tracing::warn!("Invalid password attempt on {lobby_name}");
        return (
            StatusCode::BAD_REQUEST,
            INVALID_LOBBY_NAME_ERROR_FORMAT_MSG.to_string(),
        );
    }
    let new_lobby = Lobby::new(lobby_name.clone(), password);
    let mut lobby_wg = global_state.write().await;
    match lobby_wg.get_mut_manager().add(new_lobby) {
        Ok(_) => {
            tracing::info!("Successfully created '{lobby_name}'");
            (
                StatusCode::OK,
                format!("Lobby '{lobby_name}' created successfully."),
            )
        }
        Err(e) => match e {
            LobbyManagerError::Internal(internal_error) => {
                tracing::error!("Internal Server Error during lobby creation: {internal_error}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    internal_error.to_string(),
                )
            }
            user_error => {
                tracing::warn!("User error during lobby creation: {user_error}");
                (StatusCode::BAD_REQUEST, user_error.to_string())
            }
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

pub async fn websocket_handler(
    global_state: JeopardyGlobalState,
    json_ws: JsonWebSocket<RequestType, ResponseType>,
) {
    let mut conn = PlayerConnection::new(global_state, json_ws);
    let receiver = loop {
        // TODO: throttling
        match conn.login().await {
            Ok(receiver) => break receiver,
            Err(e) => match e {
                PlayerHandlerError::User(user_error) => {
                    tracing::warn!("User error during login: {user_error}");
                    let _ = conn.handle_user_error(user_error).await.inspect_err(|e| {
                        tracing::error!("Failure during user error handling: {e}")
                    });
                }
                PlayerHandlerError::Internal(internal_error) => {
                    tracing::warn!("Internal server error during login: {internal_error}");
                    let _ = conn
                        .handle_internal_error(internal_error)
                        .await
                        .inspect_err(|e| {
                            tracing::error!("Failure during internal error handling: {e}")
                        });
                    return; // conn gets dropped here and disconnects regardless
                }
            },
        }
    };
    if let Err(e) = conn.main(receiver).await {
        match e {
            PlayerHandlerError::User(user_error) => {
                tracing::error!("User error during main player handling: {user_error}");
            }
            PlayerHandlerError::Internal(internal_error) => {
                tracing::error!(
                    "Internal server error during main player handling: {internal_error}"
                );
            }
        }
        // NOTE: no matter the error, disconnect the player from the lobby
        let _ = conn
            .leave_lobby()
            .await
            .inspect_err(|e| tracing::error!("Failure during removal of player from lobby: {e}"));
    }
}

// thin wrapper around JsonWebSocket and JeopardyGlobalState
// to easier handle errors and lifetimes of the player
pub struct PlayerConnection {
    global_state: JeopardyGlobalState,
    json_ws: JsonWebSocket<RequestType, ResponseType>,
    creds: Option<LoginResponse>,
}

impl PlayerConnection {
    pub fn new(
        global_state: JeopardyGlobalState,
        json_ws: JsonWebSocket<RequestType, ResponseType>,
    ) -> Self {
        PlayerConnection {
            global_state,
            json_ws,
            creds: None,
        }
    }

    pub async fn leave_lobby(mut self) -> Result<(), PlayerHandlerError> {
        let Some(LoginResponse {
            ref username,
            ref lobby_name,
        }) = self.creds
        else {
            return Err(PlayerHandlerError::User(UserError::ExpectedLoginRequest));
        };
        let lobby_removal_error = self
            .global_state
            .write()
            .await
            .get_mut_manager()
            .get_mut(lobby_name)
            .map(|lobby| lobby.remove_player(username))
            .err(); // we don't care about the return value here, drop

        if let Some(lobby_manager_error) = lobby_removal_error {
            tracing::error!("Failed to remove {username} from lobby: '{lobby_name}'");
            match lobby_manager_error {
                LobbyManagerError::User(user_error) => {
                    self.handle_user_error(user_error.into()).await?;
                }
                LobbyManagerError::Internal(internal_error) => {
                    self.handle_internal_error(internal_error.into()).await?;
                }
            };
        }
        Ok(())
    }

    pub async fn login(&mut self) -> Result<Receiver<ResponseType>, PlayerHandlerError> {
        // 1. parse request
        let RequestType::Login {
            username,
            lobby_name,
            password,
        } = timeout(LOGIN_TIMEOUT, self.json_ws.read_json()) // prevent soft lock
            .await
            .map_err(|_| {
                tracing::warn!("Player timed out attempting to connect to a lobby");
                UserError::ExpectedLoginRequest
            })??
        else {
            return Err(PlayerHandlerError::User(UserError::ExpectedLoginRequest));
        };

        // NOTE: we are reusing the same validation logic for player names and lobby names
        if !lobby_manager::is_valid_lobby_name(&username, MAX_NAME_LENGTH) {
            tracing::warn!("Invalid username by '{username}' for lobby: '{lobby_name}'");
            return Err(PlayerHandlerError::User(UserError::LobbyError(
                lobby::UserError::InvalidUsername(username, MAX_NAME_LENGTH),
            )));
        }

        // 2. get lobby
        let mut global_wg = self.global_state.write().await;
        let lobby = global_wg.get_mut_manager().get_mut(&lobby_name)?;

        // 3. auth
        if !lobby.is_correct_password(&password) {
            tracing::warn!("Incorrect lobby password by '{username}' for lobby: '{lobby_name}'");
            return Err(PlayerHandlerError::User(UserError::IncorrectLobbyPassword(
                lobby_name,
            )));
        }
        // create channel to communicate with the frontend
        // NOTE: normally, you would expect a broadcast::channel() instead of mpsc::channel()
        // and everyone gets a clone from global state.
        // however, i wanted to allow for direct communication with a specific player
        let (writer, receiver) = mpsc::channel(1);
        let player = Player::new(username.clone(), writer);
        // 4. add to lobby
        lobby.add_player(player)?;
        tracing::info!("Successful login by '{username}' for lobby: '{lobby_name}'");
        self.creds = Some(LoginResponse {
            username,
            lobby_name,
        });
        Ok(receiver)
    }

    pub async fn main(
        &mut self,
        mut receiver: Receiver<ResponseType>,
    ) -> Result<(), PlayerHandlerError> {
        tokio::select! {
            result = self.json_ws.read_json() => {
                let request = result?;
                // TODO: handle requests from frontend
            }
            result = receiver.recv() => {
                let response = result.ok_or(PlayerHandlerError::Internal(InternalError::EndOfChannel))?;
                self.json_ws.send_json(&response).await?;
            }
        }
        Ok(())
    }

    pub async fn handle_user_error(&mut self, e: UserError) -> Result<(), JsonWebsocketError> {
        self.json_ws
            .send_json(&ResponseType::UserError {
                error_msg: e.to_string(),
            })
            .await
    }

    pub async fn handle_internal_error(self, e: InternalError) -> Result<(), JsonWebsocketError> {
        self.json_ws.disconnect(false, Some(&e.to_string())).await
    }
}

/// internal struct to return the login params
pub struct LoginResponse {
    username: String,
    lobby_name: String,
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use std::sync::Arc;

    use crate::{
        global::{GlobalState, RequestType},
        handlers::{
            LOGIN_TIMEOUT,
            player::{LoginResponse, PlayerConnection},
        },
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
        let json_ws = JsonWebSocket::new(LoginMockWebSocket {});
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

        let mut conn = PlayerConnection::new(global_state.clone(), json_ws);
        // WHEN

        let result = conn.login().await;

        // THEN
        assert!(result.is_ok());
        assert!(matches!(
            conn.creds,
            Some(LoginResponse{username, lobby_name})
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
