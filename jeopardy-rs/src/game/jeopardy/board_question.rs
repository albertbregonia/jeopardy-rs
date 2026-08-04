use crate::game::jeopardy::question::Question;
#[cfg(test)]
use crate::server::TestDefault;
use serde::{Deserialize, Serialize};

// TODO:
/// Defines a standard Jeopardy board question as a wrapper over a `Question`
/// but includes question point value, daily double, etc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BoardQuestion {
    // i think it's funny as hell to make this an i32
    // meaning you could get an answer wrong to subtract negative points (gain points) ;)
    point_value: i32,
    daily_double: bool,
    answered: bool,

    // we can derive Serialize, Deserialize and it will still
    // use our custom validation for `Question`
    question: Question,
}

impl BoardQuestion {
    pub fn new(point_value: i32, daily_double: bool, question: Question) -> Self {
        Self {
            point_value,
            daily_double,
            answered: false,
            question,
        }
    }

    pub fn underlying(&self) -> &Question {
        &self.question
    }

    pub fn point_value(&self) -> i32 {
        self.point_value
    }

    pub fn set_point_value(&mut self, value: i32) {
        self.point_value = value;
    }

    pub fn is_daily_double(&self) -> bool {
        self.daily_double
    }

    pub fn set_daily_double(&mut self, daily_double: bool) {
        self.daily_double = daily_double;
    }

    pub fn is_answered(&self) -> bool {
        self.answered
    }

    pub fn set_answered(&mut self, answered: bool) {
        self.answered = answered;
    }

    /// Erases the underlying answer string using `Question::redact()`
    /// it also sets the `daily_double` property to `false`
    pub fn redact(&mut self) {
        self.question.redact(); // redact underlying
        self.set_daily_double(false); // clear daily double
    }
}

#[cfg(test)]
impl TestDefault for BoardQuestion {
    fn test_default() -> Self {
        // GIVEN
        let test_question = Question::test_default();
        let test_points = 100;
        let test_daily_double = true;

        // WHEN
        let mut board_q = Self::new(test_points, test_daily_double, test_question.clone());

        // THEN
        assert_eq!(test_points, board_q.point_value());
        assert_eq!(test_daily_double, board_q.is_daily_double());
        assert_eq!(false, board_q.is_answered()); // default value
        assert_eq!(&test_question, board_q.underlying());

        board_q.set_daily_double(false); // reset to default

        board_q
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod board_question_tests {
    use crate::{game::jeopardy::board_question::BoardQuestion, server::TestDefault};

    #[test]
    fn GIVEN_question_wrapper_WHEN_new_THEN_ok() {
        BoardQuestion::test_default();
    }

    #[test]
    fn GIVEN_point_value_WHEN_set_point_value_THEN_ok() {
        // GIVEN
        let mut question = BoardQuestion::test_default();
        let point_value = 100;

        // WHEN
        question.set_point_value(point_value);

        // THEN
        assert_eq!(point_value, question.point_value());
    }

    #[test]
    fn GIVEN_daily_double_bool_WHEN_set_daily_double_THEN_ok() {
        // GIVEN
        let mut question = BoardQuestion::test_default();
        assert_eq!(question.is_daily_double(), false); // ensure default false

        // WHEN
        question.set_daily_double(true);

        // THEN
        assert!(question.is_daily_double());
    }

    #[test]
    fn GIVEN_answered_bool_WHEN_set_answered_THEN_ok() {
        // GIVEN
        let mut question = BoardQuestion::test_default();
        assert_eq!(question.is_answered(), false); // ensure default false

        // WHEN
        question.set_answered(true);

        // THEN
        assert!(question.is_answered());
    }

    #[test]
    fn GIVEN_question_WHEN_redact_THEN_ok() {
        // GIVEN
        let mut board_q = BoardQuestion::test_default();

        board_q.set_daily_double(true); // ensure daily double is true
        assert!(board_q.is_daily_double());

        let point_value = board_q.point_value(); // save original values
        let answered = board_q.is_answered();
        assert_ne!(board_q.underlying().answer(), ""); // ensure not redacted

        // WHEN
        board_q.redact();

        // THEN
        assert_eq!(false, board_q.is_daily_double()); // daily double is cleared
        assert_eq!("", board_q.underlying().answer()); // answer is cleared
        assert_eq!(point_value, board_q.point_value()); // point value untouched
        assert_eq!(answered, board_q.is_answered()); // answered value is untouched
    }
}
