use std::collections::VecDeque;

use stagecrew::{
    lobby::Game,
    player::{Player, ReadPlayerCollection, player_map::PlayerMap},
};

use crate::game::{
    JeopardyCommand, JeopardyCommandResponse, JeopardyError,
    commands::player::JeopardyDisplayEvent, jeopardy::config::JeopardyConfig,
    player::JeopardyPlayer,
};

#[cfg(test)]
use crate::server::TestDefault;

/// `Jeopardy` is the top level struct encapsulating the entire game for a `Lobby` (stagecrew).
/// It contains the entire game state, the board configuration, final jeopardy, buzzer queue etc.
/// Everything needed to manage an instance of the game of Jeopardy.
pub struct Jeopardy {
    host_password: String,
    display: JeopardyDisplayEvent,
    config: JeopardyConfig,
    // board index, category index, question index
    current_question: (usize, usize, usize),
    buzzer_queue: VecDeque<String>, // vec of IDs
}

/// impl Game for Jeopardy allows us to give a hook into `Lobby`
/// this requires a sync context
// note: this is in this file bc it needs to have access to private functions
impl Game for Jeopardy {
    type Player = JeopardyPlayer;
    type Collection = PlayerMap<Self::Player>;
    type Event = JeopardyCommand;
    type EventResponse = Result<JeopardyCommandResponse, JeopardyError>;

    fn handle_event(
        &mut self,
        players: &mut dyn ReadPlayerCollection<Self::Player>,
        event: Self::Event,
    ) -> Self::EventResponse {
        let result = match event {
            JeopardyCommand::Host {
                host_password,
                command,
            } => JeopardyCommandResponse::Host(todo!()),
            JeopardyCommand::Player { player_id, command } => {
                JeopardyCommandResponse::Player(todo!())
            }
        };
        Ok(result)
    }
}

// all of these functions are private and internal
// bc the lobby would pass the message/command from public API
// down to the `handle_event(..)` function
impl Jeopardy {
    pub fn new(host_password: &str, config: JeopardyConfig) -> Result<Self, JeopardyError> {
        let display = config
            .boards()
            .get(0) // with the guarantees of `JeopardyConfig` this is rare
            .ok_or(JeopardyError::GameBoardsNotFound)?
            .redacted();
        let game = Self {
            host_password: host_password.to_string(),
            display: JeopardyDisplayEvent::Board(display),
            config,
            current_question: (0, 0, 0),
            buzzer_queue: VecDeque::new(),
        };
        Ok(game)
    }

    fn check_password(&self, host_password: &str) -> bool {
        self.host_password == host_password
    }

    fn add_player_to_buzzer_queue(
        &mut self,
        players: &dyn ReadPlayerCollection<JeopardyPlayer>,
        id: String,
    ) -> Result<(), JeopardyError> {
        if let JeopardyDisplayEvent::TextCard { .. } = self.display {
            // if there is no question shown, buzzing just no-ops
            if !players.contains(&id) {
                return Err(JeopardyError::PlayerForGivenIDNotFound(id));
            }
            self.buzzer_queue.push_back(id);
        }
        Ok(())
    }

    fn clear_buzzer_queue(&mut self) {
        self.buzzer_queue.clear();
    }

    /// From a `ReadPlayerCollection<..>`, aka a collection of `JeopardyPlayers`,
    /// creates a vec of tuples representing a player ID and their points (sorted descending).
    /// This relies on `sort_unstable_by` and therefore is `O(n * log(n))`.
    /// Using this is fine because `n` players is always going to be small (`n < 10`) for a lobby instance.
    fn scoreboard(players: &dyn ReadPlayerCollection<JeopardyPlayer>) -> Vec<(i32, String)> {
        let mut scoreboard = players
            .iter()
            .map(|p| (p.points, p.id().to_string()))
            .collect::<Vec<_>>();
        scoreboard.sort_unstable_by(|(a_points, _), (b_points, _)| b_points.cmp(a_points));
        scoreboard
    }
}

#[cfg(test)]
impl TestDefault for Jeopardy {
    fn test_default() -> Self {
        // GIVEN
        let host_password = "test_host_password";
        let config = JeopardyConfig::test_default();

        // WHEN
        let jeopardy = Self::new(host_password, config.clone()).unwrap();

        // THEN
        assert!(jeopardy.check_password(host_password));
        assert_eq!(jeopardy.config, config);
        let JeopardyDisplayEvent::Board(ref board) = jeopardy.display else {
            panic!("Default display event was not of variant JeopardyDisplayEvent::Board");
        };
        assert!(board.is_redacted());

        jeopardy
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod jeopardy_handler_tests {
    use stagecrew::player::{
        Player, ReadPlayerCollection, WritePlayerCollection, player_map::PlayerMap,
    };
    use tokio::sync::mpsc;

    use crate::{
        game::{
            Jeopardy, JeopardyError, commands::player::JeopardyDisplayEvent,
            jeopardy::config::JeopardyConfig, player::JeopardyPlayer,
        },
        server::TestDefault,
    };

    #[test]
    fn GIVEN_jeopardy_handler_WHEN_new_THEN_ok() {
        Jeopardy::test_default();
    }

    #[test]
    fn GIVEN_empty_jeopardy_handler_WHEN_new_THEN_ok() {
        // GIVEN
        let invalid_config = JeopardyConfig::invalid_default();

        // WHEN
        let result = Jeopardy::new("", invalid_config);

        // THEN
        assert!(matches!(result, Err(JeopardyError::GameBoardsNotFound)));
    }

    #[test]
    fn GIVEN_player_collection_WHEN_scoreboard_THEN_ok() {
        // GIVEN
        let n = 10usize;
        let mut players = PlayerMap::new();
        for i in 0..n {
            let (tx, _) = mpsc::channel(1);
            let id = i.to_string();
            let mut player = JeopardyPlayer::new(id.clone(), tx);
            player.points = i as i32;
            players.add(id, player);
        }

        // WHEN
        let scoreboard = Jeopardy::scoreboard(&mut players);

        // THEN
        assert_eq!(scoreboard.len(), n); // ensure same size
        for i in 0..scoreboard.len() - 1 {
            let (a_points, _) = scoreboard[i];
            let (b_points, _) = scoreboard[i + 1];
            assert!(a_points > b_points); // ensure descending in terms of points
        }
    }

    /// given a count `n`, creates adds players to a player map with an id from 1-10 (inclusive)
    fn new_test_jeopardy_player_map(n: usize) -> PlayerMap<JeopardyPlayer> {
        let mut players = PlayerMap::new();
        for i in 1..=n {
            let (tx, _) = mpsc::channel(1);
            let id = i.to_string();
            let player = JeopardyPlayer::new(id.clone(), tx);
            players.add(id, player);
        }
        players
    }

    #[test]
    fn GIVEN_player_id_WHEN_add_player_to_buzzer_queue_THEN_ok() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default();
        jeopardy.display = JeopardyDisplayEvent::TextCard { // ensure buzzing doesn't no-op
            title: "".to_string(),
            content: "".to_string(),
        };
        let n = 10;
        let players = new_test_jeopardy_player_map(n);
        assert!(jeopardy.buzzer_queue.is_empty()); // ensure empty
        let id_order = players.iter().map(|p| p.id()).collect::<Vec<_>>();

        // WHEN
        for id in id_order.iter() {
            // queue in order
            jeopardy
                .add_player_to_buzzer_queue(&players, id.to_string())
                .unwrap();
        }

        // THEN
        assert_eq!(jeopardy.buzzer_queue.len(), id_order.len()); // ensure queued length is the same as player length
        for expected_id in id_order {
            let id = jeopardy.buzzer_queue.pop_front().unwrap();
            assert_eq!(id, expected_id); // ensure that queue order matches id order
        }
    }

    #[test]
    fn GIVEN_non_text_display_WHEN_add_player_to_buzzer_queue_THEN_ok() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default(); // no text display, default JeopardyDisplayEvent::Board
        let players = new_test_jeopardy_player_map(10);
        assert!(jeopardy.buzzer_queue.is_empty());

        // WHEN
        for player in players.iter() {
            // should be no-ops
            jeopardy
                .add_player_to_buzzer_queue(&players, player.id().to_string())
                .unwrap();
        }

        // THEN - should stay empty
        assert!(jeopardy.buzzer_queue.is_empty());
    }

    #[test]
    fn GIVEN_non_empty_buzzer_queue_WHEN_clear_buzzer_queue_THEN_ok() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default();
        jeopardy.display = JeopardyDisplayEvent::TextCard {
            title: "".to_string(),
            content: "".to_string(),
        };
        let n = 10;
        let players = new_test_jeopardy_player_map(n);
        for player in players.iter() {
            jeopardy
                .add_player_to_buzzer_queue(&players, player.id().to_string())
                .unwrap();
        }
        assert_eq!(n, jeopardy.buzzer_queue.len()); // ensure non-empty

        // WHEN
        jeopardy.clear_buzzer_queue();

        // THEN
        assert!(jeopardy.buzzer_queue.is_empty());
    }

    #[test]
    fn GIVEN_invalid_player_id_WHEN_add_player_to_buzzer_queue_THEN_error() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default();
        jeopardy.display = JeopardyDisplayEvent::TextCard {
            title: "".to_string(),
            content: "".to_string(),
        };
        let n = 10;
        let players = new_test_jeopardy_player_map(n);
        assert!(jeopardy.buzzer_queue.is_empty()); // ensure empty
        let invalid_id = "11".to_string();

        // WHEN
        let result = jeopardy.add_player_to_buzzer_queue(&players, invalid_id.clone());

        // THEN
        assert!(jeopardy.buzzer_queue.is_empty()); // ensure still empty
        assert!(matches!(
            result,
            Err(JeopardyError::PlayerForGivenIDNotFound(id)) if id == invalid_id
        ))
    }
}
