use serde::{Deserialize, Serialize};

use crate::game::jeopardy::board::Board;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TextCard {
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
pub enum JeopardyDisplayState {
    Question(TextCard),
    Answer(TextCard),
    FinalJeopardyHint(TextCard),
    FinalJeopardyQuestion(TextCard),
    FinalJeopardyAnswer(TextCard),
    Board(Board), // a redacted copy of the current game board
}

/// Enum of commands any player or the host of the Jeopardy game can send.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerCommand {
    Buzz,
    Refresh,
    GetPoints,
    GetWager,
    SetWager(i32),
    GetFreeResponse,
    SetFreeResponse(String),
    GetScoreboard,
}

/// Enum of responses the `PlayerCommand` requests can send back
/// Most of them will send `PlayerCommandResponse::Success` which is equivalently `Ok(())`.
/// Otherwise, the variant mirrors the command:
/// ie. `PlayerCommand::GetPoints` maps to `HostCommandResponse::GetPoints(i32)`
#[derive(Debug, Clone, Serialize)]
pub enum PlayerCommandResponse {
    Success,
    Refresh(JeopardyDisplayState),
    GetPoints(i32),
    GetWager(i32),
    GetFreeResponse(String),
    GetScoreboard(Vec<(i32, String)>),
}
