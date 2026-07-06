use std::{error::Error, time::Duration};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use stagecrew::{
    conn::{ErrorReason, JsonConn, TextTransport},
    lobby::LobbyError,
    manager::{ManagerEntry, ManagerError},
};
use thiserror::Error;
use tokio::{sync::mpsc, time::timeout};

// we consider this file to be part of the "top level" handlers
// as this defines the websocket API for handling players.
// therefore, logging will be done extensively at this level

use crate::{
    game::{
        commands::player::{PlayerCommand, PlayerCommandResponse},
        player::{JeopardyPlayer, JeopardyPlayerEvent},
    },
    server::{CredsValidatorGeneric, GenericJeopardyServerState, ManagerGeneric},
    web::handlers::serialize_result,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginCredentials {
    lobby_id: String,
    lobby_password: String,
    username: String,
}

// variants of what can be sent by the player over the websocket
// note: serialize is required only for tests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerRequest {
    Login(LoginCredentials),
    Command(PlayerCommand),
}

// response type for player
#[derive(Debug, Serialize)]
pub struct PlayerResponse {
    #[serde(serialize_with = "serialize_result")]
    pub result: Result<PlayerCommandResponse, String>,
}

// shallow wrapper over JsonConn to interface the user with the game
// implicits:
// - all recoverable user errors are sent back to the user as a plain response
// - all internal server errors / irrecoverable user errors are returned from the function.
//   meaning, a function does not return an error unless it believes it cannot/should not continue
pub struct PlayerConn<T: TextTransport, M: ManagerGeneric, C: CredsValidatorGeneric> {
    state: GenericJeopardyServerState<M, C>,
    json_ws: JsonConn<T, PlayerRequest, PlayerResponse>,
    creds: Option<LoginCredentials>,
}

// top level error type
// at this level, we're essentially now defining what constitutes
// as an expected vs unexpected error. aka is this an issue because
// the user gave some wrong input or do we encounter an issue
// during normal operation and cannot do anything about it
#[derive(Debug, Error)]
pub enum PlayerHandlerError {
    #[error(transparent)]
    User(#[from] UserError),
    #[error(transparent)]
    Internal(#[from] InternalError),
}

// map any error to a string when  sending back to the user.
// it will get serialized to JSON using `serialize_result`
impl<E: Error> From<E> for PlayerResponse {
    fn from(e: E) -> Self {
        Self {
            result: Err(e.to_string()),
        }
    }
}

#[derive(Debug, Error)]
pub enum UserError {
    #[error("User connection timed out waiting for input")]
    RequestTimeout,
    #[error("Unexpected user disconnect")]
    UnexpectedDisconnect,
    #[error(transparent)]
    Login(#[from] LoginError),
    #[error("Unexpected Request Type: {0:?}")]
    UnexpectedRequestType(PlayerRequest),
}

#[derive(Debug, Error)]
pub enum InternalError {
    #[error("Attempted to perform an operation that requires the player to be logged in.")]
    MissingLoginCredentials,
    #[error("Unexpected response type: {0}")]
    UnexpectedResponse(anyhow::Error),
    #[error(transparent)]
    Dependency(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum LoginError {
    #[error("Invalid format for login credentials")]
    InvalidLoginCredentialsFormat,
    #[error("User exceeded login attempt limit")]
    ExceededAttemptLimit,
    #[error("Incorrect password for the desired lobby")]
    IncorrectLobbyPassword,
    #[error("Requested lobby not found.")]
    LobbyNotFound,
    #[error("Username is already taken")]
    UsernameAlreadyTaken,
}

impl<T, M, C> PlayerConn<T, M, C>
where
    T: TextTransport,
    M: ManagerGeneric,
    C: CredsValidatorGeneric,
{
    pub fn new(
        state: GenericJeopardyServerState<M, C>,
        json_ws: JsonConn<T, PlayerRequest, PlayerResponse>,
    ) -> Self {
        Self {
            state,
            json_ws,
            creds: None, // cache lobby ID and username to refer to later
        }
    }

    // helpers to send responses and errors to the player over the `JsonConn`

    async fn send_response(&mut self, value: PlayerCommandResponse) -> Result<(), InternalError> {
        self.json_ws
            .send_json(&PlayerResponse { result: Ok(value) })
            .await
            .map_err(|e| InternalError::Dependency(anyhow!("Failed to send response: {e}")))
    }

    async fn send_recoverable_user_error(&mut self, e: UserError) -> Result<(), InternalError> {
        self.json_ws
            .send_json(&e.into())
            .await
            .map_err(|e| InternalError::Dependency(anyhow!("Failed to send user error: {e}")))
    }

    // NOTE: this function consumes the connection
    pub async fn handle_irrecoverable_user_error(self, e: UserError) -> Result<(), InternalError> {
        self.json_ws
            .disconnect(Some(ErrorReason {
                internal_error: false,
                reason: e.to_string(),
            }))
            .await
            .map_err(|e| {
                InternalError::Dependency(anyhow!(
                    "Failed to disconnect when handling fatal user error: {e}"
                ))
            })
    }

    // NOTE: this function consumes the connection
    pub async fn handle_internal_error(self, e: InternalError) -> Result<(), InternalError> {
        self.json_ws
            .disconnect(Some(ErrorReason {
                internal_error: true,
                reason: e.to_string(),
            }))
            .await
            .map_err(|e| {
                InternalError::Dependency(anyhow!(
                    "Failed to disconnect when handling internal server error: {e}"
                ))
            })
    }

    async fn read_request_with_timeout(
        &mut self,
        max_timeout: Duration,
    ) -> Result<PlayerRequest, PlayerHandlerError> {
        let request = timeout(max_timeout, self.json_ws.read_json())
            .await
            .map_err(|_| UserError::RequestTimeout)?
            .ok_or(UserError::UnexpectedDisconnect)?
            .map_err(|e| InternalError::Dependency(anyhow!("Failed to read request: {e}")))?;
        Ok(request)
    }

    // public API

    /// given login credentials, creates a player and adds them to their desired lobby.
    /// returns the mpsc::Receiver handle to receive messages from the lobby
    pub async fn join_lobby(
        &mut self,
        creds: LoginCredentials,
    ) -> Result<mpsc::Receiver<JeopardyPlayerEvent>, PlayerHandlerError> {
        let LoginCredentials {
            lobby_id,
            lobby_password,
            username,
        } = &creds;
        tracing::info!("Attempt to join lobby ID: '{lobby_id}' with username: '{username}'");

        // get lobby
        let manager_wg = self.state.manager().read().await;
        let entry = manager_wg.get(lobby_id).map_err(|e| match e {
            ManagerError::EntryNotFound(_) => {
                tracing::warn!("Cannot log in to lobby that does not exist");
                PlayerHandlerError::User(UserError::Login(LoginError::LobbyNotFound))
            }
            other => {
                tracing::error!("Unexpected manager error during join lobby: {other}");
                PlayerHandlerError::Internal(InternalError::Dependency(other.into()))
            }
        })?;
        tracing::info!("Lobby ID: '{lobby_id}' exists.");

        // auth
        if !entry.is_correct_password(lobby_password) {
            tracing::warn!("Incorrect lobby password.");
            return Err(PlayerHandlerError::User(UserError::Login(
                LoginError::IncorrectLobbyPassword,
            )));
        }
        tracing::info!("Successful authZ for '{username}' and lobby ID: '{lobby_id}'");

        // create player
        // create channel to communicate with the frontend
        // mpsc bc i need direct communication with a specific player if they request for their points, etc.
        // NOTE: lowk wasteful because the call may fail but since the player structs are cheap this is fine
        let buffer_size = self.state.config().player_channel_buffer_size;
        let (writer, receiver) = mpsc::channel(buffer_size);
        let player = JeopardyPlayer::new(username.clone(), writer);
        tracing::info!("Successfully created player object for '{username}'");

        // add to lobby
        entry
            .lobby()
            .add_player(username, player)
            .await
            .map_err(|e| match e {
                LobbyError::PlayerIDConflict(_) => {
                    tracing::warn!("Username conflict for player");
                    PlayerHandlerError::User(UserError::Login(LoginError::UsernameAlreadyTaken))
                }
                other => {
                    tracing::error!("Unexpected error during add player to lobby: {other}");
                    PlayerHandlerError::Internal(InternalError::Dependency(other.into()))
                }
            })?;

        tracing::info!("Successful login for '{username}' to '{lobby_id}'.");
        self.creds = Some(creds);
        Ok(receiver)
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod player_conn_tests {

    use std::time::Duration;

    use stagecrew::{
        conn::json_conn_test_constructs::{MockTextTransport, new_mock_json_conn},
        manager::{Manager, ManagerEntry},
    };
    use tokio::sync::mpsc;

    use crate::{
        game::{commands::player::PlayerCommandResponse, jeopardy::config::JeopardyConfig},
        server::{CredsValidatorGeneric, GenericJeopardyServerState, ManagerGeneric, TestDefault},
        web::handlers::{
            create_lobby::{
                CreateLobbyRequest,
                create_lobby_test_util::{
                    new_test_server, new_test_server_with_player, new_test_server_with_test_manager,
                },
            },
            player::{
                InternalError, LoginCredentials, LoginError, PlayerConn, PlayerHandlerError,
                PlayerRequest, PlayerResponse, UserError,
            },
        },
    };

    // helper to create a player conn that uses an mpsc to simulate a websocket
    // cannot use TestDefault bc async and we need to return the mpsc handles
    async fn new_test_player_conn<M: ManagerGeneric, C: CredsValidatorGeneric>(
        state: GenericJeopardyServerState<M, C>,
        fail_during_read_text: bool,
    ) -> (
        PlayerConn<MockTextTransport<PlayerRequest>, M, C>,
        mpsc::Sender<PlayerRequest>,
        mpsc::Receiver<String>,
    ) {
        let (mock_json_conn, input_sender, output_receiver) =
            new_mock_json_conn(fail_during_read_text);
        let player_conn = PlayerConn::new(state, mock_json_conn);
        (player_conn, input_sender, output_receiver)
    }

    // helper function to check if a lobby has a player given the respective IDs
    // (mostly made bc writing out this call everywhere is long)
    async fn lobby_has_player<M: ManagerGeneric, C: CredsValidatorGeneric>(
        state: &GenericJeopardyServerState<M, C>,
        lobby_id: &str,
        player_id: &str,
    ) -> bool {
        state
            .manager()
            .read()
            .await
            .get(lobby_id)
            .unwrap()
            .lobby()
            .has_player(player_id)
            .await
            .unwrap()
    }
    // send_response() tests

    #[tokio::test]
    async fn GIVEN_player_conn_WHEN_send_response_THEN_ok() {
        // GIVEN
        let state = new_test_server(None).await;
        let (mut player_conn, _, mut output_receiver) = new_test_player_conn(state, false).await;
        let response = PlayerCommandResponse::Success;

        // WHEN
        player_conn.send_response(response.clone()).await.unwrap();

        // THEN
        let raw_msg = output_receiver.recv().await.unwrap();
        let expected = serde_json::to_string(&PlayerResponse {
            result: Ok(response),
        })
        .unwrap();
        assert_eq!(raw_msg, expected);
    }

    #[tokio::test]
    async fn GIVEN_disconnected_player_conn_WHEN_send_response_THEN_error() {
        // GIVEN
        let state = new_test_server(None).await;
        // we drop both the input sender and the output receiver so that the underlying channel fails
        let (mut player_conn, _, _) = new_test_player_conn(state, false).await;

        // WHEN
        let result = player_conn
            .send_response(PlayerCommandResponse::Success)
            .await;

        // THEN
        assert!(matches!(result, Err(InternalError::Dependency(..))));
    }

    // send_recoverable_user_error() tests

    #[tokio::test]
    async fn GIVEN_player_conn_WHEN_send_recoverable_user_error_THEN_ok() {
        // GIVEN
        let state = new_test_server(None).await;
        let (mut player_conn, _, mut output_receiver) = new_test_player_conn(state, false).await;
        let user_error = UserError::Login(LoginError::IncorrectLobbyPassword); // realistic error - we don't want to kill their connection for a typo
        let error_msg = user_error.to_string();

        // WHEN
        player_conn
            .send_recoverable_user_error(user_error)
            .await
            .unwrap();

        // THEN
        let raw_msg = output_receiver.recv().await.unwrap();
        let expected = serde_json::to_string(&PlayerResponse {
            result: Err(error_msg),
        })
        .unwrap();
        assert_eq!(raw_msg, expected);
    }

    #[tokio::test]
    async fn GIVEN_disconnected_player_conn_WHEN_send_recoverable_user_error_THEN_error() {
        // GIVEN
        let state = new_test_server(None).await;
        // we drop both the input sender and the output receiver so that the underlying channel fails
        let (mut player_conn, _, _) = new_test_player_conn(state, false).await;
        let user_error = UserError::Login(LoginError::IncorrectLobbyPassword);

        // WHEN
        let result = player_conn.send_recoverable_user_error(user_error).await;

        // THEN
        assert!(matches!(result, Err(InternalError::Dependency(..))));
    }

    // handle_irrecoverable_user_error() tests

    #[tokio::test]
    async fn GIVEN_player_conn_WHEN_send_irrecoverable_user_error_THEN_ok() {
        // GIVEN
        let state = new_test_server(None).await;
        let (player_conn, _, _output_receiver) = new_test_player_conn(state, false).await;
        let user_error = UserError::RequestTimeout; // realistic error - if soft lock, we want to kill the connection

        // WHEN
        let result = player_conn
            .handle_irrecoverable_user_error(user_error)
            .await;

        // THEN
        assert!(result.is_ok()); // player conn simply disconnects, `TextTransport` implementation decides what it does with the error
    }

    #[tokio::test]
    async fn GIVEN_disconnected_player_conn_WHEN_send_irrecoverable_user_error_THEN_error() {
        // GIVEN
        let state = new_test_server(None).await;
        // we drop both the input sender and the output receiver so that the underlying channel fails
        let (player_conn, _, _) = new_test_player_conn(state, false).await;
        let user_error = UserError::RequestTimeout; // realistic error - if soft lock, we want to kill the connection

        // WHEN
        let result = player_conn
            .handle_irrecoverable_user_error(user_error)
            .await;

        // THEN
        assert!(matches!(result, Err(InternalError::Dependency(..))));
    }

    // handle_internal_server_error() tests

    #[tokio::test]
    async fn GIVEN_player_conn_WHEN_handle_internal_error_THEN_ok() {
        // GIVEN
        let state = new_test_server(None).await;
        let (player_conn, _, _output_receiver) = new_test_player_conn(state, false).await;
        let internal_error = InternalError::MissingLoginCredentials;

        // WHEN
        let result = player_conn.handle_internal_error(internal_error).await;

        // THEN
        assert!(result.is_ok()); // player conn simply disconnects, `TextTransport` implementation decides what it does with the error
    }

    #[tokio::test]
    async fn GIVEN_disconnected_player_conn_WHEN_handle_internal_error_THEN_error() {
        // GIVEN
        let state = new_test_server(None).await;
        // we drop both the input sender and the output receiver so that the underlying channel fails
        let (player_conn, _, _) = new_test_player_conn(state, false).await;
        let internal_error = InternalError::MissingLoginCredentials;

        // WHEN
        let result = player_conn.handle_internal_error(internal_error).await;

        // THEN
        assert!(matches!(result, Err(InternalError::Dependency(..))));
    }

    // read_request_with_timeout() tests

    #[tokio::test]
    async fn GIVEN_no_timeout_player_conn_WHEN_read_request_with_timeout_THEN_ok() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;
        let (mut player_conn, input_sender, _) = new_test_player_conn(state, false).await;
        let login_request = LoginCredentials {
            // derive login request from create lobby request
            lobby_id: create_lobby_request.lobby_name,
            lobby_password: create_lobby_request.lobby_password,
            username: "username".to_string(),
        };
        input_sender // send valid request to be read
            .send(PlayerRequest::Login(login_request.clone()))
            .await
            .unwrap();

        // WHEN
        let request = player_conn
            .read_request_with_timeout(Duration::from_secs(1))
            .await
            .unwrap();

        // THEN
        let PlayerRequest::Login(LoginCredentials {
            lobby_id,
            lobby_password,
            username,
        }) = request
        else {
            panic!("Unexpected PlayerRequest variant: {request:?}");
        };
        assert_eq!(lobby_id, login_request.lobby_id);
        assert_eq!(lobby_password, login_request.lobby_password);
        assert_eq!(username, login_request.username);
    }

    #[tokio::test]
    async fn GIVEN_timeout_player_conn_WHEN_read_request_with_timeout_THEN_error() {
        // GIVEN
        let state = new_test_server(None).await;
        // we don't drop the input sender so that we have a valid connection but player_conn times out bc we don't send anything
        let (mut player_conn, _input_sender, _) = new_test_player_conn(state, false).await;

        // WHEN
        let result = player_conn
            .read_request_with_timeout(Duration::from_secs(1))
            .await;

        // THEN
        assert!(matches!(
            result,
            Err(PlayerHandlerError::User(UserError::RequestTimeout))
        ))
    }

    #[tokio::test]
    async fn GIVEN_disconnected_player_conn_WHEN_read_request_with_timeout_THEN_error() {
        // GIVEN
        let state = new_test_server(None).await;
        // drop both input sender and output receiver so that the underlying read_json() errors
        let (mut player_conn, _, _) = new_test_player_conn(state, false).await;

        // WHEN
        let result = player_conn
            .read_request_with_timeout(Duration::from_secs(1))
            .await;

        // THEN
        assert!(matches!(
            result,
            Err(PlayerHandlerError::User(UserError::UnexpectedDisconnect))
        ));
    }

    #[tokio::test]
    async fn GIVEN_json_conn_error_WHEN_read_request_with_timeout_THEN_error() {
        // GIVEN
        let state = new_test_server(None).await;
        // set `fail_during_read_text` to true to cascade the error (TextTransport errors => JsonConn errors => PlayerConn errors)
        let (mut player_conn, _, _) = new_test_player_conn(state, true).await;

        // WHEN
        let result = player_conn
            .read_request_with_timeout(Duration::from_secs(1))
            .await;

        // THEN
        assert!(matches!(
            result,
            Err(PlayerHandlerError::Internal(InternalError::Dependency(..)))
        ));
    }

    // join_lobby() tests

    #[tokio::test]
    async fn GIVEN_valid_creds_WHEN_join_lobby_THEN_ok() {
        // GIVEN
        let lobby_name = "lobby_name";
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: lobby_name.to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;
        let (mut player_conn, _, _) = new_test_player_conn(state.clone(), false).await;
        let username = "username";
        let login_request = LoginCredentials {
            // derive login request from create lobby request
            lobby_id: create_lobby_request.lobby_name,
            lobby_password: create_lobby_request.lobby_password,
            username: username.to_string(),
        };

        // WHEN
        let _receiver = player_conn.join_lobby(login_request.clone()).await.unwrap();

        // THEN
        assert!(lobby_has_player(&state, lobby_name, username).await);
        // cached creds match
        let player_creds = player_conn.creds.unwrap();
        assert_eq!(player_creds.lobby_id, login_request.lobby_id);
        assert_eq!(player_creds.lobby_password, login_request.lobby_password);
        assert_eq!(player_creds.username, login_request.username);
    }

    #[tokio::test]
    async fn GIVEN_invalid_lobby_WHEN_join_lobby_THEN_error() {
        // GIVEN
        let state = new_test_server(None).await;
        let (mut player_conn, _, _) = new_test_player_conn(state.clone(), false).await;
        let invalid_lobby_name = "INVALID"; // no lobbies exist so this is invalid

        // WHEN
        let result = player_conn
            .join_lobby(LoginCredentials {
                lobby_id: invalid_lobby_name.to_string(),
                lobby_password: "lobby_password".to_string(), // password doesn't matter, won't get checked
                username: "username".to_string(),
            })
            .await;

        // THEN
        assert!(matches!(
            result,
            Err(PlayerHandlerError::User(UserError::Login(
                LoginError::LobbyNotFound
            )))
        ));
        // can't check has_player() for lobby that doesn't exist
        assert!(player_conn.creds.is_none());
    }

    #[tokio::test]
    async fn GIVEN_invalid_lobby_password_WHEN_join_lobby_THEN_error() {
        // GIVEN
        let lobby_name = "lobby_name";
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: lobby_name.to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;
        let (mut player_conn, _, _) = new_test_player_conn(state.clone(), false).await;
        let username = "username";
        let login_request = LoginCredentials {
            lobby_id: create_lobby_request.lobby_name,
            lobby_password: "INCORRECT".to_string(), // incorrect password
            username: username.to_string(),
        };

        // WHEN
        let result = player_conn.join_lobby(login_request).await;

        // THEN
        assert!(matches!(
            result,
            Err(PlayerHandlerError::User(UserError::Login(
                LoginError::IncorrectLobbyPassword
            )))
        ));
        assert_eq!(false, lobby_has_player(&state, lobby_name, username).await); // not added
        assert!(player_conn.creds.is_none());
    }

    #[tokio::test]
    async fn GIVEN_username_conflict_WHEN_join_lobby_THEN_error() {
        // GIVEN
        let lobby_name = "lobby_name";
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: lobby_name.to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let username = "username";
        let (state, _) = new_test_server_with_player(create_lobby_request.clone(), username).await;
        let (mut player_conn, _, _) = new_test_player_conn(state.clone(), false).await;
        let login_request = LoginCredentials {
            lobby_id: create_lobby_request.lobby_name,
            lobby_password: create_lobby_request.lobby_password,
            username: username.to_string(), // reuse the username so it conflicts
        };

        // WHEN
        let result = player_conn.join_lobby(login_request).await;

        // THEN
        assert!(matches!(
            result,
            Err(PlayerHandlerError::User(UserError::Login(
                LoginError::UsernameAlreadyTaken
            )))
        ));
        // player not added guaranteed by lobby.add_player() call
        assert!(player_conn.creds.is_none());
    }

    #[tokio::test]
    async fn GIVEN_failing_manager_WHEN_join_lobby_THEN_error() {
        // GIVEN
        let lobby_name = "lobby_name";
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: lobby_name.to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server_with_test_manager(Some(create_lobby_request.clone())).await;
        state.manager().write().await.set_always_fail(); // lobby lookup should fail
        let (mut player_conn, _, _) = new_test_player_conn(state.clone(), false).await;
        let username = "username";
        let login_request = LoginCredentials {
            lobby_id: create_lobby_request.lobby_name,
            lobby_password: create_lobby_request.lobby_password,
            username: username.to_string(),
        };

        // WHEN
        let result = player_conn.join_lobby(login_request).await;

        // THEN
        state.manager().write().await.set_never_fail();
        assert!(matches!(
            result,
            Err(PlayerHandlerError::Internal(InternalError::Dependency(..)))
        ));
        assert_eq!(false, lobby_has_player(&state, lobby_name, username).await); // not added
        assert!(player_conn.creds.is_none());
    }

    #[tokio::test]
    async fn GIVEN_shutdown_lobby_WHEN_join_lobby_THEN_error() {
        // GIVEN
        let lobby_name = "lobby_name";
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: lobby_name.to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;
        let (mut player_conn, _, _) = new_test_player_conn(state.clone(), false).await;
        let username = "username";
        let login_request = LoginCredentials {
            lobby_id: create_lobby_request.lobby_name,
            lobby_password: create_lobby_request.lobby_password,
            username: username.to_string(),
        };

        // preconditions
        let manager_rg = state.manager().read().await;
        let lobby = manager_rg.get(lobby_name).unwrap().lobby();
        let shutdown_handle = lobby.shutdown().await.unwrap();
        shutdown_handle.await.unwrap();
        assert!(lobby.is_shutdown()); // ensure lobby is shut down so add fails
        drop(manager_rg); // drop mutex or else this hangs

        // WHEN
        let result = player_conn.join_lobby(login_request).await;

        // THEN
        assert!(matches!(
            result,
            Err(PlayerHandlerError::Internal(InternalError::Dependency(..)))
        ));
        // can't check has_player() bc lobby is shut down
        assert!(player_conn.creds.is_none());
    }
}
