use std::time::Duration;

use axum::Json;
use axum::http::StatusCode;
use tokio::sync::mpsc::{self, Receiver};
use tokio::time::timeout;

use crate::global::ResponseType;
use crate::handlers::{
    InternalError, LOGIN_TIMEOUT, MAX_LOGIN_ATTEMPTS, MAX_NAME_LENGTH, PlayerHandlerError,
    UserError,
};
use crate::json_websocket::{JsonWebsocketError, TextTransport};
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
) -> (StatusCode, String) {
    let RequestType::CreateLobby {
        lobby_name,
        password,
    } = create_lobby_request
    else {
        tracing::warn!("Invalid JSON received in place of create lobby request");
        return (
            StatusCode::BAD_REQUEST,
            "Malformed create lobby request".to_string(),
        );
    };
    if !lobby_manager::is_valid_lobby_name(&lobby_name, MAX_NAME_LENGTH) {
        tracing::warn!("Invalid create lobby attempt with name: {lobby_name}");
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid lobby name. Must be lowercase and alphanumeric (special chars permitted) with length 0-{}",
                MAX_NAME_LENGTH
            ),
        );
    }
    // NOTE: validation logic is reused for password as well as lobby name
    if !lobby_manager::is_valid_lobby_name(&password, MAX_NAME_LENGTH) {
        tracing::warn!("Invalid create lobby attempt with password: {password}");
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "Invalid password. Must be lowercase and alphanumeric (special chars permitted) with length 0-{}",
                MAX_NAME_LENGTH
            ),
        );
    }
    let new_lobby = Lobby::new(lobby_name.clone(), password);
    let mut lobby_wg = global_state.write().await;
    if let Err(e) = lobby_wg.get_mut_manager().add(new_lobby) {
        let response = match e {
            LobbyManagerError::Internal(internal_error) => {
                tracing::error!("Internal Server Error during lobby creation: {internal_error}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal Server Error".to_string(),
                )
            }
            user_error => {
                tracing::warn!("User error during lobby creation: {user_error}");
                (StatusCode::BAD_REQUEST, user_error.to_string())
            }
        };
        return response;
    }
    tracing::info!("Successfully created '{lobby_name}'");
    (
        StatusCode::OK,
        format!("Lobby '{lobby_name}' created successfully."),
    )
}

/// top level websocket handler
pub async fn websocket_upgrader(
    State(global_state): State<JeopardyGlobalState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // NOTE: a websocket is needed for players bc a live connection is needed for
    // bidirectional input (buzzer, live game state, etc.)
    ws.on_upgrade(|socket| async move {
        let json_ws = JsonWebSocket::new(socket);
        let mut conn = PlayerConnection::new(global_state.clone(), json_ws);
        // reuse connection loop if a player wants to connect on a different lobby
        // TODO: however, we will need to think of softlocks here and unused connections piling up
        // the login request timeout should be configured to easily reuse the connection
        // but also shut down unused connections properly (lowk like a cache)
        loop {
            if let Err(e) = player_state_machine(&mut conn).await {
                // NOTE: this may be confusing!
                // since we may encounter a fatal error and need to close the connection,
                // we need to give up ownership temporarily.
                // if the connection is not consumed, it is passed back for reuse.
                conn = match player_error_handler(conn, e).await {
                    Some(conn) => conn,
                    None => return,
                };
            }
        }
    })
}

pub async fn player_error_handler<W>(
    mut conn: PlayerConnection<W>,
    e: PlayerHandlerError,
) -> Option<PlayerConnection<W>>
where
    W: TextTransport,
{
    match e {
        PlayerHandlerError::User(user_error) => {
            tracing::warn!("User error: {user_error}");
            let _ = match user_error {
                UserError::ExceededAttemptLimit => {
                    let _ = conn
                        .handle_irrecoverable_user_error(user_error)
                        .await
                        .inspect_err(|e| {
                            tracing::warn!("Fatal failure during user error handling: {e}")
                        });
                    return None;
                }
                _ => conn
                    .handle_recoverable_user_error(user_error)
                    .await
                    .inspect_err(|e| tracing::warn!("Failure during user error handling: {e}")),
            };
        }
        PlayerHandlerError::Internal(internal_error) => {
            tracing::error!("Internal server error: {internal_error}");
            let _ = conn
                .handle_internal_error(internal_error)
                .await
                .inspect_err(|e| tracing::error!("Failure during internal error handling {e}"));
            return None;
        }
    }
    Some(conn)
}

pub async fn player_state_machine<W: TextTransport>(
    conn: &mut PlayerConnection<W>,
) -> Result<(), PlayerHandlerError> {
    let receiver = conn.login_loop(MAX_LOGIN_ATTEMPTS).await?;
    if let Err(main_error) = conn.main(receiver).await {
        // NOTE: no matter the error, disconnect the player from the lobby
        // the inspect_err here is to log the leave_lobby() error if fails.
        // this is because it is more important to inform the player of the error that caused their disconnect
        // rather than whatever internal issue that caused the leave_lobby to fail
        // if this was prod there would be some sort of error metric emitted to alarm
        let _ = conn
            .leave_lobby()
            .await
            .inspect_err(|e| tracing::error!("Failed to remove player from lobby: {e}"));
        return Err(main_error);
    }
    Ok(())
}

// thin wrapper around JsonWebSocket and JeopardyGlobalState
// to easier handle errors and lifetimes of the player
pub struct PlayerConnection<W: TextTransport> {
    global_state: JeopardyGlobalState,
    json_ws: JsonWebSocket<W, RequestType, ResponseType>,
    creds: Option<LoginResponse>,
}

// TODO: ensure that PlayerConnection is reusable and be reused in other lobbies
impl<W> PlayerConnection<W>
where
    W: TextTransport,
{
    pub fn new(
        global_state: JeopardyGlobalState,
        json_ws: JsonWebSocket<W, RequestType, ResponseType>,
    ) -> Self {
        PlayerConnection {
            global_state,
            json_ws,
            creds: None,
        }
    }

    pub async fn leave_lobby(&mut self) -> Result<Player<ResponseType>, PlayerHandlerError> {
        let Some(LoginResponse {
            username,
            lobby_name,
        }) = self.creds.take()
        else {
            return Err(PlayerHandlerError::Internal(InternalError::UserNotLoggedIn));
        };
        let player = self
            .global_state
            .write()
            .await
            .get_mut_manager()
            .get_mut(&lobby_name)
            .map(|lobby| lobby.remove_player(&username))??;
        Ok(player)
    }

    pub async fn read_request_with_timeout(
        &mut self,
        max_timeout: Duration,
    ) -> Result<RequestType, PlayerHandlerError> {
        let request = timeout(max_timeout, self.json_ws.read_json())
            .await
            .map_err(UserError::RequestTimeout)??;
        Ok(request)
    }

    pub async fn join_lobby(
        &mut self,
        login: RequestType,
    ) -> Result<Receiver<ResponseType>, PlayerHandlerError> {
        // 1. parse request
        let RequestType::Login {
            username,
            lobby_name,
            password,
        } = login
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

    /// re-attempts join_lobby attempt `max_attempts` times
    /// sending the user error message over the websocket when unsuccessful
    pub async fn login_loop(
        &mut self,
        max_attempts: usize,
    ) -> Result<Receiver<ResponseType>, PlayerHandlerError> {
        for attempt in 1..=max_attempts {
            let login = self.read_request_with_timeout(LOGIN_TIMEOUT).await?;
            match self.join_lobby(login).await {
                Ok(receiver) => return Ok(receiver),
                Err(e) => match e {
                    PlayerHandlerError::User(user_error) => {
                        tracing::warn!("Attempt {attempt} - User error during login: {user_error}");
                        self.handle_recoverable_user_error(user_error).await?;
                    }
                    other => return Err(other),
                },
            }
        }
        Err(PlayerHandlerError::User(UserError::ExceededAttemptLimit))
    }

    pub async fn main(
        &mut self,
        mut receiver: Receiver<ResponseType>,
    ) -> Result<(), PlayerHandlerError> {
        loop {
            tokio::select! {
                result = self.json_ws.read_json() => {
                    let _request = result?;
                    // TODO: handle requests from frontend
                    // NOTE: we will have to remember when to drop the lobby
                }

                // if this gets dropped => internal error => connection gets dropped
                result = receiver.recv() => {
                    let response = result.ok_or(PlayerHandlerError::Internal(InternalError::EndOfChannel))?;
                    self.json_ws.send_json(&response).await?;
                }
            }
        }
    }

    pub async fn handle_recoverable_user_error(
        &mut self,
        e: UserError,
    ) -> Result<(), JsonWebsocketError> {
        self.json_ws
            .send_json(&ResponseType::UserError {
                error_msg: e.to_string(),
            })
            .await
    }

    pub async fn handle_irrecoverable_user_error(
        self,
        e: UserError,
    ) -> Result<(), JsonWebsocketError> {
        self.json_ws.disconnect(true, Some(&e.to_string())).await
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
mod test_consts {
    pub const TEST_LOBBY_NAME: &str = "test_lobby";
    pub const TEST_LOBBY_PASSWORD: &str = "test_password";
    pub const TEST_USERNAME: &str = "test_username";
}

#[cfg(test)]
#[allow(non_snake_case)]
mod create_lobby_tests {
    use super::*;
    use crate::handlers::player::test_consts::*;
    use crate::*;

    #[tokio::test]
    async fn GIVEN_create_lobby_request_WHEN_create_lobby_THEN_ok() {
        // GIVEN
        let request = RequestType::CreateLobby {
            lobby_name: TEST_LOBBY_NAME.to_string(),
            password: TEST_LOBBY_PASSWORD.to_string(),
        };
        let global_state = Arc::new(RwLock::new(GlobalState::new()));
        // WHEN
        let (status, msg) = create_lobby(State(global_state), Json(request)).await;
        // THEN
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            msg,
            format!("Lobby '{TEST_LOBBY_NAME}' created successfully.")
        );
    }

    #[tokio::test]
    async fn GIVEN_invalid_lobby_name_create_lobby_request_WHEN_create_lobby_THEN_err() {
        // GIVEN
        let request = RequestType::CreateLobby {
            lobby_name: "".to_string(), // invalid lobby name
            password: TEST_LOBBY_PASSWORD.to_string(),
        };
        let global_state = Arc::new(RwLock::new(GlobalState::new()));
        // WHEN
        let (status, msg) = create_lobby(State(global_state), Json(request)).await;
        // THEN
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(msg.contains("Invalid lobby name."));
    }

    #[tokio::test]
    async fn GIVEN_invalid_lobby_password_create_lobby_request_WHEN_create_lobby_THEN_err() {
        // GIVEN
        let request = RequestType::CreateLobby {
            lobby_name: TEST_LOBBY_NAME.to_string(), // invalid lobby name
            password: "".to_string(),
        };
        let global_state = Arc::new(RwLock::new(GlobalState::new()));
        // WHEN
        let (status, msg) = create_lobby(State(global_state), Json(request)).await;
        // THEN
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(msg.contains("Invalid password."));
    }

    #[tokio::test]
    async fn GIVEN_non_create_lobby_request_WHEN_create_lobby_THEN_err() {
        // GIVEN
        let request = RequestType::Buzzer; // not create_lobby request
        let global_state = Arc::new(RwLock::new(GlobalState::new()));
        // WHEN
        let (status, msg) = create_lobby(State(global_state), Json(request)).await;
        // THEN
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(msg, "Malformed create lobby request");
    }

    #[tokio::test]
    async fn GIVEN_existing_lobby_WHEN_create_lobby_THEN_err() {
        // GIVEN
        let request = RequestType::CreateLobby {
            lobby_name: TEST_LOBBY_NAME.to_string(),
            password: TEST_LOBBY_PASSWORD.to_string(),
        };
        let global_state = Arc::new(RwLock::new(GlobalState::new()));
        let (status, msg) = create_lobby(State(global_state.clone()), Json(request.clone())).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            msg,
            format!("Lobby '{TEST_LOBBY_NAME}' created successfully.")
        );
        // WHEN
        let (status, msg) = create_lobby(State(global_state), Json(request)).await;
        // THEN
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(
            msg,
            LobbyManagerError::User(lobby_manager::UserError::LobbyAlreadyExists(
                TEST_LOBBY_NAME.to_string()
            ))
            .to_string()
        )
    }

    // TODO: these tests do not handle internal server error from LobbyManager
    // (but LobbyMap doesn't ever actually return one so we'll have to mock somehow)
}

#[cfg(test)]
#[allow(non_snake_case)]
mod login_tests {
    use super::*;
    use crate::handlers::player::test_consts::*;
    use crate::json_websocket::MockReadSocket;
    use crate::*;
    use tokio::sync::Notify;

    // helper function to create a mock player connection and empty lobby
    async fn create_test_lobby_and_player_conn() -> PlayerConnection<MockReadSocket<RequestType>> {
        let login_request = RequestType::Login {
            username: TEST_USERNAME.to_string(),
            lobby_name: TEST_LOBBY_NAME.to_string(),
            password: TEST_LOBBY_PASSWORD.to_string(),
        };
        let socket = MockReadSocket { msg: login_request };
        let json_ws = JsonWebSocket::new(socket);
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

        PlayerConnection::new(global_state.clone(), json_ws)
    }

    #[tokio::test]
    async fn GIVEN_player_conn_WHEN_leave_lobby_THEN_ok() {
        // GIVEN
        let mut conn = create_test_lobby_and_player_conn().await;
        conn.login_loop(1).await.unwrap(); // join the lobby
        // WHEN
        let player = conn.leave_lobby().await.unwrap();
        // THEN
        assert_eq!(player.get_name(), TEST_USERNAME);
        assert!(conn.creds.is_none());
        let player_not_found_in_lobby = conn
            .global_state
            .write()
            .await
            .get_manager()
            .get(TEST_LOBBY_NAME)
            .unwrap()
            .get_player(TEST_USERNAME)
            .is_err();
        assert!(player_not_found_in_lobby)
    }

    #[tokio::test]
    async fn GIVEN_player_conn_not_logged_in_WHEN_leave_lobby_THEN_err() {
        // GIVEN - conn and lobby exist but player not in lobby
        let mut conn = create_test_lobby_and_player_conn().await;
        // WHEN
        let result = conn.leave_lobby().await;
        // THEN
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(PlayerHandlerError::Internal(InternalError::UserNotLoggedIn))
        ));
        assert!(conn.creds.is_none());
        let player_not_found_in_lobby = conn
            .global_state
            .write()
            .await
            .get_manager()
            .get(TEST_LOBBY_NAME)
            .unwrap()
            .get_player(TEST_USERNAME)
            .is_err();
        assert!(player_not_found_in_lobby)
    }

    #[tokio::test]
    async fn GIVEN_player_conn_WHEN_login_handler_THEN_ok() {
        // GIVEN
        let mut conn = create_test_lobby_and_player_conn().await;

        // WHEN
        let result = conn.login_loop(1).await;

        // THEN
        assert!(result.is_ok());
        assert!(matches!(
            conn.creds,
            Some(LoginResponse{username, lobby_name})
                if username == TEST_USERNAME && lobby_name == TEST_LOBBY_NAME
        ));
        let player_found_in_lobby = conn
            .global_state
            .write()
            .await
            .get_manager()
            .get(TEST_LOBBY_NAME)
            .unwrap()
            .get_player(TEST_USERNAME)
            .is_ok_and(|player| player.get_name() == TEST_USERNAME);
        assert!(player_found_in_lobby)
    }

    #[tokio::test]
    async fn GIVEN_player_conn_WHEN_login_handler_rejoin_THEN_ok() {
        // GIVEN
        let mut conn = create_test_lobby_and_player_conn().await;
        conn.login_loop(1).await.unwrap(); // join the lobby
        conn.leave_lobby().await.unwrap();
        assert!(conn.creds.is_none());
        let player_not_found_in_lobby = conn
            .global_state
            .write()
            .await
            .get_manager()
            .get(TEST_LOBBY_NAME)
            .unwrap()
            .get_player(TEST_USERNAME)
            .is_err();
        assert!(player_not_found_in_lobby);

        // WHEN
        let result = conn.login_loop(1).await; // connection is reused

        // THEN
        assert!(result.is_ok());
        assert!(matches!(
            conn.creds,
            Some(LoginResponse{username, lobby_name})
                if username == TEST_USERNAME && lobby_name == TEST_LOBBY_NAME
        ));
        let player_found_in_lobby = conn
            .global_state
            .write()
            .await
            .get_manager()
            .get(TEST_LOBBY_NAME)
            .unwrap()
            .get_player(TEST_USERNAME)
            .is_ok_and(|player| player.get_name() == TEST_USERNAME);
        assert!(player_found_in_lobby)
    }

    #[tokio::test]
    async fn GIVEN_player_conn_logged_in_WHEN_lobby_dropped_THEN_ok() {
        // GIVEN
        let mut conn = create_test_lobby_and_player_conn().await;
        let global_state = conn.global_state.clone();
        let receiver = conn.login_loop(1).await.unwrap();

        let notify = Arc::new(Notify::new());
        let notifier = notify.clone();
        // create a notify to ensure that the player connection loop gets terminated
        tokio::spawn(async move {
            let result = conn.main(receiver).await;
            assert!(result.is_err_and(|e| {
                matches!(e, PlayerHandlerError::Internal(InternalError::EndOfChannel))
            }));
            notifier.notify_waiters();
        });

        // WHEN
        let lobby = global_state
            .write()
            .await
            .get_mut_manager()
            .remove(TEST_LOBBY_NAME)
            .unwrap();
        drop(lobby);

        // THEN
        timeout(Duration::from_secs(5), async { notify.notified().await })
            .await
            .unwrap();
    }

    // TODO: negative login tests,
}
