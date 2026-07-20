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

#[cfg(test)]
pub mod test_util {
    use crate::server::{CredsValidatorGeneric, ManagerGeneric};
    use crate::web::handlers::create_lobby::{CreateLobbyRequest, CreateLobbyResponse};
    use crate::{
        game::{
            Jeopardy,
            player::{JeopardyPlayer, JeopardyPlayerEvent},
        },
        server::{
            JeopardyServer, JeopardyServerState, JeopardyServerStateGeneric, ServerConfig,
            TestDefault,
        },
        web::handlers::validators::nonzero_ascii::NonZeroAsciiValidator,
    };
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::{Extension, Json};
    use stagecrew::manager::PasswordProtectedLobby;
    use stagecrew::manager::{Manager, ManagerEntry, test_manager_constructs::TestManager};
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use uuid::Uuid;

    pub async fn add_lobby_to_server_state<M: ManagerGeneric, C: CredsValidatorGeneric>(
        state: &JeopardyServerStateGeneric<M, C>,
        request: CreateLobbyRequest,
    ) {
        let lobby_name = request.lobby_name.clone();
        // WHEN
        let (status_code, result) = super::create_lobby(
            State(state.clone()),
            Extension(Uuid::new_v4()),
            Json(request),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::CREATED);
        assert!(matches!(
            result,
            Json(CreateLobbyResponse { error: None, .. })
        ));
        let lobby_exists = state.manager().read().await.has(&lobby_name).unwrap();
        assert!(lobby_exists); // ensure exists
    }

    // helper function for the positive test case
    // primarily used for delete_lobby tests as they need a lobby to delete one
    pub async fn new_test_server_state(
        create_lobby: Option<CreateLobbyRequest>,
    ) -> JeopardyServerState {
        // GIVEN
        let state = Arc::new(JeopardyServer::from_config(ServerConfig::test_default()));

        if let Some(request) = create_lobby {
            add_lobby_to_server_state(&state, request).await
        }
        state
    }

    // create a jeopardy server with a test player given their ID
    // returns the server state and mpsc::Receiver<_> to listen for broadcasts
    pub async fn new_test_server_state_with_player(
        create_lobby: CreateLobbyRequest,
        player_id: &str,
    ) -> (JeopardyServerState, mpsc::Receiver<JeopardyPlayerEvent>) {
        // GIVEN
        let state = new_test_server_state(Some(create_lobby.clone())).await;
        let (tx, rx) = mpsc::channel(1);
        let player = JeopardyPlayer::new(player_id.to_string(), 0, tx);
        state
            .manager()
            .write()
            .await
            .get(&create_lobby.lobby_name)
            .unwrap()
            .lobby() // add test player
            .add_player(player_id, player)
            .await
            .unwrap();
        (state, rx)
    }

    pub type TestManagerServerState =
        Arc<JeopardyServer<TestManager<PasswordProtectedLobby<Jeopardy>>, NonZeroAsciiValidator>>;

    // create a jeopardy server with a test manager so we can induce failures
    pub async fn new_test_manager_server_state(
        create_lobby: Option<CreateLobbyRequest>,
    ) -> TestManagerServerState {
        // GIVEN
        let state = Arc::new(JeopardyServer::new(
            TestManager::default(),
            NonZeroAsciiValidator::new(32),
            ServerConfig::test_default(),
        ));
        if let Some(request) = create_lobby {
            state.manager().write().await.set_never_fail();
            add_lobby_to_server_state(&state, request).await;
            state.manager().write().await.reset(); // don't count the create lobby operations towards permits
        }
        state
    }

    pub async fn shutdown_lobby<M: ManagerGeneric, C: CredsValidatorGeneric>(
        state: &JeopardyServerStateGeneric<M, C>,
        lobby_name: &str,
    ) {
        let manager = state.manager().read().await;
        let lobby = manager.get(lobby_name).unwrap().lobby();
        lobby.shutdown().await.unwrap(); // shutdown
        assert!(lobby.is_shutdown());
    }

    // helper function to check if a lobby has a player given the respective IDs
    // (mostly made bc writing out this call everywhere is long)
    pub async fn lobby_has_player<M: ManagerGeneric, C: CredsValidatorGeneric>(
        state: &JeopardyServerStateGeneric<M, C>,
        lobby_id: &str,
        player_id: &str,
    ) -> bool {
        state
            .manager()
            .read()
            .await
            .get(lobby_id)
            .unwrap()
            .lobby()
            .has_player(player_id)
            .await
            .unwrap()
    }

    pub async fn lobby_exists<M: ManagerGeneric, C: CredsValidatorGeneric>(
        state: &JeopardyServerStateGeneric<M, C>,
        lobby_id: &str,
    ) -> bool {
        state.manager().read().await.has(lobby_id).unwrap()
    }
}
