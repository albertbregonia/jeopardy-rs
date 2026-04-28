use thiserror::Error;

use crate::{
    json_websocket::{self, JsonWebsocketError},
    web::game::{
        LobbyManagerError,
        lobby::{self, LobbyError},
        lobby_manager,
    },
};

pub mod host;
pub mod player;

pub const CREATE_LOBBY_ERROR_MSG: &str = "Malformed create lobby request";
pub const INVALID_LOBBY_NAME_ERROR_MSG: &str =
    "Invalid lobby name. Must be lowercase and alphanumeric (underscores permitted)";

#[derive(Debug, Error)]
// top level error type for player events
pub enum PlayerHandlerError {
    #[error("User error: {0}")]
    User(#[from] UserError),
    #[error("Internal Server Error: {0}")]
    Internal(#[from] InternalError),
}

#[derive(Debug, Error)]
pub enum UserError {
    #[error("Incorrect password for the desired lobby: {0}")]
    IncorrectLobbyPassword(String),
    #[error("Expected a login request from the client that was not received.")]
    ExpectedLoginRequest,
    #[error("{0}")]
    WebSocket(#[from] json_websocket::UserError),
    #[error("{0}")]
    LobbyManager(#[from] lobby_manager::UserError),
    #[error("{0}")]
    LobbyError(#[from] lobby::UserError),
}

// enum rewrap simply to distinguish between user error and internal failure
#[derive(Debug, Error)]
pub enum InternalError {
    #[error("{0}")]
    WebSocket(#[from] json_websocket::InternalError),
    #[error("{0}")]
    LobbyManager(#[from] lobby_manager::InternalError),
    #[error("{0}")]
    LobbyError(#[from] lobby::InternalError),
}

impl From<LobbyManagerError> for PlayerHandlerError {
    fn from(e: LobbyManagerError) -> Self {
        match e {
            LobbyManagerError::User(user_error) => {
                PlayerHandlerError::User(UserError::LobbyManager(user_error))
            }
            LobbyManagerError::Internal(internal_error) => {
                PlayerHandlerError::Internal(InternalError::LobbyManager(internal_error))
            }
        }
    }
}

impl From<LobbyError> for PlayerHandlerError {
    fn from(e: LobbyError) -> Self {
        match e {
            LobbyError::User(user_error) => {
                PlayerHandlerError::User(UserError::LobbyError(user_error))
            }
            LobbyError::Internal(internal_error) => {
                PlayerHandlerError::Internal(InternalError::LobbyError(internal_error))
            }
        }
    }
}

impl From<JsonWebsocketError> for PlayerHandlerError {
    fn from(e: JsonWebsocketError) -> Self {
        match e {
            JsonWebsocketError::User(user_error) => {
                PlayerHandlerError::User(UserError::WebSocket(user_error))
            }
            JsonWebsocketError::Internal(internal_error) => {
                PlayerHandlerError::Internal(InternalError::WebSocket(internal_error))
            }
        }
    }
}
