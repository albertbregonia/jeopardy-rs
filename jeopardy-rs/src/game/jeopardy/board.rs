use std::collections::HashSet;

use crate::game::jeopardy::{JeopardyBoardError, category::Category, non_empty_vec};
use serde::{Deserialize, Serialize};

#[cfg(test)]
use crate::server::TestDefault;

// defines a Jeopardy game board as a collection of `Category`

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Board {
    #[serde(deserialize_with = "non_empty_vec")]
    categories: Vec<Category>,
}

impl Board {
    pub fn new(categories: Vec<Category>) -> Result<Self, JeopardyBoardError> {
        if categories.is_empty() {
            return Err(JeopardyBoardError::EmptyCategoryList);
        }
        Ok(Self { categories })
    }

    pub fn categories(&self) -> &[Category] {
        &self.categories
    }

    pub fn categories_mut(&mut self) -> &mut [Category] {
        &mut self.categories
    }

    /// Attempts to set `count` questions of the board as a daily double
    /// ignoring manually set daily doubles.
    /// If the number of non-daily double questions are < `count`, this errors
    pub fn assign_daily_double(&mut self, count: usize) -> Result<(), JeopardyBoardError> {
        let question_count = self
            .categories
            .iter()
            .map(|c| {
                c.questions()
                    .iter()
                    .filter(|q| !q.is_daily_double())
                    .count()
            })
            .sum();
        // if the count is higher than the amount of non-daily double questions
        // aka - do we have enough non-daily double questions to set
        if count > question_count {
            return Err(JeopardyBoardError::InvalidDailyDoubleCount);
        }
        // we cannot rely on the board as our only memory bc of manually set daily doubles
        let mut set_coordinates = HashSet::new();

        while set_coordinates.len() != count {
            let category_index = rand::random_range(0..self.categories.len());
            let question_index =
                rand::random_range(0..self.categories()[category_index].questions().len());
            let coordinates = (category_index, question_index);
            let lookup = &mut self.categories_mut()[category_index].questions_mut()[question_index];
            // if the position is not a manually set daily double and it wasn't set by the algo
            if !lookup.is_daily_double() && !set_coordinates.contains(&coordinates) {
                lookup.set_daily_double(true);
                set_coordinates.insert(coordinates);
            }
        }
        Ok(())
    }

    pub fn redacted(&self) -> Self {
        let mut redacted = self.clone();
        redacted
            .categories
            .iter_mut()
            .for_each(|c| c.questions_mut().iter_mut().map(|q| q.redact()).collect());
        redacted
    }

    pub fn is_redacted(&self) -> bool {
        let has_non_redacted = self.categories.iter().any(|c| {
            c.questions()
                .iter()
                .any(|q| q.underlying().answer() != "" || q.is_daily_double())
        });
        !has_non_redacted
    }

    /// helper function to create a n x m jeopardy board
    /// with dummy test values using the TestDefault::test_default() trait
    #[cfg(test)]
    pub fn test_default_from_counts(
        category_count: usize,
        question_count_per_category: usize,
    ) -> Self {
        use crate::game::jeopardy::{board_question::BoardQuestion, question::Question};

        let test_question =
            Question::new("test_question".to_string(), "test_answer".to_string()).unwrap();
        let test_question = BoardQuestion::new(0, false, test_question);
        let test_category = Category::new(
            "test_category".to_string(),
            (0..question_count_per_category)
                .into_iter()
                .map(|_| test_question.clone())
                .collect(),
        )
        .unwrap();
        Self::new(
            (0..category_count)
                .into_iter()
                .map(|_| test_category.clone())
                .collect(),
        )
        .unwrap()
    }
}

#[cfg(test)]
impl TestDefault for Board {
    fn test_default() -> Self {
        // GIVEN
        let test_category = Category::test_default();
        let test_categories = vec![test_category.clone()];

        // WHEN
        let mut board = Self::new(test_categories.clone()).unwrap();

        // THEN
        assert_eq!(test_categories, board.categories());
        assert_eq!(test_categories, board.categories_mut());
        board.categories_mut()[0].assign_points(); // won't compile if not mut

        board
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod board_tests {
    use crate::{
        game::jeopardy::{JeopardyBoardError, board::Board, category::Category},
        server::TestDefault,
    };
    use serde_json::json;

    #[test]
    fn GIVEN_board_WHEN_new_THEN_ok() {
        Board::test_default();
    }

    #[test]
    fn GIVEN_board_WHEN_deserialize_THEN_ok() {
        let test_categories = vec![Category::test_default()];
        // GIVEN
        let json = json!({
            "categories": test_categories
        });

        // WHEN
        let mut board = serde_json::from_value::<Board>(json).unwrap();

        // THEN
        assert_eq!(test_categories, board.categories());
        assert_eq!(test_categories, board.categories_mut());
        board.categories_mut()[0].assign_points(); // won't compile if not mut
    }

    #[test]
    fn GIVEN_empty_category_list_WHEN_new_THEN_error() {
        // GIVEN
        let categories = vec![];

        // WHEN
        let result = Board::new(categories);

        // THEN
        assert!(matches!(result, Err(JeopardyBoardError::EmptyCategoryList)));
    }

    #[test]
    fn GIVEN_empty_category_list_WHEN_deserialize_THEN_error() {
        // GIVEN
        let json = json!({
            "categories": []
        });

        // WHEN
        let result = serde_json::from_value::<Board>(json);

        // THEN
        assert!(matches!(result, Err(serde_json::Error { .. })));
    }

    #[test]
    fn GIVEN_board_WHEN_assign_daily_double_THEN_ok() {
        // GIVEN
        let category_count = 5;
        let question_count_per_category = 5;
        let mut board =
            Board::test_default_from_counts(category_count, question_count_per_category);
        let daily_double_count = category_count * question_count_per_category;

        // WHEN
        board.assign_daily_double(daily_double_count).unwrap();

        // THEN
        assert!(
            board // all should be daily double
                .categories
                .iter()
                .all(|c| c.questions().iter().all(|q| q.is_daily_double()))
        );
        assert_eq!(
            board // daily double count should be 25
                .categories
                .iter()
                .map(|c| c.questions().iter().filter(|q| q.is_daily_double()).count())
                .sum::<usize>(),
            daily_double_count
        );
    }

    #[test]
    fn GIVEN_invalid_daily_double_count_WHEN_assign_daily_double_THEN_error() {
        // GIVEN
        let mut board = Board::test_default_from_counts(1, 1);

        // WHEN
        let result = board.assign_daily_double(10); // 10 is too large for a single question board

        // THEN
        let has_daily_double = board // ensure no daily doubles were assigned
            .categories
            .iter()
            .any(|c| c.questions().iter().any(|q| q.is_daily_double()));
        assert_eq!(has_daily_double, false);
        assert!(matches!(
            result,
            Err(JeopardyBoardError::InvalidDailyDoubleCount)
        ));
    }

    #[test]
    fn GIVEN_board_WHEN_redacted_THEN_ok() {
        // GIVEN
        let category_count = 5;
        let question_count_per_category = 5;
        let board = Board::test_default_from_counts(category_count, question_count_per_category);

        // WHEN
        let redacted = board.redacted(); // returns a new copy

        // THEN
        // all answers should be "" and all daily doubles should be set to false
        let all_answers_redacted = redacted.categories.iter().all(|c| {
            c.questions()
                .iter()
                .all(|q| q.underlying().answer() == "" && !q.is_daily_double())
        });
        assert!(all_answers_redacted);
    }

    #[test]
    fn GIVEN_redacted_board_WHEN_is_redacted_THEN_ok() {
        // GIVEN
        let category_count = 5;
        let question_count_per_category = 5;
        let board = Board::test_default_from_counts(category_count, question_count_per_category);

        // WHEN
        let redacted = board.redacted().is_redacted();

        // THEN
        assert!(redacted);
    }

    #[test]
    fn GIVEN_nonredacted_board_WHEN_is_redacted_THEN_ok() {
        // GIVEN
        let category_count = 5;
        let question_count_per_category = 5;
        let board = Board::test_default_from_counts(category_count, question_count_per_category);

        // WHEN
        let redacted = board.is_redacted();

        // THEN
        assert_eq!(false, redacted);
    }

    #[test]
    fn GIVEN_manual_daily_double_WHEN_assign_daily_double_THEN_error() {
        // GIVEN
        let mut board = Board::test_default_from_counts(1, 1);
        board.categories_mut()[0].questions_mut()[0].set_daily_double(true); // manually set daily double

        // WHEN
        // invalid count bc there are not enough non-daily double questions to set
        let result = board.assign_daily_double(1);

        // THEN
        assert!(matches!(
            result,
            Err(JeopardyBoardError::InvalidDailyDoubleCount)
        ));
    }
}
