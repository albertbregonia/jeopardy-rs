use std::collections::VecDeque;

use stagecrew::{
    lobby::Game,
    player::{Player, ReadPlayerCollection, player_map::PlayerMap},
};

use crate::game::{
    JeopardyCommand, JeopardyCommandResponse, JeopardyError,
    commands::player::JeopardyDisplayEvent,
    jeopardy::{
        board::Board, board_question::BoardQuestion, category::Category, config::JeopardyConfig,
    },
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
            Jeopardy, JeopardyError,
            commands::player::JeopardyDisplayEvent,
            jeopardy::{board::Board, config::JeopardyConfig, final_jeopardy::FinalJeopardy},
            player::JeopardyPlayer,
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
        jeopardy.display = JeopardyDisplayEvent::TextCard {
            // ensure buzzing doesn't no-op
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
}
