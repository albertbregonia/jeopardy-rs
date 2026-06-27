use crate::game::jeopardy::{
    JeopardyBoardError, board_question::BoardQuestion, non_empty_trimmed, non_empty_vec,
};
#[cfg(test)]
use crate::server::TestDefault;
use serde::{Deserialize, Serialize};

// defines a Jeopardy `Category` as a collection of `BoardQuestion`

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Category {
    #[serde(deserialize_with = "non_empty_trimmed")]
    name: String,
    #[serde(deserialize_with = "non_empty_vec")]
    questions: Vec<BoardQuestion>,
}

impl Category {
    pub fn new(name: String, questions: Vec<BoardQuestion>) -> Result<Self, JeopardyBoardError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(JeopardyBoardError::EmptyCategoryName);
        }
        if questions.is_empty() {
            return Err(JeopardyBoardError::EmptyQuestionList);
        }
        Ok(Self {
            name: name.to_owned(),
            questions,
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Helper function to auto assign points based on their index in the vec
    /// the formula is  `i+1 * 100`  (+1 for 1-indexing)
    pub fn assign_points(&mut self) {
        self.questions.iter_mut().enumerate().for_each(|(i, q)|
            // index 1, each question is 100 * index
            q.set_point_value((i as i32 + 1) * 100));
    }

    pub fn questions(&self) -> &[BoardQuestion] {
        &self.questions
    }

    pub fn questions_mut(&mut self) -> &mut [BoardQuestion] {
        &mut self.questions
    }
}

#[cfg(test)]
impl TestDefault for Category {
    fn test_default() -> Self {
        // GIVEN
        let category_name = "test".to_string();
        let test_question = BoardQuestion::test_default();
        let test_questions = vec![test_question];

        // WHEN
        let mut category = Self::new(category_name.clone(), test_questions.clone()).unwrap();

        // THEN
        assert_eq!(category_name, category.name());
        assert_eq!(test_questions, category.questions());
        assert_eq!(test_questions, category.questions_mut());
        category.assign_points(); // won't compile if not mut

        category
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod category_tests {
    use crate::game::jeopardy::{
        JeopardyBoardError, board_question::BoardQuestion, category::Category,
    };
    use crate::server::TestDefault;
    use serde_json::json;

    #[test]
    fn GIVEN_category_WHEN_new_THEN_ok() {
        Category::test_default();
    }

    #[test]
    fn GIVEN_category_WHEN_deserialize_THEN_ok() {
        // GIVEN
        let test_category_name = "test".to_string();
        let test_questions = vec![BoardQuestion::test_default()];
        let json = json!({
            "name": test_category_name,
            "questions": test_questions.clone()
        });

        // WHEN
        let mut category = serde_json::from_value::<Category>(json).unwrap();

        // THEN
        assert_eq!(test_category_name, category.name());
        assert_eq!(test_questions, category.questions());
        assert_eq!(test_questions, category.questions_mut());
        category.assign_points(); // won't compile if not mut
    }

    #[test]
    fn GIVEN_empty_category_name_WHEN_new_THEN_error() {
        // GIVEN
        let name = "".to_string();

        // WHEN
        let category = Category::new(name, vec![BoardQuestion::test_default()]);

        // THEN
        assert!(matches!(
            category,
            Err(JeopardyBoardError::EmptyCategoryName)
        ));
    }

    #[test]
    fn GIVEN_empty_category_name_WHEN_deserialize_THEN_error() {
        // GIVEN
        let json = json!({
            "name": "", // empty name
            "questions": [BoardQuestion::test_default()]
        });

        // WHEN
        let result = serde_json::from_value::<Category>(json);

        // THEN
        assert!(matches!(result, Err(serde_json::Error { .. })));

        // GIVEN
        let json = json!({
            "name": "\t", // whitespace only name, empty when trimmed
            "questions": [BoardQuestion::test_default()]
        });

        // WHEN
        let result = serde_json::from_value::<Category>(json);

        // THEN
        assert!(matches!(result, Err(serde_json::Error { .. })));
    }

    #[test]
    fn GIVEN_empty_questions_list_WHEN_new_THEN_error() {
        // GIVEN
        let questions = vec![];

        // WHEN
        let category = Category::new("test".to_string(), questions);

        // THEN
        assert!(matches!(
            category,
            Err(JeopardyBoardError::EmptyQuestionList)
        ));
    }

    #[test]
    fn GIVEN_empty_question_list_WHEN_deserialize_THEN_error() {
        // GIVEN
        let json = json!({
            "name": "test",
            "questions": []
        });

        // WHEN
        let result = serde_json::from_value::<Category>(json);

        // THEN
        assert!(matches!(result, Err(serde_json::Error { .. })));
    }

    #[test]
    fn GIVEN_category_WHEN_assign_points_THEN_ok() {
        // GIVEN
        let mut category = Category::test_default();

        // WHEN
        category.assign_points();

        // THEN
        let question = &category.questions()[0];
        assert_eq!(100, question.point_value());
    }
}
