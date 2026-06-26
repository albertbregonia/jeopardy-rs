use stagecrew::{
    lobby::Game,
    player::{ReadPlayers, player_map::PlayerMap},
};

use crate::game::{
    JeopardyCommand, JeopardyCommandResponse, JeopardyError, player::JeopardyPlayer,
};

pub struct Jeopardy {}

impl Game for Jeopardy {
    type Player = JeopardyPlayer;
    type Collection = PlayerMap<Self::Player>;
    type Event = JeopardyCommand;
    type EventResponse = Result<JeopardyCommandResponse, JeopardyError>;

    fn handle_event(
        &mut self,
        players: &mut dyn ReadPlayers<Self::Player>,
        event: Self::Event,
    ) -> Self::EventResponse {
        todo!()
    }
}
