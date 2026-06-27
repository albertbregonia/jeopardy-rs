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
