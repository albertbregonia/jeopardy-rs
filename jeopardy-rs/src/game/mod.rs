use thiserror::Error;

pub mod jeopardy;
mod player;

pub enum JeopardyCommand {}

pub enum JeopardyCommandResponse {}

#[derive(Debug, Error)]
pub enum JeopardyError {}
