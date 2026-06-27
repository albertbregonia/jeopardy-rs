use serde::Serialize;
use stagecrew::player::Player;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::game::commands::player::JeopardyDisplayEvent;

/// `JeopardyPlayerEvent` is content to be sent to the player
#[derive(Debug, Clone, Serialize)]
pub enum JeopardyPlayerEvent {
    Display(JeopardyDisplayEvent),
    PointsUpdate(i32),
}

#[derive(Debug, Error)]
pub enum JeopardyPlayerError {
    #[error(
        "Invalid wager: {wager}. Must be within range [0-{current_points}] (inclusive). If your points are negative, you can wager enough to get to 0"
    )]
    InvalidWager { wager: i32, current_points: i32 },
    #[error("")]
    ConnectionLost,
}

/// Represents the player and their respective points, wager, etc.
#[derive(Debug, Serialize)]
pub struct JeopardyPlayer {
    name: String,
    wager: i32,
    pub points: i32, // pub is fine because with the api we aren't enforcing validation here
    pub free_response: String,

    // keep a handle to the player's connection so we can send events directly
    #[serde(skip_serializing)]
    sender: mpsc::Sender<JeopardyPlayerEvent>,
}

impl Player for JeopardyPlayer {
    fn id(&self) -> &str {
        &self.name
    }
}

impl JeopardyPlayer {
    pub fn new(name: String, sender: mpsc::Sender<JeopardyPlayerEvent>) -> Self {
        Self {
            name,
            wager: 0,
            points: 0,
            free_response: String::new(),
            sender,
        }
    }

    pub fn wager(&self) -> i32 {
        self.wager
    }

    pub fn set_wager(&mut self, wager: i32) -> Result<(), JeopardyPlayerError> {
        if wager < 0 {
            return Err(JeopardyPlayerError::InvalidWager {
                wager,
                current_points: self.points,
            });
        }
        // if you have negative points, you can wager enough to get to zero
        let wager_for_negatives = self.points < 0 && wager != self.points.abs();

        // you cannot wager more points than you have
        let wager_more_than_have = self.points >= 0 && wager > self.points;

        if wager_for_negatives || wager_more_than_have {
            return Err(JeopardyPlayerError::InvalidWager {
                wager,
                current_points: self.points,
            });
        }
        self.wager = wager;
        Ok(())
    }

    pub async fn send(&mut self, event: JeopardyPlayerEvent) -> Result<(), JeopardyPlayerError> {
        self.sender
            .send(event)
            .await
            .map_err(|_| JeopardyPlayerError::ConnectionLost)
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod player_tests {
    use stagecrew::player::Player;
    use tokio::sync::mpsc;

    use crate::game::player::{JeopardyPlayer, JeopardyPlayerError, JeopardyPlayerEvent};

    fn new_jeopardy_player() -> (JeopardyPlayer, mpsc::Receiver<JeopardyPlayerEvent>) {
        let (sender, rx) = mpsc::channel(1);
        let name = "test".to_string();
        let player = JeopardyPlayer::new(name.clone(), sender);
        assert_eq!(name, player.id());
        (player, rx)
    }

    #[test]
    fn GIVEN_wager_WHEN_set_wager_THEN_ok() {
        // GIVEN
        let (mut player, _) = new_jeopardy_player();
        player.points = 1000;

        // WHEN - anything in between 0 to points
        for points in 0..=player.points {
            player.set_wager(points).unwrap();
            // THEN
            assert_eq!(points, player.wager());
        }
    }

    #[test]
    fn GIVEN_wager_for_negatives_WHEN_set_wager_THEN_ok() {
        // GIVEN
        let (mut player, _) = new_jeopardy_player();
        player.points = -1000;
        let wager = player.points.abs();

        // WHEN - if negative points, you can wager to get to 0
        player.set_wager(wager).unwrap();

        assert_eq!(wager, player.wager());
    }

    #[test]
    fn GIVEN_negative_wager_WHEN_set_wager_THEN_error() {
        // GIVEN
        let (mut player, _) = new_jeopardy_player();
        let test_wager = -1;

        // WHEN
        let result = player.set_wager(test_wager);

        // THEN
        assert!(matches!(
            result,
            Err(JeopardyPlayerError::InvalidWager { wager, .. }) if wager == test_wager
        ));
    }

    #[test]
    fn GIVEN_wager_more_than_have_WHEN_set_wager_THEN_error() {
        // GIVEN
        let (mut player, _) = new_jeopardy_player();
        let test_points = 0;
        player.points = test_points;
        let test_wager = 1;

        // WHEN
        let result = player.set_wager(test_wager);

        // THEN
        assert!(matches!(
            result,
            Err(JeopardyPlayerError::InvalidWager { wager, current_points }) if wager == test_wager && current_points == test_points
        ));
    }

    #[tokio::test]
    async fn GIVEN_JeopardyPlayerEvent_WHEN_send_THEN_ok() {
        // GIVEN
        let (mut player, mut rx) = new_jeopardy_player();
        let event = JeopardyPlayerEvent::PointsUpdate(player.points);

        // WHEN
        player.send(event).await.unwrap();

        // THEN
        assert!(matches!(
            rx.recv().await.unwrap(),
            JeopardyPlayerEvent::PointsUpdate(points) if points == player.points,
        ));
    }

    #[tokio::test]
    async fn GIVEN_dropped_recv_WHEN_send_THEN_error() {
        // GIVEN
        let (mut player, _) = new_jeopardy_player();
        let event = JeopardyPlayerEvent::PointsUpdate(player.points);

        // WHEN
        let result = player.send(event).await;

        // THEN
        assert!(matches!(result, Err(JeopardyPlayerError::ConnectionLost)));
    }
}
