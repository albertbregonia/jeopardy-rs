use crate::game::jeopardy::{JeopardyBoardError, non_empty_trimmed};
use serde::{Deserialize, Serialize};

/// Defines a simple `Question` as a pair of strings
/// one for the question content and the corresponding answer
/// This way, no Question ever goes without an answer
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Question {
    #[serde(deserialize_with = "non_empty_trimmed")]
    content: String,
    #[serde(deserialize_with = "non_empty_trimmed")]
    answer: String,
}

impl Question {
    pub fn new(content: String, answer: String) -> Result<Self, JeopardyBoardError> {
        let content = content.trim();
        if content.is_empty() {
            return Err(JeopardyBoardError::EmptyQuestion);
        }
        let answer = answer.trim();
        if answer.is_empty() {
            return Err(JeopardyBoardError::EmptyAnswer);
        }
        let question = Self {
            content: content.to_owned(),
            answer: answer.to_owned(),
        };
        Ok(question)
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn answer(&self) -> &str {
        &self.answer
    }

    pub fn redact(&mut self) {
        self.answer = String::new();
    }
}

#[cfg(test)]
use crate::server::TestDefault;

#[cfg(test)]
impl TestDefault for Question {
    fn test_default() -> Self {
        // GIVEN
        let test_content = "test_question".to_string();
        let test_answer = "test_answer".to_string();

        // WHEN
        let question = Self::new(test_content.clone(), test_answer.clone()).unwrap();

        // THEN
        assert_eq!(test_content, question.content());
        assert_eq!(test_answer, question.answer());

        question
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod question_tests {
    use crate::game::jeopardy::{JeopardyBoardError, question::Question};
    use crate::server::TestDefault;
    use serde_json::json;

    #[test]
    fn GIVEN_question_WHEN_new_THEN_ok() {
        Question::test_default();
    }

    #[test]
    fn GIVEN_question_WHEN_deserialized_THEN_ok() {
        // GIVEN
        let test_content = "test".to_string();
        let test_answer = "test".to_string();

        let json = json!({
            "content": test_content,
            "answer": test_answer,
        });

        // WHEN
        let question = serde_json::from_value::<Question>(json).unwrap();

        // THEN
        assert_eq!(test_content, question.content());
        assert_eq!(test_answer, question.answer());
    }

    #[test]
    fn GIVEN_empty_question_WHEN_new_THEN_error() {
        // GIVEN
        let content = String::new();

        // WHEN
        let result = Question::new(content, "test".to_string());

        // THEN
        assert!(matches!(result, Err(JeopardyBoardError::EmptyQuestion)));

        // GIVEN
        let content = "\t".to_string(); // contains only whitespace, will be trimmed

        // WHEN
        let result = Question::new(content, "test".to_string());

        // THEN
        assert!(matches!(result, Err(JeopardyBoardError::EmptyQuestion)));
    }

    #[test]
    fn GIVEN_empty_answer_WHEN_new_THEN_error() {
        // GIVEN
        let answer = String::new();

        // WHEN
        let result = Question::new("test".to_string(), answer);

        // THEN
        assert!(matches!(result, Err(JeopardyBoardError::EmptyAnswer)));

        // GIVEN
        let answer = "\t".to_string(); // contains only whitespace will be trimmed

        // WHEN
        let result = Question::new("test".to_string(), answer);

        // THEN
        assert!(matches!(result, Err(JeopardyBoardError::EmptyAnswer)));
    }

    #[test]
    fn GIVEN_empty_question_WHEN_deserialized_THEN_error() {
        // GIVEN
        let json = json!({
            "content": "",
            "answer": "test",
        });

        // WHEN
        let result = serde_json::from_value::<Question>(json);

        // THEN
        assert!(matches!(result, Err(serde_json::Error { .. })));
    }

    #[test]
    fn GIVEN_empty_answer_WHEN_deserialized_THEN_error() {
        // GIVEN
        let json = json!({
            "content": "test",
            "answer": "",
        });

        // WHEN
        let result = serde_json::from_value::<Question>(json);

        // THEN
        assert!(matches!(result, Err(serde_json::Error { .. })));
    }

    #[test]
    fn GIVEN_question_WHEN_redact_THEN() {
        // GIVEN
        let mut question = Question::test_default();

        // WHEN
        question.redact(); // deletes answer

        // THEN
        assert_eq!("", question.answer());
    }
}
