use serde::{Deserialize, Serialize};

use crate::game::jeopardy::question::Question;
use crate::game::jeopardy::{JeopardyBoardError, non_empty_trimmed};

// defines Final Jeopardy as a `Question` with a hint string

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FinalJeopardy {
    #[serde(deserialize_with = "non_empty_trimmed")]
    hint: String,
    question: Question,
}

impl FinalJeopardy {
    pub fn new(hint: String, question: Question) -> Result<Self, JeopardyBoardError> {
        let hint = hint.trim();
        if hint.is_empty() {
            return Err(JeopardyBoardError::EmptyHint);
        }
        Ok(Self {
            hint: hint.to_owned(),
            question,
        })
    }

    pub fn hint(&self) -> &str {
        &self.hint
    }

    pub fn question(&self) -> &Question {
        &self.question
    }
}

#[cfg(test)]
use crate::server::TestDefault;

#[cfg(test)]
impl TestDefault for FinalJeopardy {
    fn test_default() -> Self {
        let hint = "hint".to_string();
        let question = Question::test_default();

        // WHEN
        let final_jeopardy = Self::new(hint.clone(), question.clone()).unwrap();

        // THEN
        assert_eq!(hint, final_jeopardy.hint());
        assert_eq!(&question, final_jeopardy.question());

        final_jeopardy
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod final_jeopardy_tests {
    use serde_json::json;

    use crate::game::jeopardy::{
        JeopardyBoardError, final_jeopardy::FinalJeopardy, question::Question,
    };
    use crate::server::TestDefault;

    #[test]
    fn GIVEN_final_jeopardy_WHEN_new_THEN_ok() {
        FinalJeopardy::test_default();
    }

    #[test]
    fn GIVEN_empty_hint_WHEN_new_THEN_error() {
        // GIVEN
        let question = Question::test_default();
        let hint = String::new();

        // WHEN - truly empty
        let result = FinalJeopardy::new(hint, question.clone());

        // THEN
        assert!(matches!(result, Err(JeopardyBoardError::EmptyHint)));

        // GIVEN
        let hint = "\t".to_string();
        // WHEN - whitespace only
        let result = FinalJeopardy::new(hint, question.clone());

        // THEN
        assert!(matches!(result, Err(JeopardyBoardError::EmptyHint)));
    }

    #[test]
    fn GIVEN_empty_hint_WHEN_deserialize_THEN_error() {
        // GIVEN
        let question = Question::test_default();
        let json = json!({
            "hint": "", // empty hint
            "question": question,
        });

        // WHEN
        let result = serde_json::from_value::<FinalJeopardy>(json);

        // THEN
        assert!(matches!(result, Err(serde_json::Error { .. })));
    }
}
