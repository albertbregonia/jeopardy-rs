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

const RESULT_ERR_JSON_KEY: &str = "error";
const RESULT_OK_JSON_KEY: &str = "value";

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
    let result = result.as_ref();
    serde_json::json!({
        RESULT_OK_JSON_KEY: result.ok(),
        RESULT_ERR_JSON_KEY: result.err(),
    })
    .serialize(serializer)
}

#[cfg(test)]
#[allow(non_snake_case)]
mod serialize_result_tests {
    use crate::web::handlers::{RESULT_ERR_JSON_KEY, RESULT_OK_JSON_KEY, serialize_result};
    use serde::Serialize;

    #[derive(Serialize)]
    struct ResultWrapper {
        #[serde(serialize_with = "serialize_result")]
        result: Result<usize, String>,
    }

    #[test]
    fn GIVEN_ok_result_WHEN_serialize_result_THEN_ok() {
        // GIVEN
        let result = ResultWrapper { result: Ok(3) };

        // WHEN
        let serialized = serde_json::to_value(&result).unwrap();
        let serialized = serialized.as_object().unwrap().get("result").unwrap();
        let serialized_value = serialized.get(RESULT_OK_JSON_KEY).unwrap();
        let serialized_error = serialized.get(RESULT_ERR_JSON_KEY).unwrap();

        // THEN
        assert_eq!(false, serialized_value.is_null()); // simply ensure not null, more generic check for all types
        assert!(serialized_error.is_null());
    }

    #[test]
    fn GIVEN_err_result_WHEN_serialize_result_THEN_ok() {
        // GIVEN
        let error_msg = "error".to_string();
        let result = ResultWrapper {
            result: Err(error_msg.clone()),
        };

        // WHEN
        let serialized = serde_json::to_value(&result).unwrap();
        let serialized = serialized.as_object().unwrap().get("result").unwrap();
        let serialized_value = serialized.get(RESULT_OK_JSON_KEY).unwrap();
        let serialized_error = serialized.get(RESULT_ERR_JSON_KEY).unwrap();

        // THEN
        assert_eq!(error_msg, serialized_error.as_str().unwrap());
        assert!(serialized_value.is_null());
    }
}
