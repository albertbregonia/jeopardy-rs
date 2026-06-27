use serde::{Deserialize, Serialize};

use crate::game::jeopardy::{
    JeopardyBoardError, board::Board, final_jeopardy::FinalJeopardy, non_empty_vec,
};

/// `JeopardyConfig` is a high level struct encapsulating
/// the entirety of the Jeopardy game configuration (board, final jeopardy, etc)
/// not including state (points, daily doubles, etc.)
/// The main purpose is to make this reusable across game lobby instances
/// and allow for user-defined custom boards with custom questions, etc.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct JeopardyConfig {
    #[serde(deserialize_with = "non_empty_vec")]
    boards: Vec<Board>,
    final_jeopardy: FinalJeopardy,
}

impl JeopardyConfig {
    pub fn new(
        boards: Vec<Board>,
        final_jeopardy: FinalJeopardy,
    ) -> Result<Self, JeopardyBoardError> {
        if boards.is_empty() {
            return Err(JeopardyBoardError::EmptyBoardList);
        }
        Ok(Self {
            boards,
            final_jeopardy,
        })
    }

    pub fn boards(&self) -> &[Board] {
        &self.boards
    }

    pub fn boards_mut(&mut self) -> &mut [Board] {
        &mut self.boards
    }

    pub fn final_jeopardy(&self) -> &FinalJeopardy {
        &self.final_jeopardy
    }
}

#[cfg(test)]
use crate::server::TestDefault;

#[cfg(test)]
impl TestDefault for JeopardyConfig {
    fn test_default() -> Self {
        // GIVEN
        let boards = vec![Board::test_default()];
        let final_jeopardy = FinalJeopardy::test_default();

        // WHEN
        let mut config = Self::new(boards.clone(), final_jeopardy.clone()).unwrap();

        // THEN
        assert_eq!(&boards, config.boards());
        assert_eq!(&boards, config.boards_mut());
        assert_eq!(&final_jeopardy, config.final_jeopardy());
        config.boards_mut()[0].assign_daily_double(1).unwrap(); // won't compile if not mut

        config
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod jeopardy_config_tests {
    use serde_json::json;

    use crate::game::jeopardy::board::Board;
    use crate::game::jeopardy::{
        JeopardyBoardError, config::JeopardyConfig, final_jeopardy::FinalJeopardy,
    };
    use crate::server::TestDefault;

    #[test]
    fn GIVEN_jeopardy_config_WHEN_new_THEN_ok() {
        JeopardyConfig::test_default();
    }

    #[test]
    fn GIVEN_jeopardy_config_WHEN_deserialize_THEN_ok() {
        // GIVEN
        let boards = vec![Board::test_default()];
        let final_jeopardy = FinalJeopardy::test_default();
        let json = json!({
            "boards": boards.clone(),
            "final_jeopardy": final_jeopardy.clone(),
        });

        // WHEN
        let mut config = serde_json::from_value::<JeopardyConfig>(json).unwrap();

        // THEN
        assert_eq!(&boards, config.boards());
        assert_eq!(&boards, config.boards_mut());
        assert_eq!(&final_jeopardy, config.final_jeopardy());
        config.boards_mut()[0].assign_daily_double(1).unwrap(); // won't compile if not mut
    }

    #[test]
    fn GIVEN_empty_board_list_WHEN_new_THEN_error() {
        // GIVEN
        let boards = vec![];

        // WHEN
        let result = JeopardyConfig::new(boards, FinalJeopardy::test_default());

        // THEN
        assert!(matches!(result, Err(JeopardyBoardError::EmptyBoardList)))
    }

    #[test]
    fn GIVEN_empty_board_list_WHEN_deserialize_THEN_error() {
        // GIVEN
        let json = json!({
            "boards": [],
            "final_jeopardy": FinalJeopardy::test_default(),
        });

        // WHEN
        let result = serde_json::from_value::<JeopardyConfig>(json);

        // THEN
        assert!(matches!(result, Err(serde_json::Error { .. })))
    }
}
