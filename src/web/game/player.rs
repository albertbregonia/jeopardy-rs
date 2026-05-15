use std::fmt::Debug;

use serde::Serialize;
use thiserror::Error;
use tokio::sync::mpsc::{Sender, error::SendError};

// `T`, throughout this file (and the upward propagation),
// is a generic used to represent the output type
// to send to the player frontend

#[derive(Debug, Error)]
pub enum PlayerError<T> {
    #[error("{0}")]
    User(#[from] UserError),
    #[error("{0}")]
    Internal(#[from] InternalError<T>),
}

#[derive(Debug, Error)]
pub enum UserError {
    #[error("Wager value of {0} is invalid. It must be between 0 and {1}")]
    InvalidWager(i32, i32),
    #[error("Free response is invalid. Must be alphanumeric")]
    InvalidFreeResponse,
}

#[derive(Debug, Error)]
pub enum InternalError<T> {
    #[error("Failed to send back to Player over tokio channel: {0}")]
    Send(#[from] SendError<T>),
}

#[derive(Clone)]
pub struct Player<T: Serialize + Debug> {
    name: String,       // immutable
    channel: Sender<T>, // hook to the original websocket
    points: i32,        // can go negative
    wager: i32,
    free_response: String,
}

impl<T> Player<T>
where
    T: Serialize + Debug,
{
    pub fn new(name: String, channel: Sender<T>) -> Self {
        Self {
            name,
            channel,
            points: 0,
            wager: 0,
            free_response: "No answer".to_string(),
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

    pub fn set_wager(&mut self, wager: i32) -> Result<(), PlayerError<T>> {
        if wager > self.points || wager < 0 {
            return Err(PlayerError::User(UserError::InvalidWager(
                wager,
                self.points,
            )));
        }
        self.wager = wager;
        Ok(())
    }

    pub fn get_free_response(&self) -> &str {
        &self.free_response
    }

    fn validate_free_response(free_response: &str) -> bool {
        free_response
            .chars()
            .all(|c| c.is_alphanumeric() || c.is_whitespace())
    }

    pub fn set_free_response(&mut self, free_response: String) -> Result<(), PlayerError<T>> {
        // NOTE: normally this would be validated at the higher level
        // but in this case, we never want to store input strings that aren't alphanumeric
        if !Self::validate_free_response(&free_response) {
            return Err(PlayerError::User(UserError::InvalidFreeResponse));
        }
        self.free_response = free_response.trim().to_string();
        Ok(())
    }

    pub fn set_points(&mut self, points: i32) {
        self.points = points;
    }

    pub fn update_points(&mut self, delta: i32) -> i32 {
        self.points += delta;
        self.points
    }

    pub async fn send(&mut self, payload: T) -> Result<(), PlayerError<T>> {
        self.channel
            .send(payload)
            .await
            .map_err(|e| PlayerError::Internal(InternalError::Send(e)))?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod player_tests {
    use tokio::sync::mpsc;

    use super::*;

    const TEST_PLAYER_NAME: &str = "player";
    const TEST_INVALID_FREE_RESPONSE: &str = "the answer.";
    const TEST_VALID_FREE_RESPONSE: &str = "the answer";

    #[test]
    fn GIVEN_invalid_wager_WHEN_set_wager_THEN_err() {
        // GIVEN
        let (sender, _receiver) = mpsc::channel::<u8>(1);
        let mut player = Player::new(TEST_PLAYER_NAME.to_string(), sender);
        player.points = 100;
        // WHEN
        let negative_wager = -1;
        let negative_wager_result = player.set_wager(negative_wager);
        // THEN
        assert!(matches!(
            negative_wager_result,
            Err(PlayerError::User(UserError::InvalidWager(wager, points))) if wager == negative_wager && points == player.points
        ));

        // WHEN
        let wager_more_than_owned = player.points + 1;
        let wager_more_than_owned_result = player.set_wager(wager_more_than_owned);
        // THEN
        assert!(matches!(
            wager_more_than_owned_result,
            Err(PlayerError::User(UserError::InvalidWager(wager, points))) if wager == wager_more_than_owned && points == player.points
        ));
    }

    #[test]
    fn GIVEN_invalid_free_response_WHEN_set_free_response_THEN_err() {
        // GIVEN
        let (sender, _receiver) = mpsc::channel::<u8>(1);
        let mut player = Player::new(TEST_PLAYER_NAME.to_string(), sender);
        // WHEN
        let result = player.set_free_response(TEST_INVALID_FREE_RESPONSE.to_string());
        // THEN
        assert!(matches!(
            result,
            Err(PlayerError::User(UserError::InvalidFreeResponse))
        ));
    }

    #[test]
    fn GIVEN_valid_free_response_WHEN_set_free_response_THEN_ok() {
        // GIVEN
        let (sender, _receiver) = mpsc::channel::<u8>(1);
        let mut player = Player::new(TEST_PLAYER_NAME.to_string(), sender);
        // WHEN
        player
            .set_free_response(TEST_VALID_FREE_RESPONSE.to_string())
            .unwrap();
        // THEN
        assert_eq!(player.free_response, TEST_VALID_FREE_RESPONSE);
    }
}
