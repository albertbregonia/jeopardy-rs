use std::collections::VecDeque;

use stagecrew::{
    lobby::Game,
    player::{ReadPlayerCollection, player_map::PlayerMap},
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
    use crate::{game::Jeopardy, server::TestDefault};

    #[test]
    fn GIVEN_jeopardy_handler_WHEN_new_THEN_ok() {
        Jeopardy::test_default();
    }
}
