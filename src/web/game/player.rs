use serde::Serialize;
use thiserror::Error;
use tokio::sync::mpsc::{Sender, error::SendError};

// `T`, throughout this file (and the upward propagation),
// is a generic used to represent the output type
// to send to the player frontend

#[derive(Debug, Error)]
pub enum PlayerManagementError<T> {
    #[error("{0}")]
    User(#[from] UserError),
    #[error("{0}")]
    Internal(#[from] InternalError<T>),
}

#[derive(Debug, Error)]
pub enum UserError {
    #[error("Wager value of {0} is invalid. It must be between 0 and {1}")]
    InvalidWager(i32, i32),
}

#[derive(Debug, Error)]
pub enum InternalError<T> {
    #[error("Failed to send back to Player over tokio channel: {0}")]
    Send(#[from] SendError<T>),
}

pub struct Player<T: Serialize> {
    name: String,       // immutable
    channel: Sender<T>, // hook to the original websocket
    points: i32,        // can go negative
    wager: i32,
    input: String,
}

impl<T> Player<T>
where
    T: Serialize,
{
    pub fn new(name: String, channel: Sender<T>) -> Self {
        Self {
            name,
            channel,
            points: 0,
            wager: 0,
            input: "No answer".to_string(),
        }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_points(&self) -> i32 {
        self.points
    }

    pub fn get_wager(&self) -> i32 {
        self.wager
    }

    pub fn set_wager(&mut self, wager: i32) -> Result<(), PlayerManagementError<T>> {
        if wager as i32 > self.points || wager < 0 {
            return Err(PlayerManagementError::User(UserError::InvalidWager(
                wager,
                self.points,
            )));
        }
        self.wager = wager as i32;
        Ok(())
    }

    pub fn get_input(&self) -> &str {
        &self.input
    }

    pub fn set_input(&mut self, input: String) -> Result<(), PlayerManagementError<T>> {
        // TODO: validation
        self.input = input;
        Ok(())
    }

    pub fn set_points(&mut self, points: i32) -> Result<(), PlayerManagementError<T>> {
        // TODO: validation
        self.points = points;
        Ok(())
    }

    pub fn update_points(&mut self, delta: i32) -> Result<i32, PlayerManagementError<T>> {
        // TODO: validation
        self.points += delta;
        Ok(self.points)
    }

    pub async fn send(&mut self, payload: T) -> Result<(), PlayerManagementError<T>> {
        self.channel
            .send(payload)
            .await
            .map_err(|e| PlayerManagementError::Internal(InternalError::Send(e)))?;
        Ok(())
    }
}
