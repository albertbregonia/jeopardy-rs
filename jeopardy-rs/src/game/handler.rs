use std::collections::VecDeque;

use stagecrew::{
    lobby::Game,
    player::{Player, ReadPlayerCollection, player_map::PlayerMap},
};

use crate::game::{
    JeopardyCommand, JeopardyCommandResponse, JeopardyError,
    commands::{
        host::{HostCommand, HostCommandResponse},
        player::{JeopardyDisplayEvent, PlayerCommand, PlayerCommandResponse},
    },
    jeopardy::{
        board::Board, board_question::BoardQuestion, category::Category, config::JeopardyConfig,
    },
    player::{JeopardyPlayer, JeopardyPlayerEvent},
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
            } => JeopardyCommandResponse::Host(self.handle_host_command(
                players,
                host_password,
                command,
            )?),
            JeopardyCommand::Player { player_id, command } => JeopardyCommandResponse::Player(
                self.handle_player_command(players, player_id, command)?,
            ),
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
            .first() // with the guarantees of `JeopardyConfig` this is rare
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

    fn handle_host_command(
        &mut self,
        players: &mut dyn ReadPlayerCollection<JeopardyPlayer>,
        host_password: String,
        command: HostCommand,
    ) -> Result<HostCommandResponse, JeopardyError> {
        if !self.check_password(&host_password) {
            return Err(JeopardyError::IncorrectHostPassword);
        }
        let result = match command {
            HostCommand::ShowBoard { board_index } => {
                self.show_board(players, board_index)?;
                HostCommandResponse::Success
            }
            HostCommand::ShowQuestion {
                board_index,
                category_index,
                question_index,
            } => {
                self.show_question(players, board_index, category_index, question_index)?;
                HostCommandResponse::Success
            }
            HostCommand::ShowCurrentAnswer => {
                self.show_current_answer(players)?;
                HostCommandResponse::Success
            }
            HostCommand::SetPoints { player_id, points } => {
                players.set_points_for_player(player_id, points)?;
                HostCommandResponse::Success
            }
            HostCommand::UpdatePoints { player_id, delta } => {
                let points = players.update_points_for_player(player_id, delta)?;
                HostCommandResponse::UpdatePoints(points)
            }
            HostCommand::GetAnswer {
                board_index,
                category_index,
                question_index,
            } => HostCommandResponse::GetAnswer(self.get_answer(
                board_index,
                category_index,
                question_index,
            )?),
            HostCommand::ShowFinalJeopardyQuestion => {
                self.show_final_jeopardy_question(players);
                HostCommandResponse::Success
            }
            HostCommand::ShowFinalJeopardyAnswer => {
                self.show_final_jeopardy_answer(players);
                HostCommandResponse::Success
            }
            HostCommand::ClearBuzzerQueue => {
                self.clear_buzzer_queue();
                HostCommandResponse::Success
            }
            HostCommand::GetBuzzerQueue => {
                HostCommandResponse::GetBuzzerQueue(self.buzzer_queue.clone())
            }
        };
        Ok(result)
    }

    fn handle_player_command(
        &mut self,
        players: &mut dyn ReadPlayerCollection<JeopardyPlayer>,
        player_id: String,
        command: PlayerCommand,
    ) -> Result<PlayerCommandResponse, JeopardyError> {
        let player = players
            .get_mut(&player_id)
            .ok_or(JeopardyError::PlayerForGivenIDNotFound(player_id.clone()))?;
        let result = match command {
            PlayerCommand::Buzz => {
                self.add_player_to_buzzer_queue(players, player_id)?;
                PlayerCommandResponse::Success
            }
            PlayerCommand::Refresh => PlayerCommandResponse::Refresh(self.display.clone()),
            PlayerCommand::GetPoints => PlayerCommandResponse::GetPoints(player.points),
            PlayerCommand::GetWager => PlayerCommandResponse::GetWager(player.wager()),
            PlayerCommand::SetWager(wager) => {
                player.set_wager(wager)?;
                PlayerCommandResponse::Success
            }
            PlayerCommand::GetFreeResponse => {
                PlayerCommandResponse::GetFreeResponse(player.free_response.clone())
            }
            PlayerCommand::SetFreeResponse(free_response) => {
                // TODO: some sort of validation
                player.free_response = free_response;
                PlayerCommandResponse::Success
            }
            PlayerCommand::GetScoreboard => {
                PlayerCommandResponse::GetScoreboard(players.scoreboard())
            }
        };
        Ok(result)
    }

    fn check_password(&self, host_password: &str) -> bool {
        self.host_password == host_password
    }

    // small helper functions to manage the internal state

    fn add_player_to_buzzer_queue(
        &mut self,
        players: &dyn ReadPlayerCollection<JeopardyPlayer>,
        player_id: String,
    ) -> Result<(), JeopardyError> {
        if let JeopardyDisplayEvent::TextCard { .. } = self.display {
            // if there is no question shown, buzzing just no-ops
            if !players.contains(&player_id) {
                return Err(JeopardyError::PlayerForGivenIDNotFound(player_id));
            }
            self.buzzer_queue.push_back(player_id);
        }
        Ok(())
    }

    fn clear_buzzer_queue(&mut self) {
        self.buzzer_queue.clear();
    }

    fn set_board(&mut self, board_index: usize) -> Result<(), JeopardyError> {
        self.set_question(board_index, 0, 0)?;
        Ok(())
    }

    fn set_question(
        &mut self,
        board_index: usize,
        category_index: usize,
        question_index: usize,
    ) -> Result<(), JeopardyError> {
        self.get_question(
            board_index, // validate only, we don't need the response
            category_index,
            question_index,
        )?;
        self.current_question = (
            board_index, // set the current question indices
            category_index,
            question_index,
        );
        Ok(())
    }

    // helper functions to interface with the game boards

    fn get_question(
        &self,
        board_index: usize,
        category_index: usize,
        question_index: usize,
    ) -> Result<(&Board, &Category, &BoardQuestion), JeopardyError> {
        let board = self
            .config
            .boards()
            .get(board_index)
            .ok_or(JeopardyError::InvalidBoardIndex(board_index))?;
        let category = board
            .categories()
            .get(category_index)
            .ok_or(JeopardyError::InvalidCategoryIndex(category_index))?;
        let question = category
            .questions()
            .get(question_index)
            .ok_or(JeopardyError::InvalidQuestionIndex(question_index))?;
        Ok((board, category, question))
    }

    fn get_mut_question(
        &mut self,
        board_index: usize,
        category_index: usize,
        question_index: usize,
    ) -> Result<&mut BoardQuestion, JeopardyError> {
        let board = self
            .config
            .boards_mut()
            .get_mut(board_index)
            .ok_or(JeopardyError::InvalidBoardIndex(board_index))?;
        let category = board
            .categories_mut()
            .get_mut(category_index)
            .ok_or(JeopardyError::InvalidCategoryIndex(category_index))?;
        let question = category
            .questions_mut()
            .get_mut(question_index)
            .ok_or(JeopardyError::InvalidQuestionIndex(question_index))?;
        Ok(question)
    }

    // primary use case is for the host to see the answer
    fn get_answer(
        &self,
        board_index: usize,
        category_index: usize,
        question_index: usize,
    ) -> Result<String, JeopardyError> {
        let (_board, _category, question) =
            self.get_question(board_index, category_index, question_index)?;
        Ok(question.underlying().answer().to_string())
    }

    // high level helper functions
    // these abstractions represent actions to play the game of Jeopardy
    // not just manage internal state

    fn show_board(
        &mut self,
        players: &dyn ReadPlayerCollection<JeopardyPlayer>,
        board_index: usize,
    ) -> Result<(), JeopardyError> {
        self.set_board(board_index)?;
        players.broadcast(&self.display);
        Ok(())
    }

    fn show_question(
        &mut self,
        players: &dyn ReadPlayerCollection<JeopardyPlayer>,
        board_index: usize,
        category_index: usize,
        question_index: usize,
    ) -> Result<(), JeopardyError> {
        let (_, category, question) =
            self.get_question(board_index, category_index, question_index)?;
        // create text display from question
        self.display = JeopardyDisplayEvent::TextCard {
            title: category.name().to_string(),
            content: question.underlying().content().to_string(),
        };
        self.set_question(board_index, category_index, question_index)?;

        // clear buzzer before showing to ensure no pre-emptive buzz
        self.clear_buzzer_queue();
        players.broadcast(&self.display);
        Ok(())
    }

    // technically infallible but the compiler cannot guarantee
    fn show_current_answer(
        &mut self,
        players: &dyn ReadPlayerCollection<JeopardyPlayer>,
    ) -> Result<(), JeopardyError> {
        let (board_index, category_index, question_index) = self.current_question;
        let question = self.get_mut_question(board_index, category_index, question_index)?;
        question.set_answered(true);
        let question = question.underlying();
        self.display = JeopardyDisplayEvent::TextCard {
            title: question.content().to_string(),
            content: question.answer().to_string(),
        };
        self.clear_buzzer_queue();
        players.broadcast(&self.display);
        Ok(())
    }

    fn show_final_jeopardy_question(&mut self, players: &dyn ReadPlayerCollection<JeopardyPlayer>) {
        let final_jeopardy = self.config.final_jeopardy();
        self.display = JeopardyDisplayEvent::TextCard {
            title: final_jeopardy.hint().to_string(),
            content: final_jeopardy.question().content().to_string(),
        };
        players.broadcast(&self.display);
    }

    fn show_final_jeopardy_answer(&mut self, players: &dyn ReadPlayerCollection<JeopardyPlayer>) {
        let final_jeopardy = self.config.final_jeopardy().question();
        self.display = JeopardyDisplayEvent::TextCard {
            title: final_jeopardy.content().to_string(),
            content: final_jeopardy.answer().to_string(),
        };
        players.broadcast(&self.display);
    }
}

/// - internal trait just to make the code look more idiomatic
///   instead of having to pass &mut dyn ReadPlayersCollection<_> everywhere
///
/// these are Jeopardy operations that do not update state
/// and just interface with the player collection
trait JeopardyPlayerCollectionOperation {
    fn scoreboard(&self) -> Vec<(i32, String)>;
    fn broadcast(&self, event: &JeopardyDisplayEvent);
    fn set_points_for_player(
        &mut self,
        player_id: String,
        points: i32,
    ) -> Result<(), JeopardyError>;
    fn update_points_for_player(
        &mut self,
        player_id: String,
        delta: i32,
    ) -> Result<i32, JeopardyError>;
}

impl<T: ReadPlayerCollection<JeopardyPlayer> + ?Sized> JeopardyPlayerCollectionOperation for T {
    /// From a `ReadPlayerCollection<..>`, aka a collection of `JeopardyPlayers`,
    /// creates a vec of tuples representing a player ID and their points (sorted descending).
    /// This relies on `sort_unstable_by` and therefore is `O(n * log(n))`.
    /// Using this is fine because `n` players is always going to be small (`n < 10`) for a lobby instance.
    fn scoreboard(&self) -> Vec<(i32, String)> {
        let mut scoreboard = self
            .iter()
            .map(|p| (p.points, p.id().to_string()))
            .collect::<Vec<_>>();
        scoreboard.sort_unstable_by(|(a_points, _), (b_points, _)| b_points.cmp(a_points));
        scoreboard
    }

    fn broadcast(&self, event: &JeopardyDisplayEvent) {
        // here, we don't care about if the broadcast fails.
        // the actor lobby at the higher level will handle
        // if the player's disconnects or the recv handle is dropped

        // therefore, we can use a bunch of short lived tokio tasks
        // to broadcast the display event to everyone
        for p in self.iter() {
            p.send_background(JeopardyPlayerEvent::Display(event.clone()));
        }
    }

    // helepr functions to update player state

    fn set_points_for_player(
        &mut self,
        player_id: String,
        points: i32,
    ) -> Result<(), JeopardyError> {
        let player = self
            .get_mut(&player_id)
            .ok_or(JeopardyError::PlayerForGivenIDNotFound(player_id))?;
        player.points = points;
        // notify the player that their points have changed
        player.send_background(JeopardyPlayerEvent::PointsUpdate(player.points));
        Ok(())
    }

    fn update_points_for_player(
        &mut self,
        player_id: String,
        delta: i32,
    ) -> Result<i32, JeopardyError> {
        let player = self
            .get_mut(&player_id)
            .ok_or(JeopardyError::PlayerForGivenIDNotFound(player_id))?;
        player.points += delta;
        // notify the player that their points have changed
        player.send_background(JeopardyPlayerEvent::PointsUpdate(player.points));
        Ok(player.points)
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
mod handler_tests {
    use stagecrew::{
        lobby::Game,
        player::{Player, ReadPlayerCollection, WritePlayerCollection, player_map::PlayerMap},
    };
    use tokio::sync::mpsc;

    use super::JeopardyPlayerCollectionOperation;
    use crate::{
        game::{
            Jeopardy, JeopardyCommand, JeopardyCommandResponse, JeopardyError,
            commands::{
                host::{HostCommand, HostCommandResponse},
                player::{JeopardyDisplayEvent, PlayerCommand, PlayerCommandResponse},
            },
            jeopardy::{board::Board, config::JeopardyConfig, final_jeopardy::FinalJeopardy},
            player::{JeopardyPlayer, JeopardyPlayerError, JeopardyPlayerEvent},
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
        let host_password = ""; // no validation

        // WHEN
        let result = Jeopardy::new(host_password, invalid_config);

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
        let scoreboard = players.scoreboard();

        // THEN
        assert_eq!(scoreboard.len(), n); // ensure same size
        for i in 0..scoreboard.len() - 1 {
            let (a_points, _) = scoreboard[i];
            let (b_points, _) = scoreboard[i + 1];
            assert!(a_points > b_points); // ensure descending in terms of points
        }
    }

    /// given a count `n`, creates adds players to a player map with an id from 1-10 (inclusive)
    /// returns that map with all mpsc::Receiver<_> handlers
    fn new_test_jeopardy_player_map(
        n: usize,
    ) -> (
        PlayerMap<JeopardyPlayer>,
        Vec<mpsc::Receiver<JeopardyPlayerEvent>>,
    ) {
        let mut players = PlayerMap::new();
        let mut receivers = vec![];
        for i in 1..=n {
            let (tx, rx) = mpsc::channel(1);
            let id = i.to_string();
            let player = JeopardyPlayer::new(id.clone(), tx);
            players.add(id, player);
            receivers.push(rx);
        }
        (players, receivers)
    }

    #[test]
    fn GIVEN_player_id_WHEN_add_player_to_buzzer_queue_THEN_ok() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default();
        jeopardy.display = JeopardyDisplayEvent::TextCard {
            // ensure buzzing doesn't no-op
            title: "".to_string(),
            content: "".to_string(),
        };
        let n = 10;
        let (players, _) = new_test_jeopardy_player_map(n);
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
        let (players, _) = new_test_jeopardy_player_map(10);
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
        let (players, _) = new_test_jeopardy_player_map(n);
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
        let (players, _) = new_test_jeopardy_player_map(n);
        assert!(jeopardy.buzzer_queue.is_empty()); // ensure empty
        let invalid_id = "11".to_string();

        // WHEN
        let result = jeopardy.add_player_to_buzzer_queue(&players, invalid_id.clone());

        // THEN
        assert!(jeopardy.buzzer_queue.is_empty()); // ensure still empty
        assert!(matches!(
            result,
            Err(JeopardyError::PlayerForGivenIDNotFound(id)) if id == invalid_id
        ));
    }

    #[test]
    fn GIVEN_valid_indices_WHEN_get_question_and_answer_THEN_ok() {
        // GIVEN
        let boards = vec![
            // create a jeopardy game with 2 boards and dims
            Board::test_default_from_counts(1, 1),
            Board::test_default_from_counts(10, 10),
        ];
        let config = JeopardyConfig::new(boards, FinalJeopardy::test_default()).unwrap();
        let host_password = "";
        let mut jeopardy = Jeopardy::new(host_password, config).unwrap();

        // WHEN
        for board_index in 0..jeopardy.config.boards().len() {
            let category_len = jeopardy.config.boards()[board_index].categories().len();
            for category_index in 0..category_len {
                let question_len = jeopardy.config.boards()[board_index].categories()
                    [category_index]
                    .questions()
                    .len();
                for question_index in 0..question_len {
                    // lookup question
                    let (board, category, question) = jeopardy
                        .get_question(board_index, category_index, question_index)
                        .unwrap();

                    // THEN
                    let expected_board = &jeopardy.config.boards()[board_index];
                    let expected_category = &expected_board.categories()[category_index];
                    let expected_question = &expected_category.questions()[question_index];
                    assert_eq!(board, expected_board); // assert manual lookup matches the function
                    assert_eq!(category, expected_category);
                    assert_eq!(question, expected_question);

                    // ensure that the answer is the same
                    // note: normally this should be a separate test but setup is tedious
                    let answer = jeopardy
                        .get_answer(board_index, category_index, question_index)
                        .unwrap();
                    assert_eq!(question.underlying().answer(), answer);

                    // ensure that mut question returns the same
                    let mut question = question.clone();
                    let mut_question = jeopardy
                        .get_mut_question(board_index, category_index, question_index)
                        .unwrap();
                    assert_eq!(&mut question, mut_question); // won't compile if not mut
                }
            }
        }
    }

    #[test]
    fn GIVEN_invalid_indices_WHEN_get_question_THEN_error() {
        // GIVEN
        let jeopardy = Jeopardy::test_default(); // default 1 category, 1 question
        let invalid_index = 10;

        // WHEN
        let invalid_board_index_result = jeopardy.get_question(invalid_index, 0, 0);
        let invalid_category_index_result = jeopardy.get_question(0, invalid_index, 0);
        let invalid_question_index_result = jeopardy.get_question(0, 0, invalid_index);

        // THEN
        assert!(matches!(
            invalid_board_index_result,
            Err(JeopardyError::InvalidBoardIndex(index)) if index == invalid_index
        ));
        assert!(matches!(
            invalid_category_index_result,
            Err(JeopardyError::InvalidCategoryIndex(index)) if index == invalid_index
        ));
        assert!(matches!(
            invalid_question_index_result,
            Err(JeopardyError::InvalidQuestionIndex(index)) if index == invalid_index
        ));
    }

    #[test]
    fn GIVEN_invalid_indices_WHEN_get_mut_question_THEN_error() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default(); // default 1 category, 1 question
        let invalid_index = 10;

        // WHEN / THEN
        assert!(matches!(
            jeopardy.get_mut_question(invalid_index, 0, 0),
            Err(JeopardyError::InvalidBoardIndex(index)) if index == invalid_index
        ));
        assert!(matches!(
            jeopardy.get_mut_question(0, invalid_index, 0),
            Err(JeopardyError::InvalidCategoryIndex(index)) if index == invalid_index
        ));
        assert!(matches!(
            jeopardy.get_mut_question(0, 0, invalid_index),
            Err(JeopardyError::InvalidQuestionIndex(index)) if index == invalid_index
        ));
    }

    #[test]
    fn GIVEN_invalid_indices_WHEN_get_answer_THEN_error() {
        // GIVEN
        let jeopardy = Jeopardy::test_default(); // default 1 category, 1 question
        let invalid_index = 10;

        // WHEN
        let invalid_board_index_result = jeopardy.get_answer(invalid_index, 0, 0);
        let invalid_category_index_result = jeopardy.get_answer(0, invalid_index, 0);
        let invalid_question_index_result = jeopardy.get_answer(0, 0, invalid_index);

        // THEN
        assert!(matches!(
            invalid_board_index_result,
            Err(JeopardyError::InvalidBoardIndex(index)) if index == invalid_index
        ));
        assert!(matches!(
            invalid_category_index_result,
            Err(JeopardyError::InvalidCategoryIndex(index)) if index == invalid_index
        ));
        assert!(matches!(
            invalid_question_index_result,
            Err(JeopardyError::InvalidQuestionIndex(index)) if index == invalid_index
        ));
    }

    // creates a new PlayerMap<JeopardyPlayer> with a single player
    // given their ID and a `player_setup()` function to modify the player's state
    // before adding them to the map.
    // returns map and the mpsc::Receiver<_> handle that the player would receive events on
    fn new_test_player_map_with_setup<F>(
        player_id: String,
        player_setup: F,
    ) -> (
        PlayerMap<JeopardyPlayer>,
        mpsc::Receiver<JeopardyPlayerEvent>,
    )
    where
        F: FnOnce(&mut JeopardyPlayer),
    {
        let mut players = PlayerMap::new();
        let (tx, rx) = mpsc::channel(1);
        let mut player = JeopardyPlayer::new(player_id.clone(), tx);
        player_setup(&mut player);
        players.add(player_id.clone(), player);
        (players, rx)
    }

    #[tokio::test]
    async fn GIVEN_points_WHEN_set_points_for_player_THEN_ok() {
        // GIVEN
        let player_id = "test".to_string();
        let (mut players, mut rx) = new_test_player_map_with_setup(player_id.clone(), |player| {
            player.points = 100; // should get overwritten
        });

        let expected_points = 10;

        // WHEN
        players
            .set_points_for_player(player_id.clone(), expected_points)
            .unwrap();

        // THEN
        let player = players.get(&player_id).unwrap();
        assert_eq!(expected_points, player.points);
        assert!(matches!(
            rx.recv().await.unwrap(),
            JeopardyPlayerEvent::PointsUpdate(update) if update == expected_points
        ));
    }

    #[test]
    fn GIVEN_invalid_player_id_WHEN_set_points_for_player_THEN_error() {
        // GIVEN
        let expected_points = 100;
        let player_id = "test".to_string();
        let (mut players, _) = new_test_player_map_with_setup(player_id.clone(), |player| {
            player.points = expected_points;
        });

        let invalid_id = "invalid".to_string(); // not in map
        let points = 10; // dummy value

        // WHEN
        let result = players.set_points_for_player(invalid_id.clone(), points);

        // THEN
        let player = players.get(&player_id).unwrap();
        assert_eq!(expected_points, player.points); // should not change
        assert!(matches!(
            result,
            Err(JeopardyError::PlayerForGivenIDNotFound(id)) if id == invalid_id
        ));
    }

    #[tokio::test]
    async fn GIVEN_delta_WHEN_update_points_for_player_THEN_ok() {
        // GIVEN
        let player_id = "test".to_string();
        let init_points = 100;
        let delta = -10;
        let expected_points = init_points + delta;
        let (mut players, mut rx) = new_test_player_map_with_setup(player_id.clone(), |player| {
            player.points = init_points;
        });

        // WHEN
        players
            .update_points_for_player(player_id.clone(), delta)
            .unwrap();

        // THEN
        let player = players.get(&player_id).unwrap();
        assert_eq!(expected_points, player.points);
        assert!(matches!(
            rx.recv().await.unwrap(),
            JeopardyPlayerEvent::PointsUpdate(update) if update == expected_points
        ));
    }

    #[test]
    fn GIVEN_invalid_player_id_WHEN_update_points_for_player_THEN_error() {
        // GIVEN
        let player_id = "test".to_string();
        let expected_points = 100;
        let (mut players, _) = new_test_player_map_with_setup(player_id.clone(), |player| {
            player.points = expected_points;
        });

        let invalid_id = "invalid".to_string(); // not in map
        let delta = -10; // dummy value

        // WHEN
        let result = players.update_points_for_player(invalid_id.clone(), delta);

        // THEN
        let player = players.get(&player_id).unwrap();
        assert_eq!(expected_points, player.points); // should not change
        assert!(matches!(
            result,
            Err(JeopardyError::PlayerForGivenIDNotFound(id)) if id == invalid_id
        ));
    }

    #[tokio::test] // there is no negative test for this bc `broadcast()` is fire and forget
    async fn GIVEN_display_event_WHEN_broadcast_THEN_ok() {
        // GIVEN
        let (players, receivers) = new_test_jeopardy_player_map(10);
        let expected_title = "title".to_string();
        let expected_content = "content".to_string();
        let event = JeopardyDisplayEvent::TextCard {
            title: expected_title.clone(),
            content: expected_content.clone(),
        };

        // WHEN
        players.broadcast(&event); // this operation is infallible, fire and forget

        // THEN
        for mut rx in receivers {
            assert!(matches!(
                rx.recv().await.unwrap(), // ensure we receive the broadcast
                JeopardyPlayerEvent::Display(JeopardyDisplayEvent::TextCard { title, content })
                    if title == expected_title && content == expected_content
            ));
        }
    }

    #[test]
    fn GIVEN_valid_indices_WHEN_set_question_THEN_ok() {
        // GIVEN
        let boards = vec![
            // create a jeopardy game with 2 boards and dims
            Board::test_default_from_counts(1, 1),
            Board::test_default_from_counts(10, 10),
        ];
        let config = JeopardyConfig::new(boards, FinalJeopardy::test_default()).unwrap();
        let host_password = "";
        let mut jeopardy = Jeopardy::new(host_password, config).unwrap();

        // WHEN
        // goes through every question and set it as the current question
        for expected_board_index in 0..jeopardy.config.boards().len() {
            let category_len = jeopardy.config.boards()[expected_board_index]
                .categories()
                .len();
            for expected_category_index in 0..category_len {
                let question_len = jeopardy.config.boards()[expected_board_index].categories()
                    [expected_category_index]
                    .questions()
                    .len();
                for expected_question_index in 0..question_len {
                    jeopardy
                        .set_question(
                            expected_board_index,
                            expected_category_index,
                            expected_question_index,
                        )
                        .unwrap();

                    // THEN
                    let (current_board_index, current_category_index, current_question_index) =
                        jeopardy.current_question;
                    assert_eq!(expected_board_index, current_board_index); // assert current question matches expected
                    assert_eq!(expected_category_index, current_category_index);
                    assert_eq!(expected_question_index, current_question_index);
                }
            }
        }
    }

    #[test]
    fn GIVEN_invalid_indices_WHEN_set_question_THEN_error() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default(); // default 1 category, 1 question
        let invalid_index = 10;

        // WHEN
        let invalid_board_index_result = jeopardy.set_question(invalid_index, 0, 0);
        let invalid_category_index_result = jeopardy.set_question(0, invalid_index, 0);
        let invalid_question_index_result = jeopardy.set_question(0, 0, invalid_index);

        // THEN
        assert!(matches!(
            invalid_board_index_result,
            Err(JeopardyError::InvalidBoardIndex(index)) if index == invalid_index
        ));
        assert!(matches!(
            invalid_category_index_result,
            Err(JeopardyError::InvalidCategoryIndex(index)) if index == invalid_index
        ));
        assert!(matches!(
            invalid_question_index_result,
            Err(JeopardyError::InvalidQuestionIndex(index)) if index == invalid_index
        ));
    }

    #[test]
    fn GIVEN_valid_indices_WHEN_set_board_THEN_ok() {
        // GIVEN
        let boards = vec![
            // create a jeopardy game with 2 boards and dims
            Board::test_default_from_counts(1, 1),
            Board::test_default_from_counts(10, 10),
        ];
        let config = JeopardyConfig::new(boards, FinalJeopardy::test_default()).unwrap();
        let host_password = "";
        let mut jeopardy = Jeopardy::new(host_password, config).unwrap();

        // WHEN
        for board_index in 0..jeopardy.config.boards().len() {
            jeopardy.set_board(board_index).unwrap();

            // THEN
            let (current_board_index, current_category_index, current_question_index) =
                jeopardy.current_question;
            // changing the board sets it to category 0 and question 0 (the types ensure non-empty)
            assert_eq!(board_index, current_board_index);
            assert_eq!(0, current_category_index);
            assert_eq!(0, current_question_index);
        }
    }

    #[test]
    fn GIVEN_invalid_index_WHEN_set_board_THEN_error() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default();
        let invalid_board_index = 100;

        // WHEN
        let result = jeopardy.set_board(invalid_board_index);

        // THEN
        assert!(matches!(
            result,
            Err(JeopardyError::InvalidBoardIndex(index))
                if index == invalid_board_index
        ))
    }

    #[tokio::test]
    async fn GIVEN_valid_indices_WHEN_show_board_THEN_ok() {
        // GIVEN
        let boards = vec![
            // create a jeopardy game with 2 boards and dims
            Board::test_default_from_counts(1, 1),
            Board::test_default_from_counts(10, 10),
        ];
        let config = JeopardyConfig::new(boards, FinalJeopardy::test_default()).unwrap();
        let host_password = "";
        let mut jeopardy = Jeopardy::new(host_password, config).unwrap();
        let (players, mut receivers) = new_test_jeopardy_player_map(10);

        // WHEN
        for board_index in 0..jeopardy.config.boards().len() {
            jeopardy.show_board(&players, board_index).unwrap();
            let expected_board = &jeopardy.config.boards()[board_index];

            // THEN

            // ensure internal state is correct
            let (current_board_index, current_category_index, current_question_index) =
                jeopardy.current_question;
            // changing the board sets it to category 0 and question 0 (the types ensure non-empty)
            assert_eq!(board_index, current_board_index);
            assert_eq!(0, current_category_index);
            assert_eq!(0, current_question_index);

            assert!(matches!( // ensure internal cache is correct
                &jeopardy.display,
                JeopardyDisplayEvent::Board(board) if
                    board.is_redacted_version(expected_board)
            ));
            // ensure that players receive the event
            for rx in &mut receivers {
                let JeopardyPlayerEvent::Display(JeopardyDisplayEvent::Board(board)) =
                    rx.recv().await.unwrap()
                else {
                    panic!("Player did not receive show_board() display event");
                };
                assert!(board.is_redacted_version(expected_board));
            }
        }
    }

    #[test]
    fn GIVEN_invalid_index_WHEN_show_board_THEN_error() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default();
        let invalid_board_index = 100;

        // WHEN
        let result = jeopardy.set_board(invalid_board_index);

        // THEN
        assert!(matches!(
            result,
            Err(JeopardyError::InvalidBoardIndex(index))
                if index == invalid_board_index
        ))
    }

    #[tokio::test]
    async fn GIVEN_valid_indices_WHEN_show_question_THEN_ok() {
        let boards = vec![
            // create a jeopardy game with 2 boards and dims
            Board::test_default_from_counts(1, 1),
            Board::test_default_from_counts(10, 10),
        ];
        let config = JeopardyConfig::new(boards, FinalJeopardy::test_default()).unwrap();
        let host_password = "";
        let mut jeopardy = Jeopardy::new(host_password, config).unwrap();
        let (players, mut receivers) = new_test_jeopardy_player_map(10);

        // goes through every question and "shows" the question
        for expected_board_index in 0..jeopardy.config.boards().len() {
            let category_len = jeopardy.config.boards()[expected_board_index]
                .categories()
                .len();
            for expected_category_index in 0..category_len {
                let question_len = jeopardy.config.boards()[expected_board_index].categories()
                    [expected_category_index]
                    .questions()
                    .len();
                for expected_question_index in 0..question_len {
                    // GIVEN
                    let question = &jeopardy.config.boards()[expected_board_index].categories()
                        [expected_category_index]
                        .questions()[expected_question_index];
                    // set buzzer queue to some dummy data to ensure it gets cleared
                    jeopardy.buzzer_queue = (0..10).into_iter().map(|i| i.to_string()).collect();
                    assert_eq!(false, jeopardy.buzzer_queue.is_empty());
                    assert_eq!(false, question.is_answered()); // default !answered

                    // WHEN
                    jeopardy
                        .show_question(
                            &players,
                            expected_board_index,
                            expected_category_index,
                            expected_question_index,
                        )
                        .unwrap();

                    // THEN

                    // ensure internal state is correct
                    let (current_board_index, current_category_index, current_question_index) =
                        jeopardy.current_question;
                    assert_eq!(expected_board_index, current_board_index); // assert current question matches expected
                    assert_eq!(expected_category_index, current_category_index);
                    assert_eq!(expected_question_index, current_question_index);
                    let question = &jeopardy.config.boards()[expected_board_index].categories()
                        [expected_category_index]
                        .questions()[expected_question_index];
                    assert_eq!(false, question.is_answered()); // untouched
                    assert!(jeopardy.buzzer_queue.is_empty()); // cleared

                    // ensure display cache is correct
                    let category = &jeopardy.config.boards()[expected_board_index].categories()
                        [expected_category_index];
                    let expected_title = category.name();
                    let expected_content = question.underlying().content();
                    assert!(matches!(
                        &jeopardy.display,
                        JeopardyDisplayEvent::TextCard{title, content}
                                if title == expected_title && content == expected_content
                    ));

                    // ensure that players receive the text card
                    for rx in &mut receivers {
                        assert!(matches!(
                            rx.recv().await.unwrap(),
                            JeopardyPlayerEvent::Display(JeopardyDisplayEvent::TextCard{title, content})
                                if title == expected_title && content == expected_content
                        ))
                    }
                }
            }
        }
    }

    #[test]
    fn GIVEN_invalid_indices_WHEN_show_question_THEN_error() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default(); // default 1 category, 1 question
        let (players, _) = new_test_jeopardy_player_map(10);
        let invalid_index = 10;

        // WHEN
        let invalid_board_index_result = jeopardy.show_question(&players, invalid_index, 0, 0);
        let invalid_category_index_result = jeopardy.show_question(&players, 0, invalid_index, 0);
        let invalid_question_index_result = jeopardy.show_question(&players, 0, 0, invalid_index);

        // THEN
        assert!(matches!( // ensure unchanged
            &jeopardy.display,
            JeopardyDisplayEvent::Board(board) if
                board.is_redacted_version(&jeopardy.config.boards()[0])
        ));
        assert!(matches!(
            invalid_board_index_result,
            Err(JeopardyError::InvalidBoardIndex(index)) if index == invalid_index
        ));
        assert!(matches!(
            invalid_category_index_result,
            Err(JeopardyError::InvalidCategoryIndex(index)) if index == invalid_index
        ));
        assert!(matches!(
            invalid_question_index_result,
            Err(JeopardyError::InvalidQuestionIndex(index)) if index == invalid_index
        ));
    }

    #[tokio::test]
    async fn GIVEN_valid_indices_WHEN_show_current_answer_THEN_ok() {
        let boards = vec![
            // create a jeopardy game with 2 boards and dims
            Board::test_default_from_counts(1, 1),
            Board::test_default_from_counts(10, 10),
        ];
        let config = JeopardyConfig::new(boards, FinalJeopardy::test_default()).unwrap();
        let host_password = "";
        let mut jeopardy = Jeopardy::new(host_password, config).unwrap();
        let (players, mut receivers) = new_test_jeopardy_player_map(10);

        // goes through every question and "shows" the question
        for expected_board_index in 0..jeopardy.config.boards().len() {
            let category_len = jeopardy.config.boards()[expected_board_index]
                .categories()
                .len();
            for expected_category_index in 0..category_len {
                let question_len = jeopardy.config.boards()[expected_board_index].categories()
                    [expected_category_index]
                    .questions()
                    .len();
                for expected_question_index in 0..question_len {
                    // GIVEN
                    let question = &jeopardy.config.boards()[expected_board_index].categories()
                        [expected_category_index]
                        .questions()[expected_question_index];
                    // set buzzer queue to some dummy data to ensure it gets cleared
                    jeopardy.buzzer_queue = (0..10).into_iter().map(|i| i.to_string()).collect();
                    assert_eq!(false, jeopardy.buzzer_queue.is_empty());
                    assert_eq!(false, question.is_answered()); // default !answered
                    jeopardy
                        .set_question(
                            expected_board_index,
                            expected_category_index,
                            expected_question_index,
                        )
                        .unwrap();

                    // WHEN
                    jeopardy.show_current_answer(&players).unwrap();

                    // THEN

                    // ensure internal state is correct
                    let (current_board_index, current_category_index, current_question_index) =
                        jeopardy.current_question;
                    assert_eq!(expected_board_index, current_board_index); // assert current question matches expected
                    assert_eq!(expected_category_index, current_category_index);
                    assert_eq!(expected_question_index, current_question_index);
                    let question = &jeopardy.config.boards()[expected_board_index].categories()
                        [expected_category_index]
                        .questions()[expected_question_index];
                    assert!(question.is_answered()); // set
                    assert!(jeopardy.buzzer_queue.is_empty()); // cleared

                    // ensure display cache is correct
                    let question = question.underlying();
                    assert!(matches!(
                        &jeopardy.display,
                        JeopardyDisplayEvent::TextCard{title, content}
                            if title == question.content() && content == question.answer()
                    ));

                    // ensure that players receive the text card
                    for rx in &mut receivers {
                        assert!(matches!(
                            rx.recv().await.unwrap(),
                            JeopardyPlayerEvent::Display(JeopardyDisplayEvent::TextCard{title, content})
                                if title == question.content() && content == question.answer()
                        ))
                    }
                }
            }
        }
    }

    #[test]
    fn GIVEN_invalid_indices_WHEN_show_current_answer_THEN_error() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default(); // default 1 category, 1 question
        let (players, _) = new_test_jeopardy_player_map(10);
        let invalid_index = 10;

        // WHEN
        // with the current implementation, this is impossible
        // however, we still test in case future changes break it
        jeopardy.current_question = (invalid_index, 0, 0);
        let invalid_board_index_result = jeopardy.show_current_answer(&players);
        jeopardy.current_question = (0, invalid_index, 0);
        let invalid_category_index_result = jeopardy.show_current_answer(&players);
        jeopardy.current_question = (0, 0, invalid_index);
        let invalid_question_index_result = jeopardy.show_current_answer(&players);

        // THEN
        assert!(matches!( // ensure unchanged
            &jeopardy.display,
            JeopardyDisplayEvent::Board(board) if
                board.is_redacted_version(&jeopardy.config.boards()[0])
        ));
        assert!(matches!(
            invalid_board_index_result,
            Err(JeopardyError::InvalidBoardIndex(index)) if index == invalid_index
        ));
        assert!(matches!(
            invalid_category_index_result,
            Err(JeopardyError::InvalidCategoryIndex(index)) if index == invalid_index
        ));
        assert!(matches!(
            invalid_question_index_result,
            Err(JeopardyError::InvalidQuestionIndex(index)) if index == invalid_index
        ));
    }

    #[tokio::test]
    async fn GIVEN_jeopardy_WHEN_show_final_jeopardy_question_THEN_ok() {
        // infallible - so no negative test
        // GIVEN
        let mut jeopardy = Jeopardy::test_default();
        let (players, receivers) = new_test_jeopardy_player_map(10);

        // WHEN
        jeopardy.show_final_jeopardy_question(&players);

        // THEN
        let fin_jeopardy = jeopardy.config.final_jeopardy();
        let expected_title = fin_jeopardy.hint();
        let expected_content = fin_jeopardy.question().content();
        // ensure text card was created properly
        assert!(matches!(
            jeopardy.display,
            JeopardyDisplayEvent::TextCard { title, content }
                if title == expected_title && content == expected_content
        ));

        // ensure players receive the same event
        for mut rx in receivers {
            assert!(matches!(
                rx.recv().await.unwrap(),
                JeopardyPlayerEvent::Display(JeopardyDisplayEvent::TextCard { title, content })
                    if title == expected_title && content == expected_content
            ))
        }
    }

    #[tokio::test]
    async fn GIVEN_jeopardy_WHEN_show_final_jeopardy_answer_THEN_ok() {
        // infallible - so no negative test
        // GIVEN
        let mut jeopardy = Jeopardy::test_default();
        let (players, receivers) = new_test_jeopardy_player_map(10);

        // WHEN
        jeopardy.show_final_jeopardy_answer(&players);

        // THEN
        let fin_jeopardy = jeopardy.config.final_jeopardy().question();
        let expected_title = fin_jeopardy.content();
        let expected_answer = fin_jeopardy.answer();
        // ensure text card was created properly
        assert!(matches!(
            jeopardy.display,
            JeopardyDisplayEvent::TextCard { title, content }
                if title == expected_title && content == expected_answer
        ));

        // ensure players receive the same event
        for mut rx in receivers {
            assert!(matches!(
                rx.recv().await.unwrap(),
                JeopardyPlayerEvent::Display(JeopardyDisplayEvent::TextCard { title, content })
                    if title == expected_title && content == expected_answer
            ))
        }
    }

    // this test only ensures that RequestA -> ResponseA
    // the perspective here is ensuring that the caller receives a response we expect (as this is a near top level handler)
    // not validating internal state exhaustively (other unit tests do that)
    #[tokio::test] // needs tokio bc it calls `send_background()`
    async fn GIVEN_host_command_WHEN_handle_host_command_THEN_ok() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default();
        let buzzer_queue = (0..10).into_iter().map(|i| i.to_string());
        jeopardy.buzzer_queue.extend(buzzer_queue.clone());
        let expected_buzzer_queue = buzzer_queue.collect::<Vec<_>>();
        assert_eq!(false, jeopardy.buzzer_queue.is_empty()); // set the buzzer queue to dummy data

        let (mut players, _) = new_test_jeopardy_player_map(10);
        // programmatically get a random player to perform operations on
        let player_id = players.iter().map(|p| p.id().to_string()).next().unwrap();
        let player = players.get_mut(&player_id).unwrap();
        let init_points = 100;
        player.points = init_points;

        let host_commands = [
            // order matters here
            HostCommand::GetBuzzerQueue,
            HostCommand::ClearBuzzerQueue,
            HostCommand::UpdatePoints {
                player_id: player_id.clone(),
                delta: -1,
            },
            HostCommand::SetPoints {
                player_id: player_id.clone(),
                points: 0,
            },
            HostCommand::ShowBoard { board_index: 0 },
            HostCommand::ShowQuestion {
                board_index: 0,
                category_index: 0,
                question_index: 0,
            },
            HostCommand::GetAnswer {
                board_index: 0,
                category_index: 0,
                question_index: 0,
            },
            HostCommand::ShowCurrentAnswer,
            HostCommand::ShowFinalJeopardyAnswer,
            HostCommand::ShowFinalJeopardyQuestion,
        ];
        // WHEN
        for command in host_commands {
            let response = jeopardy
                .handle_host_command(
                    &mut players,
                    jeopardy.host_password.clone(),
                    command.clone(),
                )
                .unwrap();
            // THEN - ensure responses are what we expect
            let maps_correctly = match command {
                HostCommand::GetBuzzerQueue => {
                    matches!(
                        response,
                        HostCommandResponse::GetBuzzerQueue(queue)
                            if queue == expected_buzzer_queue
                    )
                }
                HostCommand::ClearBuzzerQueue => {
                    matches!(response, HostCommandResponse::Success)
                }
                HostCommand::UpdatePoints { delta, .. } => {
                    matches!(
                        response,
                        HostCommandResponse::UpdatePoints(points)
                            if points == init_points + delta
                    )
                }
                HostCommand::SetPoints { .. } => {
                    matches!(response, HostCommandResponse::Success)
                }
                HostCommand::ShowBoard { .. } => {
                    matches!(response, HostCommandResponse::Success)
                }
                HostCommand::ShowQuestion { .. } => {
                    matches!(response, HostCommandResponse::Success)
                }
                HostCommand::ShowCurrentAnswer => {
                    matches!(response, HostCommandResponse::Success)
                }
                HostCommand::GetAnswer {
                    board_index,
                    category_index,
                    question_index,
                } => {
                    let expected_answer = jeopardy.config.boards()[board_index].categories()
                        [category_index]
                        .questions()[question_index]
                        .underlying()
                        .answer();
                    matches!(
                        response,
                        HostCommandResponse::GetAnswer(answer)
                            if answer == expected_answer
                    )
                }
                HostCommand::ShowFinalJeopardyQuestion => {
                    matches!(response, HostCommandResponse::Success)
                }
                HostCommand::ShowFinalJeopardyAnswer => {
                    matches!(response, HostCommandResponse::Success)
                }
            };
            assert!(maps_correctly)
        }
    }

    // this test only ensures that RequestA -> ResponseA
    // the perspective here is ensuring that the caller receives a response we expect (as this is a near top level handler)
    // not validating internal state exhaustively (other unit tests do that)
    #[test]
    fn GIVEN_player_command_WHEN_handle_player_command_THEN_ok() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default();
        let (mut players, _) = new_test_jeopardy_player_map(10);
        struct PlayerTestConfig {
            points: i32,
            wager: i32,
            free_response: String,
        }
        // we need to do some setup so that scoreboard can actually sort by points
        let test_configs = players
            .iter()
            .enumerate()
            .map(|(index, p)| {
                let expected = index as i32 + 1;
                (
                    p.id().to_string(),
                    PlayerTestConfig {
                        points: expected,
                        wager: expected,
                        free_response: index.to_string(),
                    },
                )
            })
            .collect::<Vec<_>>();
        test_configs.iter().for_each(|(id, config)| {
            let p = players.get_mut(id).unwrap();

            p.points = config.points;
            p.free_response = config.free_response.clone();
            p.set_wager(config.wager).unwrap();

            assert_ne!("", p.free_response); // ensure not empty so we can empty it
            assert_ne!(0, p.wager()); // ensure non-zero so we can set to 0
        });

        let player_commands = vec![
            // order matters here
            PlayerCommand::Buzz,
            PlayerCommand::GetPoints,
            PlayerCommand::GetScoreboard,
            PlayerCommand::Refresh,
            PlayerCommand::GetFreeResponse,
            PlayerCommand::SetFreeResponse("".to_string()),
            PlayerCommand::GetWager,
            PlayerCommand::SetWager(0),
        ];
        // WHEN
        for (player_id, config) in test_configs {
            for command in player_commands.clone() {
                let response = jeopardy
                    .handle_player_command(&mut players, player_id.clone(), command.clone())
                    .unwrap();
                let player = players.get_mut(&player_id).unwrap();
                // THEN - ensure responses are what we expect
                let maps_correctly = match command {
                    PlayerCommand::Buzz => {
                        matches!(response, PlayerCommandResponse::Success)
                    }
                    PlayerCommand::GetPoints => {
                        matches!(response, PlayerCommandResponse::GetPoints(points) if points == player.points)
                    }
                    PlayerCommand::GetScoreboard => matches!(
                        response,
                        PlayerCommandResponse::GetScoreboard(scoreboard)
                            if scoreboard.is_sorted_by(|a, b| a >= b)
                    ),
                    PlayerCommand::Refresh => matches!(
                        response,
                        PlayerCommandResponse::Refresh(JeopardyDisplayEvent::Board(board))
                            if board.is_redacted_version(&jeopardy.config.boards()[0])
                    ),
                    PlayerCommand::GetFreeResponse => matches!(
                        response,
                        PlayerCommandResponse::GetFreeResponse(free_resp)
                            if free_resp == config.free_response
                    ),
                    PlayerCommand::SetFreeResponse(free_resp) => {
                        // we test this here bc `handle_player_command()` set manually
                        assert_eq!(free_resp, player.free_response);
                        matches!(response, PlayerCommandResponse::Success)
                    }
                    PlayerCommand::GetWager => {
                        matches!(response, PlayerCommandResponse::GetWager(wager) if wager == player.wager())
                    }
                    PlayerCommand::SetWager(wager) => {
                        assert_eq!(wager, player.wager());
                        matches!(response, PlayerCommandResponse::Success)
                    }
                };
                assert!(maps_correctly)
            }
        }
    }

    #[test]
    fn GIVEN_invalid_player_id_WHEN_handle_player_command_THEN_error() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default();
        let (mut players, _) = new_test_jeopardy_player_map(10);
        let invalid_player_id = "invalid".to_string(); // not in players map
        let command = PlayerCommand::GetPoints;

        // WHEN
        let handle_player_command_result = jeopardy.handle_player_command(
            &mut players,
            invalid_player_id.clone(),
            command.clone(),
        ); // ensure unified and specialized handler match
        let handle_event_result = jeopardy.handle_event(
            &mut players,
            JeopardyCommand::Player {
                player_id: invalid_player_id.clone(),
                command,
            },
        );

        // THEN
        assert!(matches!(
            handle_event_result,
            Err(JeopardyError::PlayerForGivenIDNotFound(id))
                if id == invalid_player_id
        ));
        assert!(matches!(
            handle_player_command_result,
            Err(JeopardyError::PlayerForGivenIDNotFound(id))
                if id == invalid_player_id
        ));
    }

    #[test]
    fn GIVEN_incorrect_host_password_WHEN_handle_event_THEN_error() {
        // GIVEN
        let host_password = "test";
        let mut jeopardy = Jeopardy::new(host_password, JeopardyConfig::test_default()).unwrap();
        let (mut players, _) = new_test_jeopardy_player_map(10);
        let command = HostCommand::GetBuzzerQueue;
        let invalid_host_password = "invalid".to_string();

        // WHEN
        let handle_host_command_result = jeopardy.handle_host_command(
            &mut players,
            invalid_host_password.clone(),
            command.clone(),
        ); // ensure unified and specialized handler match
        let handle_event_result = jeopardy.handle_event(
            &mut players,
            JeopardyCommand::Host {
                host_password: invalid_host_password,
                command,
            },
        );

        // THEN
        assert!(matches!(
            handle_host_command_result,
            Err(JeopardyError::IncorrectHostPassword)
        ));
        assert!(matches!(
            handle_event_result,
            Err(JeopardyError::IncorrectHostPassword)
        ));
    }

    #[test]
    fn GIVEN_player_error_WHEN_handle_event_THEN_error() {
        // GIVEN
        let mut jeopardy = Jeopardy::test_default();
        let (mut players, _) = new_test_jeopardy_player_map(10);
        let player_id = players.iter().next().map(|p| p.id().to_string()).unwrap();
        let invalid_wager = -1;
        let command = PlayerCommand::SetWager(invalid_wager);

        // WHEN
        let result =
            jeopardy.handle_event(&mut players, JeopardyCommand::Player { player_id, command });

        // THEN
        assert!(matches!(
            result, // ensure that player errors are propagated
            Err(JeopardyError::PlayerMisconfig(JeopardyPlayerError::InvalidWager { wager, .. }))
                if wager == invalid_wager
        ));
    }

    #[test]
    fn GIVEN_command_WHEN_handle_event_THEN_ok() {
        // GIVEN
        let host_password = "host_password";
        let mut jeopardy = Jeopardy::new(host_password, JeopardyConfig::test_default()).unwrap();
        let (mut players, _) = new_test_jeopardy_player_map(10);
        let player_id = players.iter().next().map(|p| p.id().to_string()).unwrap();

        // WHEN
        // ensure that player commands map to player responses
        // and host commands map to host responses at the top level
        let player_response = jeopardy
            .handle_event(
                &mut players,
                JeopardyCommand::Player {
                    player_id,
                    command: PlayerCommand::GetPoints,
                },
            )
            .unwrap();
        let host_response = jeopardy
            .handle_event(
                &mut players,
                JeopardyCommand::Host {
                    host_password: host_password.to_string(),
                    command: HostCommand::GetBuzzerQueue,
                },
            )
            .unwrap();

        // THEN
        assert!(matches!(
            player_response,
            JeopardyCommandResponse::Player(PlayerCommandResponse::GetPoints(..))
        ));
        assert!(matches!(
            host_response,
            JeopardyCommandResponse::Host(HostCommandResponse::GetBuzzerQueue(..))
        ));
    }
}
