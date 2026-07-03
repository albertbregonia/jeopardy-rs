mod create_lobby;
mod delete_lobby;
mod host_command;
mod join_lobby;

pub mod middleware;
mod player;
pub mod validators;

pub use create_lobby::create_lobby;
pub use delete_lobby::delete_lobby;
pub use host_command::handle_host_command;
pub use join_lobby::join_lobby;
use serde::Serialize;

const HOST_RESPONSE_RESULT_ERROR_KEY: &str = "error";
const HOST_RESPONSE_RESULT_VALUE_KEY: &str = "value";

pub(crate) fn serialize_result<T, S>(
    result: &Result<T, String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    T: Serialize,
    S: serde::Serializer,
{
    // we want to use result internally
    // but outwardly, we don't want to have callers have to handle
    // "Err" and "Ok" Rust formats bc that's too low level.
    // therefore, make it standard JSON and use Option<> so we get nulls
    serde_json::json!({
        HOST_RESPONSE_RESULT_VALUE_KEY: result.as_ref().ok(),
        HOST_RESPONSE_RESULT_ERROR_KEY: result.as_ref().err(),
    })
    .serialize(serializer)
}
