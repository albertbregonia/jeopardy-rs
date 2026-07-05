use std::error::Error;

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use stagecrew::conn::{ErrorReason, JsonConn, TextTransport};
use thiserror::Error;

// we consider this file to be part of the "top level" handlers
// as this defines the websocket API for handling players.
// therefore, logging will be done extensively at this level

use crate::{
    game::commands::player::{PlayerCommand, PlayerCommandResponse},
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
}

#[cfg(test)]
#[allow(non_snake_case)]
mod player_conn_tests {

    use stagecrew::conn::json_conn_test_constructs::{MockTextTransport, new_mock_json_conn};
    use tokio::sync::mpsc;

    use crate::{
        game::{commands::player::PlayerCommandResponse, jeopardy::config::JeopardyConfig},
        server::{
            CredsValidatorGeneric, GenericJeopardyServerState, JeopardyServerState, ManagerGeneric,
            TestDefault,
        },
        web::handlers::{
            create_lobby::{CreateLobbyRequest, create_lobby_test_util::new_test_server},
            player::{
                InternalError, LoginError, PlayerConn, PlayerRequest, PlayerResponse, UserError,
            },
        },
    };

    // helper to create a player conn that uses an mpsc to simulate a websocket
    // cannot use TestDefault bc async and we need to return the mpsc handles
    async fn new_test_player_conn<M: ManagerGeneric, C: CredsValidatorGeneric>(
        state: GenericJeopardyServerState<M, C>,
    ) -> (
        PlayerConn<MockTextTransport<PlayerRequest>, M, C>,
        mpsc::Sender<PlayerRequest>,
        mpsc::Receiver<String>,
    ) {
        let (mock_json_conn, input_sender, output_receiver) = new_mock_json_conn();
        let player_conn = PlayerConn::new(state, mock_json_conn);
        (player_conn, input_sender, output_receiver)
    }

    async fn default_server_state_with_lobby() -> JeopardyServerState {
        new_test_server(Some(CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        }))
        .await
    }

    // send_response() tests

    #[tokio::test]
    async fn GIVEN_player_conn_WHEN_send_response_THEN_ok() {
        // GIVEN
        let state = default_server_state_with_lobby().await;
        let (mut player_conn, _, mut output_receiver) = new_test_player_conn(state).await;
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
        let state = default_server_state_with_lobby().await;
        // we drop both the input sender and the output receiver so that the underlying channel fails
        let (mut player_conn, _, _) = new_test_player_conn(state).await;

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
        let state = default_server_state_with_lobby().await;
        let (mut player_conn, _, mut output_receiver) = new_test_player_conn(state).await;
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
        let state = default_server_state_with_lobby().await;
        // we drop both the input sender and the output receiver so that the underlying channel fails
        let (mut player_conn, _, _) = new_test_player_conn(state).await;
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
        let state = default_server_state_with_lobby().await;
        let (player_conn, _, _output_receiver) = new_test_player_conn(state).await;
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
        let state = default_server_state_with_lobby().await;
        // we drop both the input sender and the output receiver so that the underlying channel fails
        let (player_conn, _, _) = new_test_player_conn(state).await;
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
        let state = default_server_state_with_lobby().await;
        let (player_conn, _, _output_receiver) = new_test_player_conn(state).await;
        let internal_error = InternalError::MissingLoginCredentials;

        // WHEN
        let result = player_conn.handle_internal_error(internal_error).await;

        // THEN
        assert!(result.is_ok()); // player conn simply disconnects, `TextTransport` implementation decides what it does with the error
    }

    #[tokio::test]
    async fn GIVEN_disconnected_player_conn_WHEN_handle_internal_error_THEN_error() {
        // GIVEN
        let state = default_server_state_with_lobby().await;
        // we drop both the input sender and the output receiver so that the underlying channel fails
        let (player_conn, _, _) = new_test_player_conn(state).await;
        let internal_error = InternalError::MissingLoginCredentials;

        // WHEN
        let result = player_conn.handle_internal_error(internal_error).await;

        // THEN
        assert!(matches!(result, Err(InternalError::Dependency(..))));
    }
}
