use std::{error::Error, time::Duration};

use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use stagecrew::{
    conn::{ErrorReason, JsonConn, TextTransport},
    lobby::{Lobby, LobbyError},
    manager::{ManagerEntry, ManagerError},
};
use thiserror::Error;
use tokio::{sync::mpsc, time::timeout};

// we consider this file to be part of the "top level" handlers
// as this defines the websocket API for handling players.
// therefore, logging will be done extensively at this level

use crate::{
    game::{
        Jeopardy, JeopardyCommand, JeopardyCommandResponse, JeopardyError,
        commands::player::{PlayerCommand, PlayerCommandResponse},
        player::{JeopardyPlayer, JeopardyPlayerEvent},
    },
    server::{CredsValidatorGeneric, JeopardyServerStateGeneric, ManagerGeneric},
    web::handlers::serialize_result,
};

/// Helper struct to encapsulate creds for a login request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginCredentials {
    pub lobby_id: String,
    pub lobby_password: String,
    pub username: String,
}

/// Helper struct to encapsulate former player data after leaving a lobby.
/// Useful for updating persistent player stats and handling lobby cleanup
#[derive(Debug)]
pub struct LeaveCredentials {
    pub player: JeopardyPlayer,
    pub former_creds: LoginCredentials,
    pub lobby_ref: Lobby<Jeopardy>,
}

fn is_valid_login_request(
    validator: &impl CredsValidatorGeneric,
    creds: &LoginCredentials,
) -> bool {
    let lobby_id_ok = validator.is_valid_lobby_id(&creds.lobby_id);
    let lobby_pw_ok = validator.is_valid_lobby_password(&creds.lobby_password);
    let username_ok = validator.is_valid_username(&creds.username);
    tracing::info!(
        "Valid lobby ID?: {lobby_id_ok} | lobby password?: {lobby_pw_ok} | username?: {username_ok}"
    );
    lobby_id_ok && lobby_pw_ok && username_ok
}

// variants of what can be sent by the player over the websocket
// note: serialize is required only for tests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlayerRequest {
    Login(LoginCredentials),
    Command(PlayerCommand),
}

// response type for player
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
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
    state: JeopardyServerStateGeneric<M, C>,
    json_ws: JsonConn<T, PlayerRequest, PlayerResponse>,
    lobby: Option<Lobby<Jeopardy>>,
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
    ActivityTimeout,
    #[error("Unexpected user disconnect")]
    Disconnected,
    #[error(transparent)]
    Login(#[from] LoginError),
    #[error(transparent)]
    Game(#[from] JeopardyError),
    #[error("Unexpected Request Type: {0:?}")]
    UnexpectedRequestType(PlayerRequest),
}

#[derive(Debug, Error)]
pub enum InternalError {
    #[error("Inactive lobby ID: '{0}' still present in manager")]
    InactiveLobby(String),
    #[error("Attempted to perform an operation that requires the player to be logged in.")]
    NotLoggedIn,
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
        state: JeopardyServerStateGeneric<M, C>,
        json_ws: JsonConn<T, PlayerRequest, PlayerResponse>,
    ) -> Self {
        Self {
            state,
            json_ws,
            lobby: None,
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
    pub async fn handle_internal_error(self, _e: InternalError) -> Result<(), InternalError> {
        self.json_ws
            .disconnect(Some(ErrorReason {
                internal_error: true,
                reason: "Internal Server Error".to_string(),
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
            .map_err(|_| UserError::ActivityTimeout)?
            .ok_or(UserError::Disconnected)?
            .map_err(|e| InternalError::Dependency(anyhow!("Failed to read request: {e}")))?;
        Ok(request)
    }

    // public API

    /// given login credentials, creates a player and adds them to their desired lobby.
    /// returns the mpsc::Receiver handle to receive messages from the lobby
    pub async fn join_lobby(
        &mut self,
        mut creds: LoginCredentials,
    ) -> Result<mpsc::Receiver<JeopardyPlayerEvent>, PlayerHandlerError> {
        let LoginCredentials {
            lobby_id,
            lobby_password,
            username,
        } = &creds;
        let join_span = tracing::info_span!("join_lobby", lobby_id = lobby_id, username = username);
        let _entered = join_span.enter();

        // get lobby
        let manager = self.state.manager().read().await;
        let entry = manager.get(lobby_id).map_err(|e| match e {
            ManagerError::EntryNotFound(_) => {
                tracing::warn!("Cannot log in to lobby that does not exist");
                PlayerHandlerError::User(UserError::Login(LoginError::LobbyNotFound))
            }
            other => {
                tracing::error!("Unexpected manager error during join lobby: {other}");
                PlayerHandlerError::Internal(InternalError::Dependency(other.into()))
            }
        })?;
        tracing::info!("Requested lobby exists");

        // auth
        if !entry.is_correct_password(lobby_password) {
            tracing::warn!("Incorrect lobby password");
            return Err(PlayerHandlerError::User(UserError::Login(
                LoginError::IncorrectLobbyPassword,
            )));
        }
        tracing::info!("Successful authZ");

        // create player
        // create channel to communicate with the frontend
        // mpsc bc i need direct communication with a specific player if they request for their points, etc.
        // NOTE: lowk wasteful because the call may fail but since the player structs are cheap this is fine
        let buffer_size = self.state.config().player_channel_buffer_size;
        let (sender, receiver) = mpsc::channel(buffer_size);
        let player = JeopardyPlayer::new(username.clone(), 0, sender);
        tracing::info!("Successfully created player object");

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
                LobbyError::ActorShutdown => {
                    // we should maintain the invariant that a lobby in the manager is always an active one
                    // however, the compiler cannot guarantee that. therefore we still handle it
                    tracing::error!("Attempted to join a lobby that was shutdown");
                    PlayerHandlerError::Internal(InternalError::InactiveLobby(lobby_id.clone()))
                }
                other => {
                    // cant test this but that's expected
                    tracing::error!("Unexpected error during add player to lobby: {other}");
                    PlayerHandlerError::Internal(InternalError::UnexpectedResponse(other.into()))
                }
            })?;

        tracing::info!("Successful login for '{username}' to '{lobby_id}'.");
        creds.lobby_password = String::new(); // delete password, we don't want to store it
        self.lobby = Some(entry.lobby().clone()); // clone the handle
        self.creds = Some(creds);
        Ok(receiver)
    }

    /// attempts to remove a player from their lobby using their saved login creds (self.creds)
    /// the following are not considered failures, but moreso no-ops
    /// - if the lobby is already deleted, this simply returns Ok(None)
    /// - if the player was not part of the lobby (somehow) or the lobby exists but was shutdown returns Ok(None)
    ///
    /// possible failures:
    /// - InternalError::UserNotLoggedIn, attempted to leave a lobby without a previous `join_lobby(..)` call (self.creds == None)
    /// - any LobbyError other than ActorShutdown after calling remove_player()
    pub async fn leave_lobby(&mut self) -> Result<LeaveCredentials, InternalError> {
        let lobby_ref = self.lobby.take().ok_or(InternalError::NotLoggedIn)?;
        let former_creds = self.creds.take().ok_or(InternalError::NotLoggedIn)?;
        let leave_span = tracing::info_span!(
            "leave_lobby",
            lobby_id = former_creds.lobby_id,
            username = former_creds.username
        );
        let _entered = leave_span.enter();

        // remove player from lobby
        let player = lobby_ref
            .remove_player(&former_creds.username)
            .await
            .map_err(|e| match e {
                LobbyError::ActorShutdown => {
                    tracing::error!(
                        "Failed to remove player from lobby. Lobby is already shut down"
                    );
                    InternalError::InactiveLobby(former_creds.lobby_id.clone())
                }
                other => {
                    // not tested but expected
                    tracing::error!("Unexpected error during remove player: {other}");
                    InternalError::UnexpectedResponse(other.into())
                }
            })?;
        tracing::info!("Successful leave lobby");
        Ok(LeaveCredentials {
            player,
            former_creds,
            lobby_ref,
        })
    }

    /// attempts to call `join_lobby(..)` `max_attempts` times
    /// sending the user error message over the websocket when unsuccessful
    ///
    /// possible failures:
    /// - max user error count (LoginError::ExceededAttemptLimit)
    /// - timeout
    /// - UserError::Disconnected
    /// - non-login request received (UserError::UnexpectedRequestType)
    /// - request validation
    /// - errors listed from join_lobby()
    pub async fn login(
        &mut self,
        max_attempts: usize,
        timeout: Duration,
    ) -> Result<mpsc::Receiver<JeopardyPlayerEvent>, PlayerHandlerError> {
        let login_span = tracing::info_span!("login", max_attempts=max_attempts, timeout=?timeout);
        let _entered = login_span.enter();
        for attempt in 1..=max_attempts {
            // read request from frontend
            let request = self.read_request_with_timeout(timeout).await?;
            // ensure request is of type login
            let PlayerRequest::Login(creds) = request else {
                tracing::warn!("Received non-login request during login: {request:?}");
                return Err(PlayerHandlerError::User(UserError::UnexpectedRequestType(
                    request,
                )));
            };
            // validation
            if !is_valid_login_request(self.state.validator(), &creds) {
                tracing::warn!("Invalid format for login credentials: {creds:?}");
                return Err(PlayerHandlerError::User(UserError::Login(
                    LoginError::InvalidLoginCredentialsFormat,
                )));
            }
            // attempt to join lobby based on request
            match self.join_lobby(creds).await {
                Ok(receiver) => return Ok(receiver),
                Err(e) => match e {
                    PlayerHandlerError::User(e) => {
                        // if we get a user error, inform the user and try again
                        tracing::warn!("Attempt #{attempt}: User error during login: {e}");
                        self.send_recoverable_user_error(e).await?
                    }
                    other => {
                        tracing::warn!("Propagating error from join lobby: {other}");
                        return Err(other);
                    }
                },
            }
        }
        tracing::warn!("Exceeded login attempt limit");
        Err(PlayerHandlerError::User(UserError::Login(
            LoginError::ExceededAttemptLimit,
        )))
    }

    async fn handle_player_command(
        &mut self,
        command: PlayerCommand,
    ) -> Result<(), PlayerHandlerError> {
        let LoginCredentials {
            lobby_id, username, ..
        } = self.creds.as_ref().ok_or(InternalError::NotLoggedIn)?;
        let lobby = self.lobby.as_ref().ok_or(InternalError::NotLoggedIn)?;

        let cmd_span = tracing::info_span!("handle_player_command", lobby_id=lobby_id, username=username, command=?command);
        let _entered = cmd_span.enter();

        // send to lobby
        let game_response = lobby
            .send_game_event_and_wait(JeopardyCommand::Player {
                player_id: username.to_string(),
                command,
            })
            .await
            .map_err(|e| match e {
                LobbyError::ActorShutdown => InternalError::InactiveLobby(lobby_id.clone()),
                other => InternalError::UnexpectedResponse(anyhow!(
                    "Unexpected error during send game event to lobby: {other}"
                )),
            })
            .inspect_err(|e| tracing::error!("Failed to route command to lobby: {e}"))?;

        // handle response
        match game_response {
            Ok(response) => match response {
                JeopardyCommandResponse::Player(response) => {
                    tracing::info!("Successfully obtained player response: {response:?}");
                    self.send_response(response).await.inspect_err(|e| {
                        tracing::error!("Failed to send response to player: {e}")
                    })?;
                }
                other => {
                    // can't test this but that's expected
                    let error_msg = format!(
                        "Received non-player response for during player command handling: {other:?}"
                    );
                    tracing::error!(error_msg);
                    return Err(PlayerHandlerError::Internal(
                        InternalError::UnexpectedResponse(anyhow!(error_msg)),
                    ));
                }
            },
            Err(jeopardy_error) => {
                tracing::warn!("Player command returned a game error: {jeopardy_error}");
                self.send_recoverable_user_error(jeopardy_error.into())
                    .await
                    .map_err(|e| InternalError::Dependency(e.into()))
                    .inspect_err(|e| tracing::error!("Failed to send game error to player: {e}"))?;
            }
        }
        Ok(())
    }

    pub async fn main(
        &mut self,
        mut event_receiver: mpsc::Receiver<JeopardyPlayerEvent>,
        activity_timeout: Duration,
    ) -> Result<(), PlayerHandlerError> {
        let main_span = tracing::info_span!("main", activity_timeout=?activity_timeout);
        let _entered = main_span.enter();
        tracing::info!("Starting main player loop");

        loop {
            // infinitely handle incoming player commands / game events
            // if the lobby doesn't send anything for a while / if the player hasn't sent anything in a while
            // we consider that as an inactive connection and forcefully free those resources
            // this may cascade into the lobby getting freed (when 0 players are in the lobby)
            let connected = timeout(activity_timeout, async {
                tokio::select! {
                    result = self.json_ws.read_json() => match result {
                        Some(result) => match result {
                            Ok(request) => {
                                let PlayerRequest::Command(cmd) = request else {
                                    tracing::warn!("Received non-command request when expected: {request:?}");
                                    return Err(PlayerHandlerError::User(UserError::UnexpectedRequestType(request)));
                                };
                                tracing::info!("Handling player command: {cmd:#?}");
                                self.handle_player_command(cmd).await?;
                                tracing::info!("Command response sent");
                            }
                            Err(e) => {
                                let error_msg = format!("Unexpected connection failure: {e}");
                                tracing::warn!(error_msg);
                                return Err(PlayerHandlerError::Internal(InternalError::Dependency(anyhow!(error_msg))));
                            }
                        }
                        None => {
                            tracing::info!("Lost connection to JsonConn");
                            return Ok(false);
                        }
                    },
                    // internal send to player handler
                    result = event_receiver.recv() => match result {
                        Some(event) => {
                            tracing::info!("Received Jeopardy game event from lobby");
                            let response = match event {
                                JeopardyPlayerEvent::Display(jeopardy_display) =>
                                    PlayerCommandResponse::Refresh(jeopardy_display),
                                JeopardyPlayerEvent::PointsUpdate(points) =>
                                    PlayerCommandResponse::GetPoints(points),
                            };
                            tracing::info!("Game event mapped to response: {response:#?}");
                            self.send_response(response).await?;
                            tracing::info!("Game event broadcasted");
                        }
                        None => {
                            tracing::info!("Lost connection to lobby.");
                            return Ok(false); // clean ws disconnect
                        }
                    }
                }
                Ok(true)
            }).await.map_err(|_| UserError::ActivityTimeout)??;
            if !connected {
                break;
            }
        }
        tracing::info!("Clean disconnect");
        Ok(())
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod player_conn_tests {
    use stagecrew::{
        conn::json_conn_test_constructs::{MockTextTransport, new_test_json_conn},
        manager::{MapManager, PasswordProtectedLobby, test_manager_constructs::TestManager},
        player::Player,
    };
    use std::assert_matches;
    use std::time::Duration;
    use tokio::sync::mpsc;

    use crate::{
        game::{
            Jeopardy, JeopardyError,
            commands::{
                host::HostCommand,
                player::{JeopardyDisplayState, PlayerCommand, PlayerCommandResponse, TextCard},
            },
            jeopardy::config::JeopardyConfig,
            player::{JeopardyPlayerError, JeopardyPlayerEvent},
        },
        server::{
            CredsValidatorGeneric, JeopardyServerState, JeopardyServerStateGeneric, ManagerGeneric,
            TestDefault,
        },
        web::handlers::{
            create_lobby::CreateLobbyRequest,
            player::{
                InternalError, LeaveCredentials, LoginCredentials, LoginError, PlayerConn,
                PlayerHandlerError, PlayerRequest, PlayerResponse, UserError,
            },
            test_util::{
                TestManagerServerState, lobby_has_player, new_test_manager_server_state,
                new_test_server_state, new_test_server_state_with_player,
                send_host_command_for_lobby, shutdown_lobby,
            },
            validators::nonzero_ascii::NonZeroAsciiValidator,
        },
    };

    // helper to create a player conn that uses an mpsc to simulate a websocket
    // cannot use TestDefault bc async and we need to return the mpsc handles
    fn new_test_player_conn<M: ManagerGeneric, C: CredsValidatorGeneric>(
        state: JeopardyServerStateGeneric<M, C>,
        fail_during_read_text: bool,
        buffer_size: usize,
    ) -> (
        PlayerConn<MockTextTransport<PlayerRequest>, M, C>,
        mpsc::Sender<PlayerRequest>,
        mpsc::Receiver<String>,
    ) {
        let (mock_json_conn, input_sender, output_receiver) =
            new_test_json_conn(fail_during_read_text, buffer_size);
        let player_conn = PlayerConn::new(state, mock_json_conn);
        (player_conn, input_sender, output_receiver)
    }

    // send_response() tests

    #[tokio::test]
    async fn GIVEN_player_conn_WHEN_send_response_THEN_ok() {
        // GIVEN
        let state = new_test_server_state(None).await;
        let (mut player_conn, _, mut output_receiver) = new_test_player_conn(state, false, 1);
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
        let state = new_test_server_state(None).await;
        // we drop both the input sender and the output receiver so that the underlying channel fails
        let (mut player_conn, _, _) = new_test_player_conn(state, false, 1);

        // WHEN
        let result = player_conn
            .send_response(PlayerCommandResponse::Success)
            .await;

        // THEN
        assert_matches!(result, Err(InternalError::Dependency(..)));
    }

    // send_recoverable_user_error() tests

    #[tokio::test]
    async fn GIVEN_player_conn_WHEN_send_recoverable_user_error_THEN_ok() {
        // GIVEN
        let state = new_test_server_state(None).await;
        let (mut player_conn, _, mut output_receiver) = new_test_player_conn(state, false, 1);
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
        let state = new_test_server_state(None).await;
        // we drop both the input sender and the output receiver so that the underlying channel fails
        let (mut player_conn, _, _) = new_test_player_conn(state, false, 1);
        let user_error = UserError::Login(LoginError::IncorrectLobbyPassword);

        // WHEN
        let result = player_conn.send_recoverable_user_error(user_error).await;

        // THEN
        assert_matches!(result, Err(InternalError::Dependency(..)));
    }

    // handle_irrecoverable_user_error() tests

    #[tokio::test]
    async fn GIVEN_player_conn_WHEN_send_irrecoverable_user_error_THEN_ok() {
        // GIVEN
        let state = new_test_server_state(None).await;
        let (player_conn, _, _output_receiver) = new_test_player_conn(state, false, 1);
        let user_error = UserError::ActivityTimeout; // realistic error - if soft lock, we want to kill the connection

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
        let state = new_test_server_state(None).await;
        // we drop both the input sender and the output receiver so that the underlying channel fails
        let (player_conn, _, _) = new_test_player_conn(state, false, 1);
        let user_error = UserError::ActivityTimeout; // realistic error - if soft lock, we want to kill the connection

        // WHEN
        let result = player_conn
            .handle_irrecoverable_user_error(user_error)
            .await;

        // THEN
        assert_matches!(result, Err(InternalError::Dependency(..)));
    }

    // handle_internal_server_error() tests

    #[tokio::test]
    async fn GIVEN_player_conn_WHEN_handle_internal_error_THEN_ok() {
        // GIVEN
        let state = new_test_server_state(None).await;
        let (player_conn, _, _output_receiver) = new_test_player_conn(state, false, 1);
        let internal_error = InternalError::NotLoggedIn;

        // WHEN
        let result = player_conn.handle_internal_error(internal_error).await;

        // THEN
        assert!(result.is_ok()); // player conn simply disconnects, `TextTransport` implementation decides what it does with the error
    }

    #[tokio::test]
    async fn GIVEN_disconnected_player_conn_WHEN_handle_internal_error_THEN_error() {
        // GIVEN
        let state = new_test_server_state(None).await;
        // we drop both the input sender and the output receiver so that the underlying channel fails
        let (player_conn, _, _) = new_test_player_conn(state, false, 1);
        let internal_error = InternalError::NotLoggedIn;

        // WHEN
        let result = player_conn.handle_internal_error(internal_error).await;

        // THEN
        assert_matches!(result, Err(InternalError::Dependency(..)));
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
        let state = new_test_server_state(Some(create_lobby_request.clone())).await;
        let (mut player_conn, input_sender, _) = new_test_player_conn(state, false, 1);
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
        let state = new_test_server_state(None).await;
        // we don't drop the input sender so that we have a valid connection but player_conn times out bc we don't send anything
        let (mut player_conn, _input_sender, _) = new_test_player_conn(state, false, 1);

        // WHEN
        let result = player_conn
            .read_request_with_timeout(Duration::from_secs(1))
            .await;

        // THEN
        assert_matches!(
            result,
            Err(PlayerHandlerError::User(UserError::ActivityTimeout))
        );
    }

    #[tokio::test]
    async fn GIVEN_disconnected_player_conn_WHEN_read_request_with_timeout_THEN_error() {
        // GIVEN
        let state = new_test_server_state(None).await;
        // drop both input sender and output receiver so that the underlying read_json() errors
        let (mut player_conn, _, _) = new_test_player_conn(state, false, 1);

        // WHEN
        let result = player_conn
            .read_request_with_timeout(Duration::from_secs(1))
            .await;

        // THEN
        assert_matches!(
            result,
            Err(PlayerHandlerError::User(UserError::Disconnected))
        );
    }

    #[tokio::test]
    async fn GIVEN_json_conn_error_WHEN_read_request_with_timeout_THEN_error() {
        // GIVEN
        let state = new_test_server_state(None).await;
        // set `fail_during_read_text` to true to cascade the error (TextTransport errors => JsonConn errors => PlayerConn errors)
        let (mut player_conn, _, _) = new_test_player_conn(state, true, 1);

        // WHEN
        let result = player_conn
            .read_request_with_timeout(Duration::from_secs(1))
            .await;

        // THEN
        assert_matches!(
            result,
            Err(PlayerHandlerError::Internal(InternalError::Dependency(..)))
        );
    }

    // helper built out of GIVEN_valid_creds_WHEN_join_lobby_THEN_ok
    // primary use case is to be setup for the leave_lobby() tests
    async fn new_logged_in_player_conn<M, C>(
        state: &JeopardyServerStateGeneric<M, C>,
        create_lobby_request: CreateLobbyRequest,
        username: &str,
        player_channel_buffer_size: usize,
    ) -> (
        PlayerConn<MockTextTransport<PlayerRequest>, M, C>,
        mpsc::Sender<PlayerRequest>,
        mpsc::Receiver<String>,
    )
    where
        M: ManagerGeneric,
        C: CredsValidatorGeneric,
    {
        // GIVEN
        let lobby_name = create_lobby_request.lobby_name.clone();
        let (mut player_conn, input_sender, output_receiver) =
            new_test_player_conn(state.clone(), false, player_channel_buffer_size);
        let login_request = LoginCredentials {
            // derive login request from create lobby request
            lobby_id: create_lobby_request.lobby_name,
            lobby_password: create_lobby_request.lobby_password,
            username: username.to_string(),
        };

        // WHEN
        let _receiver = player_conn.join_lobby(login_request.clone()).await.unwrap();

        // THEN
        assert!(lobby_has_player(&state, &lobby_name, username).await);
        // cached creds match
        let player_creds = player_conn.creds.clone().unwrap();
        assert_eq!(player_creds.lobby_id, login_request.lobby_id);
        assert_eq!(player_creds.lobby_password, ""); // we do not save lobby password
        assert_eq!(player_creds.username, login_request.username);
        assert!(player_conn.lobby.is_some());
        (player_conn, input_sender, output_receiver)
    }

    // really terrible but it is only needed for the following return signatures
    type TestPlayerConn = PlayerConn<
        MockTextTransport<PlayerRequest>,
        MapManager<PasswordProtectedLobby<Jeopardy>>,
        NonZeroAsciiValidator,
    >;
    type TestManagerPlayerConn = PlayerConn<
        MockTextTransport<PlayerRequest>,
        TestManager<PasswordProtectedLobby<Jeopardy>>,
        NonZeroAsciiValidator,
    >;

    async fn new_test_server_state_with_logged_in_players(
        usernames: Vec<String>,
        player_channel_buffer_size: usize,
        lobby_name: &str,
    ) -> (
        JeopardyServerState,
        Vec<(
            // collection of player conn and the respective mpsc handles for MockConn
            TestPlayerConn,
            mpsc::Sender<PlayerRequest>,
            mpsc::Receiver<String>,
        )>,
    ) {
        let request = CreateLobbyRequest {
            lobby_name: lobby_name.to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server_state(Some(request.clone())).await;
        let mut player_conns = vec![];
        for username in usernames {
            let player_conn = new_logged_in_player_conn(
                &state,
                request.clone(),
                &username,
                player_channel_buffer_size,
            )
            .await;
            player_conns.push(player_conn);
        }
        (state, player_conns)
    }

    async fn new_test_manager_server_state_with_logged_in_player(
        username: &str,
        player_channel_buffer_size: usize,
        lobby_name: &str,
    ) -> (
        TestManagerServerState,
        TestManagerPlayerConn,
        mpsc::Sender<PlayerRequest>,
        mpsc::Receiver<String>,
    ) {
        let request = CreateLobbyRequest {
            lobby_name: lobby_name.to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_manager_server_state(Some(request.clone())).await;
        let (player_conn, input_sender, output_receiver) =
            new_logged_in_player_conn(&state, request, username, player_channel_buffer_size).await;
        (state, player_conn, input_sender, output_receiver)
    }

    // join_lobby() tests

    #[tokio::test]
    async fn GIVEN_valid_creds_WHEN_join_lobby_THEN_ok() {
        new_test_server_state_with_logged_in_players(vec!["username".to_string()], 1, "lobby_name")
            .await;
    }

    #[tokio::test]
    async fn GIVEN_invalid_lobby_WHEN_join_lobby_THEN_error() {
        // GIVEN
        let state = new_test_server_state(None).await;
        let (mut player_conn, _, _) = new_test_player_conn(state.clone(), false, 1);
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
        assert_matches!(
            result,
            Err(PlayerHandlerError::User(UserError::Login(
                LoginError::LobbyNotFound
            )))
        );
        // can't check has_player() for lobby that doesn't exist
        assert!(player_conn.lobby.is_none());
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
        let state = new_test_server_state(Some(create_lobby_request.clone())).await;
        let (mut player_conn, _, _) = new_test_player_conn(state.clone(), false, 1);
        let username = "username";
        let login_request = LoginCredentials {
            lobby_id: create_lobby_request.lobby_name,
            lobby_password: "INCORRECT".to_string(), // incorrect password
            username: username.to_string(),
        };

        // WHEN
        let result = player_conn.join_lobby(login_request).await;

        // THEN
        assert_matches!(
            result,
            Err(PlayerHandlerError::User(UserError::Login(
                LoginError::IncorrectLobbyPassword
            )))
        );
        assert_eq!(false, lobby_has_player(&state, lobby_name, username).await); // not added
        assert!(player_conn.lobby.is_none());
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
        let (state, _) =
            new_test_server_state_with_player(create_lobby_request.clone(), username).await;
        let (mut player_conn, _, _) = new_test_player_conn(state.clone(), false, 1);
        let login_request = LoginCredentials {
            lobby_id: create_lobby_request.lobby_name,
            lobby_password: create_lobby_request.lobby_password,
            username: username.to_string(), // reuse the username so it conflicts
        };

        // WHEN
        let result = player_conn.join_lobby(login_request).await;

        // THEN
        assert_matches!(
            result,
            Err(PlayerHandlerError::User(UserError::Login(
                LoginError::UsernameAlreadyTaken
            )))
        );
        // player not added guaranteed by lobby.add_player() call
        assert!(player_conn.lobby.is_none());
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
        let state = new_test_manager_server_state(Some(create_lobby_request.clone())).await;
        state.manager().write().await.set_always_fail(); // lobby lookup should fail
        let (mut player_conn, _, _) = new_test_player_conn(state.clone(), false, 1);
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
        assert_matches!(
            result,
            Err(PlayerHandlerError::Internal(InternalError::Dependency(..)))
        );
        assert_eq!(false, lobby_has_player(&state, lobby_name, username).await); // not added
        assert!(player_conn.lobby.is_none());
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
        let state = new_test_server_state(Some(create_lobby_request.clone())).await;
        let (mut player_conn, _, _) = new_test_player_conn(state.clone(), false, 1);
        let username = "username";
        let login_request = LoginCredentials {
            lobby_id: create_lobby_request.lobby_name,
            lobby_password: create_lobby_request.lobby_password,
            username: username.to_string(),
        };

        // preconditions
        shutdown_lobby(&state, lobby_name).await; // ensure lobby is shut down so add fails

        // WHEN
        let result = player_conn.join_lobby(login_request).await;

        // THEN
        assert_matches!(
            result,
            Err(PlayerHandlerError::Internal(InternalError::InactiveLobby(
                ..
            )))
        );
        // can't check has_player() bc lobby is shut down
        assert!(player_conn.lobby.is_none());
        assert!(player_conn.creds.is_none());
    }

    // leave_lobby() tests

    #[tokio::test]
    async fn GIVEN_logged_in_player_conns_WHEN_leave_lobby_THEN_ok() {
        // GIVEN
        // more than one player so that the lobby doesn't get deleted
        let usernames = vec!["player1".to_string(), "player2".to_string()];
        let lobby_name = "lobby_name";
        let (state, mut player_conns) =
            new_test_server_state_with_logged_in_players(usernames.clone(), 1, lobby_name).await;
        let (player_conn, _, _) = &mut player_conns[0];

        // WHEN
        let LeaveCredentials {
            player,
            former_creds,
            ..
        } = player_conn.leave_lobby().await.unwrap();

        // THEN
        let expected_player_id = &usernames[0];
        assert_eq!(expected_player_id, player.id()); // returned player is correct
        assert_matches!( // returned creds are correct
            former_creds,
            LoginCredentials { lobby_id, lobby_password, username }
                if lobby_id == lobby_name && username == usernames[0] && lobby_password == ""
        );
        // we cannot test lobby ref bc it is a clone of the underlying mpsc

        let has_player = lobby_has_player(&state, lobby_name, expected_player_id).await;
        assert_eq!(false, has_player); // lobby still exists bc player_count > 0

        assert!(player_conn.lobby.is_none());
        assert!(player_conn.creds.is_none());
    }

    #[tokio::test]
    async fn GIVEN_not_logged_in_player_conn_WHEN_leave_lobby_THEN_error() {
        let state = new_test_server_state(None).await; // no lobby to log in to
        let (mut player_conn, _, _) = new_test_player_conn(state, false, 1); // not logged in

        // WHEN
        let result_no_lobby_cached = player_conn.leave_lobby().await;

        // WHEN
        player_conn.creds = Some(LoginCredentials {
            lobby_id: String::new(),
            lobby_password: String::new(),
            username: String::new(),
        });
        let result_no_creds_cached = player_conn.leave_lobby().await;

        // THEN
        assert_matches!(result_no_lobby_cached, Err(InternalError::NotLoggedIn));
        assert_matches!(result_no_creds_cached, Err(InternalError::NotLoggedIn));
    }

    #[tokio::test]
    async fn GIVEN_shutdown_lobby_WHEN_leave_lobby_THEN_error() {
        // GIVEN
        let username = "player1";
        let lobby_name = "lobby_name";
        let (state, mut player_conns) =
            new_test_server_state_with_logged_in_players(vec![username.to_string()], 1, lobby_name)
                .await;
        let (player_conn, _, _) = &mut player_conns[0];

        // preconditions
        shutdown_lobby(&state, lobby_name).await; // shut down lobby so it fails

        // WHEN
        let result = player_conn.leave_lobby().await;

        // THEN
        assert_matches!(result, Err(InternalError::InactiveLobby(..)));
        assert!(player_conn.lobby.is_none());
        assert!(player_conn.creds.is_none());
    }

    // login_loop() tests

    #[tokio::test]
    async fn GIVEN_valid_creds_WHEN_login_loop_THEN_ok() {
        // GIVEN
        let request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server_state(Some(request.clone())).await;
        let (mut player_conn, input_sender, _) = new_test_player_conn(state, false, 1);

        // preconditions
        input_sender
            .send(PlayerRequest::Login(LoginCredentials {
                lobby_id: request.lobby_name,
                lobby_password: request.lobby_password,
                username: "username".to_string(),
            }))
            .await
            .unwrap();

        // WHEN
        let _receiver = player_conn.login(1, Duration::from_secs(1)).await.unwrap();

        // THEN
        // loosely check that join_lobby() worked
        // the details are guaranteed by join_lobby() tests
        assert!(player_conn.creds.is_some());
        assert!(player_conn.lobby.is_some());
    }

    #[tokio::test]
    async fn GIVEN_non_login_request_WHEN_login_loop_THEN_error() {
        // GIVEN
        let state = new_test_server_state(Some(CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        }))
        .await;
        let (mut player_conn, input_sender, _) = new_test_player_conn(state, false, 1);

        // preconditions
        input_sender // send command when login expected
            .send(PlayerRequest::Command(PlayerCommand::Buzz))
            .await
            .unwrap();

        // WHEN
        let result = player_conn.login(1, Duration::from_secs(1)).await;

        // THEN
        assert_matches!(
            result,
            Err(PlayerHandlerError::User(UserError::UnexpectedRequestType(
                ..
            )))
        );

        assert!(player_conn.creds.is_none());
        assert!(player_conn.lobby.is_none());
    }

    #[tokio::test]
    async fn GIVEN_invalid_format_creds_WHEN_login_loop_THEN_error() {
        // GIVEN
        let request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server_state(Some(request.clone())).await;
        let (mut player_conn, input_sender, _) = new_test_player_conn(state, false, 1);
        for i in 0..3 {
            // switch which field has the invalid format
            let lobby_id = if i == 0 {
                String::new()
            } else {
                request.lobby_name.clone()
            };
            let lobby_password = if i == 1 {
                String::new()
            } else {
                request.lobby_password.clone()
            };
            let username = if i == 2 {
                String::new()
            } else {
                "username".to_string()
            };
            // preconditions
            input_sender
                .send(PlayerRequest::Login(LoginCredentials {
                    lobby_id,
                    lobby_password,
                    username,
                }))
                .await
                .unwrap();

            // WHEN
            let result = player_conn.login(1, Duration::from_secs(1)).await;

            // THEN
            assert_matches!(
                result,
                Err(PlayerHandlerError::User(UserError::Login(
                    LoginError::InvalidLoginCredentialsFormat
                )))
            );

            assert!(player_conn.creds.is_none());
            assert!(player_conn.lobby.is_none());
        }
    }

    #[tokio::test]
    async fn GIVEN_max_attempts_WHEN_login_loop_THEN_error() {
        // GIVEN
        let request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let max_attempts = 3;
        let state = new_test_server_state(Some(request.clone())).await;
        let (mut player_conn, input_sender, _output_receiver) = // we don't drop the input/output hooks so the helpers pass 
            new_test_player_conn(state, false, max_attempts);

        // preconditions
        for _ in 0..max_attempts {
            // queue multiple requests so this fails
            input_sender
                .send(PlayerRequest::Login(LoginCredentials {
                    lobby_id: request.lobby_name.clone(),
                    lobby_password: "INVALID".to_string(), // give invalid password
                    username: "username".to_string(),
                }))
                .await
                .unwrap();
        }

        // WHEN
        let result = player_conn
            .login(max_attempts, Duration::from_secs(1))
            .await;

        // THEN
        // we don't need to test that we get the incorrect password error bc the helper is tested
        assert_matches!(
            result, // ensure we hit attempt limit
            Err(PlayerHandlerError::User(UserError::Login(
                LoginError::ExceededAttemptLimit
            )))
        );

        assert!(player_conn.creds.is_none());
        assert!(player_conn.lobby.is_none());
    }

    #[tokio::test]
    async fn GIVEN_join_lobby_error_WHEN_login_loop_THEN_error() {
        // GIVEN
        let request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_manager_server_state(Some(request.clone())).await;
        let (mut player_conn, input_sender, _) = new_test_player_conn(state.clone(), false, 1);

        // preconditions
        state.manager().write().await.set_always_fail();
        input_sender
            .send(PlayerRequest::Login(LoginCredentials {
                lobby_id: request.lobby_name,
                lobby_password: request.lobby_password,
                username: "username".to_string(),
            }))
            .await
            .unwrap();

        // WHEN
        let result = player_conn.login(1, Duration::from_secs(1)).await;

        // THEN
        assert_matches!(
            result, // ensure join_lobby error is propagated
            Err(PlayerHandlerError::Internal(InternalError::Dependency(..)))
        );
        assert!(player_conn.creds.is_none());
        assert!(player_conn.lobby.is_none());
    }

    // handle_player_command() tests

    #[tokio::test]
    async fn GIVEN_valid_command_WHEN_handle_player_command_THEN_ok() {
        // GIVEN
        let usernames = vec!["player1".to_string()];
        let lobby_name = "lobby_name";
        let (_state, mut player_conns) =
            new_test_server_state_with_logged_in_players(usernames.clone(), 1, lobby_name).await;
        let (mut player_conn, _, mut output) = player_conns.remove(0);

        // WHEN
        player_conn
            .handle_player_command(PlayerCommand::GetWager)
            .await
            .unwrap();

        // THEN
        // ensure that the correct response was sent
        let response = output.recv().await.unwrap();
        let expected_response = serde_json::to_string(&PlayerResponse {
            result: Ok(PlayerCommandResponse::GetWager(0)), // wager is always default 0
        })
        .unwrap();
        assert_eq!(expected_response, response);
    }

    #[tokio::test]
    async fn GIVEN_not_logged_in_WHEN_handle_player_command_THEN_error() {
        // GIVEN
        let state = new_test_server_state(None).await;
        let (mut player_conn, _, _) = new_test_player_conn(state, false, 1);

        // WHEN
        let result = player_conn.handle_player_command(PlayerCommand::Buzz).await;

        // THEN
        assert_matches!(
            result,
            Err(PlayerHandlerError::Internal(InternalError::NotLoggedIn))
        );
    }

    #[tokio::test]
    async fn GIVEN_shutdown_lobby_WHEN_handle_player_command_THEN_error() {
        // GIVEN
        let usernames = vec!["player1".to_string()];
        let lobby_name = "lobby_name";
        let (state, mut player_conns) =
            new_test_server_state_with_logged_in_players(usernames.clone(), 1, lobby_name).await;
        let (mut player_conn, _, _) = player_conns.remove(0);

        // preconditions
        shutdown_lobby(&state, lobby_name).await;

        // WHEN
        let result = player_conn.handle_player_command(PlayerCommand::Buzz).await;

        // THEN
        assert_matches!(
            result,
            Err(PlayerHandlerError::Internal(InternalError::InactiveLobby(
                ..
            )))
        );
    }

    #[tokio::test]
    async fn GIVEN_invalid_command_WHEN_handle_player_command_THEN_error() {
        // GIVEN
        let usernames = vec!["player1".to_string()];
        let lobby_name = "lobby_name";
        let (state, mut player_conns) =
            new_test_server_state_with_logged_in_players(usernames.clone(), 1, lobby_name).await;
        // we need inner state to show final jeopardy hint to allow for SetWager
        send_host_command_for_lobby(
            &state,
            lobby_name,
            "host_password",
            HostCommand::ShowFinalJeopardyHint,
        )
        .await;

        let (mut player_conn, _, mut output) = player_conns.remove(0);
        let invalid_wager = -1000;

        // WHEN
        player_conn
            .handle_player_command(PlayerCommand::SetWager(invalid_wager))
            .await
            .unwrap();

        // THEN
        // ensure that the error was sent over the websocket
        let response = output.recv().await.unwrap();
        let expected_response =
            serde_json::to_string(&PlayerResponse {
                result: Err(PlayerHandlerError::User(UserError::Game(
                    JeopardyError::PlayerMisconfig(JeopardyPlayerError::InvalidWager {
                        wager: invalid_wager,
                        current_points: 0, // the default points are 0 in the helper
                    }),
                ))
                .to_string()),
            })
            .unwrap();
        assert_eq!(expected_response, response);
    }

    // main() tests

    #[tokio::test]
    async fn GIVEN_valid_command_WHEN_main_THEN_ok() {
        // GIVEN
        let usernames = vec!["player1".to_string()];
        let lobby_name = "lobby_name";
        let (_state, mut player_conns) =
            new_test_server_state_with_logged_in_players(usernames.clone(), 1, lobby_name).await;
        let (mut player_conn, input, mut output) = player_conns.remove(0);
        let activity_timeout = Duration::from_secs(1);

        // preconditions
        input
            .send(PlayerRequest::Command(PlayerCommand::GetWager))
            .await
            .unwrap();
        drop(input); // drop `input` so that the main loop cleanly disconnects - if not main() runs until timeout

        // we can get the mpsc::Receiver from the join_lobby() call but it is easier to test like this
        let (_player_event_sender, player_event_receiver) = mpsc::channel(1);

        // WHEN
        player_conn
            .main(player_event_receiver, activity_timeout)
            .await
            .unwrap();

        // THEN
        let response = output.recv().await.unwrap(); // ensure that we receive the response from the lobby
        let expected_response = serde_json::to_string(&PlayerResponse {
            result: Ok(PlayerCommandResponse::GetWager(0)), // wager is always default 0
        })
        .unwrap();
        assert_eq!(expected_response, response);
    }

    #[tokio::test]
    async fn GIVEN_player_event_WHEN_main_THEN_ok() {
        // GIVEN
        let state = new_test_server_state(None).await; // we don't need an actual lobby here, we just sim the events
        let (mut player_conn, _input, mut output) = new_test_player_conn(state, false, 2);
        let title = "title".to_string();
        let content = "content".to_string();
        let points = 0;
        let activity_timeout = Duration::from_secs(1);

        // preconditions - queue up player events
        // we can get the mpsc::Receiver from the join_lobby() call but it is easier to test like this
        let (player_event_sender, player_event_receiver) = mpsc::channel(2);
        player_event_sender
            .send(JeopardyPlayerEvent::PointsUpdate(points))
            .await
            .unwrap();
        player_event_sender
            .send(JeopardyPlayerEvent::Display(
                JeopardyDisplayState::Question(TextCard {
                    title: title.clone(),
                    content: content.clone(),
                }),
            ))
            .await
            .unwrap();
        // drop `player_event_sender` so that the main loop cleanly disconnects - if not main() runs until timeout
        // this is equivalent to the lobby kicking a player
        drop(player_event_sender);

        // WHEN
        player_conn
            .main(player_event_receiver, activity_timeout)
            .await
            .unwrap();

        // THEN - we test both events here bc they get remapped to standard responses
        // ensure points update event is received
        let response = output.recv().await.unwrap();
        let expected_response = serde_json::to_string(&PlayerResponse {
            result: Ok(PlayerCommandResponse::GetPoints(points)),
        })
        .unwrap();
        assert_eq!(expected_response, response);

        // ensure text card event is received
        let response = output.recv().await.unwrap();
        let expected_response = serde_json::to_string(&PlayerResponse {
            result: Ok(PlayerCommandResponse::Refresh(
                JeopardyDisplayState::Question(TextCard { title, content }),
            )),
        })
        .unwrap();
        assert_eq!(expected_response, response);
    }

    #[tokio::test]
    async fn GIVEN_non_command_request_WHEN_main_THEN_error() {
        // GIVEN
        let state = new_test_server_state(None).await; // we don't need an actual lobby here, we just sim the events
        let (mut player_conn, input, _) = new_test_player_conn(state, false, 1);
        let activity_timeout = Duration::from_secs(1);

        // preconditions
        input
            .send(PlayerRequest::Login(LoginCredentials {
                lobby_id: "lobby_name".to_string(), // main() isn't expecting a login request and should error out
                lobby_password: "lobby_password".to_string(),
                username: "username".to_string(),
            }))
            .await
            .unwrap();
        let (_player_event_sender, player_event_receiver) = mpsc::channel(1);

        // WHEN
        let result = player_conn
            .main(player_event_receiver, activity_timeout)
            .await;

        // THEN
        assert_matches!(
            result,
            Err(PlayerHandlerError::User(UserError::UnexpectedRequestType(
                ..
            )))
        )
    }

    #[tokio::test]
    async fn GIVEN_timeout_WHEN_main_THEN_error() {
        // GIVEN
        let state = new_test_server_state(None).await;
        let activity_timeout = Duration::from_secs(1);

        // precondition: we don't drop the input/output mpsc handles so it is set up for a valid connection,
        // we just don't send anything so it times out
        let (mut player_conn, _input, _output) = new_test_player_conn(state, false, 1);
        let (_player_event_sender, player_event_receiver) = mpsc::channel(1);

        // WHEN
        let result = player_conn
            .main(player_event_receiver, activity_timeout)
            .await;

        // THEN
        assert_matches!(
            result,
            Err(PlayerHandlerError::User(UserError::ActivityTimeout))
        );
    }

    #[tokio::test]
    async fn GIVEN_json_conn_error_WHEN_main_THEN_error() {
        // GIVEN
        let state = new_test_server_state(None).await;
        let (mut player_conn, _, _) = new_test_player_conn(state, true, 1); // fails during read set to `true`
        let (_player_event_sender, player_event_receiver) = mpsc::channel(1);
        let activity_timeout = Duration::from_secs(1);

        // WHEN
        let result = player_conn
            .main(player_event_receiver, activity_timeout)
            .await;

        // THEN
        assert_matches!(
            result,
            Err(PlayerHandlerError::Internal(InternalError::Dependency(..)))
        );
    }
}
