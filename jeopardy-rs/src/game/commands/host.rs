use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Enum of commands the host or admin of the Jeopardy game can send.
#[derive(Debug, Deserialize, Clone)]
pub enum HostCommand {
    // may return JeopardyError - may be invalid board index
    ShowBoard {
        board_index: usize,
    },
    // may return JeopardyError - may be invalid indices
    ShowQuestion {
        board_index: usize,
        category_index: usize,
        question_index: usize,
    },
    ShowCurrentAnswer,
    // may return JeopardyError - may be invalid indices
    GetAnswer {
        board_index: usize,
        category_index: usize,
        question_index: usize,
    },
    ShowFinalJeopardyHint,
    ShowFinalJeopardyQuestion,
    ShowFinalJeopardyAnswer,
    // may return JeopardyError - may be invalid player ID
    SetPoints {
        player_id: String,
        points: i32,
    },
    // may return JeopardyError - may be invalid player ID
    UpdatePoints {
        player_id: String,
        delta: i32,
    },
    // variants without the comment are technically infallible
    // but the method signature cannot guarantee this
    ClearBuzzerQueue,
    GetBuzzerQueue,
}

/// Enum of responses the `HostCommand` requests can send back
/// Most of them will send `HostCommandResponse::Success` which is equivalently `Ok(())`.
/// Otherwise, the variant mirrors the command:
/// ie. `HostCommand::GetAnswer` maps to `HostCommandResponse::GetAnswer(String)`
#[derive(Debug, Serialize)]
pub enum HostCommandResponse {
    Success,
    GetAnswer(String),
    UpdatePoints(i32),
    GetBuzzerQueue(VecDeque<String>),
}
