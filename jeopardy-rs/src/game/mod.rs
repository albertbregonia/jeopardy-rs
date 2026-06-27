use thiserror::Error;

mod handler;
pub use handler::*;

mod commands;

pub mod jeopardy;
mod player;

pub enum JeopardyCommand {}

pub enum JeopardyCommandResponse {}

#[derive(Debug, Error)]
pub enum JeopardyError {}
