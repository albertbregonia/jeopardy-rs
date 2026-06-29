use crate::game::{
    commands::{
        host::{HostCommand, HostCommandResponse},
        player::{PlayerCommand, PlayerCommandResponse},
    },
    player::JeopardyPlayerError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

mod handler;
pub use handler::*;
pub mod commands;
pub mod jeopardy;
pub mod player;

/// A unified type to combine requests when interfacing with a Game trait
#[derive(Debug, Deserialize)]
pub enum JeopardyCommand {
    Host {
        host_password: String,
        command: HostCommand,
    },
    Player {
        player_id: String,
        command: PlayerCommand,
    },
}

/// A unified type to combine responses when interfacing with a Game trait
#[derive(Debug, Serialize)]
pub enum JeopardyCommandResponse {
    Host(HostCommandResponse),
    Player(PlayerCommandResponse),
}

// NOTE: these error messages are propagated to the user
/// Errors that the player / host may invoke due to bad input
#[derive(Debug, Error)]
pub enum JeopardyError {
    #[error("Invalid board index {0} for the given Jeopardy instance")]
    InvalidBoardIndex(usize),
    #[error("Invalid category index {0} for the given Jeopardy board")]
    InvalidCategoryIndex(usize),
    #[error("Invalid question index {0} for the given Jeopardy category")]
    InvalidQuestionIndex(usize),
    #[error("The corresponding player for ID: {0} was not found")]
    PlayerForGivenIDNotFound(String),
    #[error("Incorrect host password. Action unauthorized")]
    IncorrectHostPassword,
    #[error("Failed to create Jeopardy game instance. No boards found")]
    GameBoardsNotFound, // bc of our many checks this is very rare but cannot compiler guarantee
    #[error(transparent)]
    PlayerMisconfig(#[from] JeopardyPlayerError),
}
