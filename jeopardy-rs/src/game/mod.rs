use thiserror::Error;

mod handler;
pub use handler::*;

pub mod jeopardy;
mod player;

pub enum JeopardyCommand {}

pub enum JeopardyCommandResponse {}

#[derive(Debug, Error)]
pub enum JeopardyError {}
