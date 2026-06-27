use serde::{Deserialize, Deserializer};
use thiserror::Error;

pub mod board;
pub mod board_question;
pub mod category;
pub mod config;
pub mod final_jeopardy;
pub mod question;

// constructs for the Jeopardy game such as Board, Questions, Categories, etc

const DESERIALIZE_EMPTY_TRIM_ERROR_MSG: &str = "Encountered empty string after trim in JSON";
const DESERIALIZE_EMPTY_VEC_ERROR_MSG: &str = "Encountered empty array in JSON";

#[derive(Debug, Error)]
pub enum JeopardyBoardError {
    #[error("Board list must never be empty")]
    EmptyBoardList,
    #[error("Category list for a board must never be empty")]
    EmptyCategoryList,
    #[error("Category name must never be empty")]
    EmptyCategoryName,
    #[error("Question list for a category must never be empty")]
    EmptyQuestionList,
    #[error("Missing value for hint")]
    EmptyHint,
    #[error("Missing value for answer")]
    EmptyAnswer,
    #[error("Missing value for question")]
    EmptyQuestion,
    #[error("Requested daily double count is larger than the question count")]
    InvalidDailyDoubleCount,
}

/// deserialize helper to ensure we never get an empty string
pub fn non_empty_trimmed<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(serde::de::Error::custom(DESERIALIZE_EMPTY_TRIM_ERROR_MSG));
    }
    Ok(trimmed.to_owned())
}

/// deserialize helper to ensure we never get an empty vec
pub fn non_empty_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let vec = Vec::<T>::deserialize(deserializer)?;
    if vec.is_empty() {
        return Err(serde::de::Error::custom(DESERIALIZE_EMPTY_VEC_ERROR_MSG));
    }
    Ok(vec)
}
