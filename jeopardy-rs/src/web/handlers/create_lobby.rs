use axum::Extension;
use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use stagecrew::lobby::Lobby;
use stagecrew::manager::PasswordProtectedLobby;
use stagecrew::player::player_map::PlayerMap;
use uuid::Uuid;

use crate::game::Jeopardy;
use crate::game::jeopardy::config::JeopardyConfig;
use crate::server::{CredsValidatorGeneric, GenericJeopardyServerState, ManagerGeneric};
use crate::web::handlers::validators::CredsValidator;

#[derive(Debug, Deserialize, Clone)]
pub struct CreateLobbyRequest {
    lobby_name: String,
    lobby_password: String,
    host_password: String,
    config: JeopardyConfig,
}

#[derive(Debug, Serialize)]
pub struct CreateLobbyResponse {
    request_id: String,
    pub error: Option<String>,
}

fn is_valid_create_lobby_request(
    validator: &impl CredsValidator,
    request: &CreateLobbyRequest,
) -> bool {
    let lobby_id_ok = validator.is_valid_lobby_id(&request.lobby_name);
    let lobby_pw_ok = validator.is_valid_lobby_password(&request.lobby_password);
    let host_pw_ok = validator.is_valid_host_password(&request.host_password);
    tracing::info!(
        "Valid lobby ID / lobby password / host password? {lobby_id_ok} {lobby_pw_ok} {host_pw_ok}"
    );
    lobby_id_ok && lobby_pw_ok && host_pw_ok
}

// TODO: we need to be careful here of soft memory leaks
// - players can create lobbies but we need rules on how long they live
// - there should be a grace period on join/deletion if the lobby is empty
pub async fn create_lobby<M: ManagerGeneric, C: CredsValidatorGeneric>(
    State(state): State<GenericJeopardyServerState<M, C>>,
    Extension(request_id): Extension<Uuid>,
    Json(request): Json<CreateLobbyRequest>,
) -> (StatusCode, Json<CreateLobbyResponse>) {
    // although lobbies are ephemeral i don't like logging passwords
    let create_lobby_span = tracing::info_span!("create_lobby", lobby_id = request.lobby_name);
    let _enter = create_lobby_span.enter();

    let request_id = request_id.to_string();
    if !is_valid_create_lobby_request(state.validator(), &request) {
        let error_msg = "Invalid format given for one or more parameters of create lobby request";
        tracing::warn!("{error_msg}");
        return (
            StatusCode::BAD_REQUEST,
            Json(CreateLobbyResponse {
                request_id,
                error: Some(error_msg.to_string()),
            }),
        );
    }
    let CreateLobbyRequest {
        lobby_name,
        lobby_password,
        host_password,
        config,
    } = request;
    match state.manager().read().await.has(&lobby_name) {
        Ok(lobby_exists) => {
            if lobby_exists {
                tracing::warn!("Lobby already exists");
                return (
                    StatusCode::CONFLICT,
                    Json(CreateLobbyResponse {
                        request_id,
                        error: Some(
                            "Failed to create lobby. A lobby with the given name already exists."
                                .to_string(),
                        ),
                    }),
                );
            }
            // otherwise continue
        }
        Err(e) => {
            tracing::error!("Failed to check if lobby exists: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(CreateLobbyResponse {
                    request_id,
                    error: Some("Internal Server Error".to_string()),
                }),
            );
        }
    }

    let new_game = match Jeopardy::new(&host_password, config) {
        Ok(game) => game,
        Err(e) => {
            tracing::warn!("Failed to create new Jeopardy game: {e}");
            return (
                StatusCode::BAD_REQUEST,
                Json(CreateLobbyResponse {
                    request_id,
                    error: Some(e.to_string()),
                }),
            );
        }
    };
    let new_lobby = Lobby::new(
        new_game,
        PlayerMap::new(),
        state.config().player_channel_buffer_size,
    );
    let lobby_entry = PasswordProtectedLobby::new(lobby_name.clone(), lobby_password, new_lobby);

    let create_lobby_result = state.manager().write().await.add(&lobby_name, lobby_entry);
    match create_lobby_result {
        Ok(_) => {
            tracing::info!("Lobby successfully created");
            // in case they create a lobby but log off
            // TODO: spawn auto-delete if empty lobby after grace period
            (
                StatusCode::CREATED,
                Json(CreateLobbyResponse {
                    request_id,
                    error: None,
                }),
            )
        }
        Err(e) => {
            tracing::error!("Failed to create lobby: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(CreateLobbyResponse {
                    request_id,
                    error: Some("Internal Server Error".to_string()),
                }),
            )
        }
    }
}

#[cfg(test)]
pub mod create_lobby_test_util {
    use std::sync::Arc;

    use super::*;
    use crate::{
        game::Jeopardy,
        server::{JeopardyServer, JeopardyServerState, ServerConfig, TestDefault},
        web::handlers::validators::nonzero_ascii::NonZeroAsciiValidator,
    };
    use stagecrew::manager::test_manager_constructs::TestManager;

    // helper function for the positive test case
    // primarily used for delete_lobby tests as they need a lobby to delete one
    pub async fn new_test_server(create_lobby: Option<CreateLobbyRequest>) -> JeopardyServerState {
        // GIVEN
        let state = Arc::new(JeopardyServer::from_config(ServerConfig::test_default()));

        if let Some(request) = create_lobby {
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
        }
        state
    }

    // create a jeopardy server with a test manager so we can induce failures
    pub async fn new_test_server_with_test_manager(
        create_lobby: Option<CreateLobbyRequest>,
    ) -> Arc<JeopardyServer<TestManager<PasswordProtectedLobby<Jeopardy>>, NonZeroAsciiValidator>>
    {
        // GIVEN
        let state = Arc::new(JeopardyServer::new(
            TestManager::default(),
            NonZeroAsciiValidator::new(32),
            ServerConfig::test_default(),
        ));
        if let Some(request) = create_lobby {
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
            state.manager().write().await.reset(); // don't count the create lobby operations towards permits
        }
        state
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod create_lobby_tests {
    use super::*;
    use crate::{
        server::TestDefault,
        web::handlers::create_lobby::create_lobby_test_util::{
            new_test_server, new_test_server_with_test_manager,
        },
    };

    #[tokio::test]
    async fn GIVEN_valid_create_lobby_request_WHEN_create_lobby_THEN_success() {
        new_test_server(Some(CreateLobbyRequest {
            lobby_name: "test".to_string(),
            lobby_password: "test".to_string(),
            host_password: "test".to_string(),
            config: JeopardyConfig::test_default(),
        }))
        .await;
    }

    #[tokio::test]
    async fn GIVEN_invalid_format_create_lobby_request_WHEN_create_lobby_THEN_error() {
        // GIVEN
        let state = new_test_server(None).await;

        // WHEN - invalid lobby name test
        let (status_code, result) = super::create_lobby(
            State(state.clone()),
            Extension(Uuid::new_v4()),
            Json(CreateLobbyRequest {
                lobby_name: "".to_string(), // cant be empty
                lobby_password: "test".to_string(),
                host_password: "test".to_string(),
                config: JeopardyConfig::test_default(),
            }),
        )
        .await;
        // THEN
        assert_eq!(status_code, StatusCode::BAD_REQUEST);
        assert!(matches!(
            result,
            Json(CreateLobbyResponse {
                error: Some(..),
                ..
            })
        ));

        // WHEN - invalid lobby password test
        let (status_code, result) = super::create_lobby(
            State(state.clone()),
            Extension(Uuid::new_v4()),
            Json(CreateLobbyRequest {
                lobby_name: "test".to_string(),
                lobby_password: "".to_string(), // can't be empty
                host_password: "test".to_string(),
                config: JeopardyConfig::test_default(),
            }),
        )
        .await;
        // THEN
        assert_eq!(status_code, StatusCode::BAD_REQUEST);
        assert!(matches!(
            result,
            Json(CreateLobbyResponse {
                error: Some(..),
                ..
            })
        ));

        // WHEN - invalid host password test
        let (status_code, result) = super::create_lobby(
            State(state.clone()),
            Extension(Uuid::new_v4()),
            Json(CreateLobbyRequest {
                lobby_name: "test".to_string(),
                lobby_password: "test".to_string(),
                host_password: "".to_string(), // cant be empty
                config: JeopardyConfig::test_default(),
            }),
        )
        .await;
        // THEN
        assert_eq!(status_code, StatusCode::BAD_REQUEST);
        assert!(matches!(
            result,
            Json(CreateLobbyResponse {
                error: Some(..),
                ..
            })
        ));
    }

    #[tokio::test]
    async fn GIVEN_invalid_jeopardy_config_WHEN_create_lobby_THEN_error() {
        // GIVEN
        let state = new_test_server(None).await;

        // WHEN - invalid JeopardyConfig test
        let (status_code, result) = super::create_lobby(
            State(state),
            Extension(Uuid::new_v4()),
            Json(CreateLobbyRequest {
                lobby_name: "test".to_string(),
                lobby_password: "test".to_string(),
                host_password: "test".to_string(),
                config: JeopardyConfig::invalid_default(), // test function (not in actual API)
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::BAD_REQUEST);
        assert!(matches!(
            result,
            Json(CreateLobbyResponse {
                error: Some(..),
                ..
            })
        ));
    }

    #[tokio::test]
    async fn GIVEN_duplicate_create_lobby_request_WHEN_create_lobby_THEN_error() {
        // GIVEN
        // create lobby request
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "test".to_string(),
            lobby_password: "test".to_string(),
            host_password: "test".to_string(),
            config: JeopardyConfig::test_default(),
        }; // create a lobby to conflict with
        let state = new_test_server(Some(create_lobby_request.clone())).await;

        // WHEN
        // attempt to create lobby again
        let (status_code, result) = super::create_lobby(
            State(state),
            Extension(Uuid::new_v4()),
            Json(create_lobby_request),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::CONFLICT);
        assert!(matches!(
            result,
            Json(CreateLobbyResponse {
                error: Some(..),
                ..
            })
        ));
    }

    // internal server error checks

    #[tokio::test]
    async fn GIVEN_failing_manager_during_check_conflict_WHEN_create_lobby_THEN_error() {
        // GIVEN
        let state = new_test_server_with_test_manager(None).await;
        state.manager().write().await.set_fail_after_n(0); // prevent lobby lookup from passing

        // WHEN
        let (status_code, result) = super::create_lobby(
            State(state.clone()),
            Extension(Uuid::new_v4()),
            Json(CreateLobbyRequest {
                lobby_name: "test".to_string(),
                lobby_password: "test".to_string(),
                host_password: "test".to_string(),
                config: JeopardyConfig::test_default(),
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(matches!(
            result,
            Json(CreateLobbyResponse {
                error: Some(..),
                ..
            })
        ));
    }

    #[tokio::test]
    async fn GIVEN_failing_manager_during_create_lobby_WHEN_create_lobby_THEN_error() {
        // GIVEN
        let state = new_test_server_with_test_manager(None).await;
        state.manager().write().await.set_fail_after_n(1); // let the lobby lookup pass but let the creation fail

        // WHEN
        let (status_code, result) = super::create_lobby(
            State(state.clone()),
            Extension(Uuid::new_v4()),
            Json(CreateLobbyRequest {
                lobby_name: "test".to_string(),
                lobby_password: "test".to_string(),
                host_password: "test".to_string(),
                config: JeopardyConfig::test_default(),
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(matches!(
            result,
            Json(CreateLobbyResponse {
                error: Some(..),
                ..
            })
        ));
    }
}
