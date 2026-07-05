use axum::Extension;
use axum::extract::Path;
use axum::{Json, extract::State, http::StatusCode};
use serde::{Deserialize, Serialize};
use stagecrew::lobby::Lobby;
use stagecrew::manager::PasswordProtectedLobby;
use stagecrew::player::player_map::PlayerMap;
use tokio::time::sleep;
use tracing::Instrument;
use uuid::Uuid;

use crate::game::Jeopardy;
use crate::game::jeopardy::config::JeopardyConfig;
use crate::server::{CredsValidatorGeneric, GenericJeopardyServerState, ManagerGeneric};
use crate::web::handlers::delete_lobby::DeleteLobbyRequest;
use crate::web::handlers::validators::CredsValidator;

#[derive(Debug, Deserialize, Clone)]
pub struct CreateLobbyRequest {
    pub lobby_name: String,
    pub lobby_password: String,
    pub host_password: String,
    pub config: JeopardyConfig,
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
        "Valid lobby ID?: {lobby_id_ok} | lobby password?: {lobby_pw_ok} | host password?: {host_pw_ok}"
    );
    lobby_id_ok && lobby_pw_ok && host_pw_ok
}
/// Top level handler for creating a lobby in `JeopardyServer`.
/// Creates a lobby and auto-deletes it if the lobby is still empty after the grace period.
pub async fn create_lobby<M: ManagerGeneric, C: CredsValidatorGeneric>(
    State(state): State<GenericJeopardyServerState<M, C>>,
    Extension(request_id_uuid): Extension<Uuid>,
    Json(request): Json<CreateLobbyRequest>,
) -> (StatusCode, Json<CreateLobbyResponse>) {
    // although lobbies are ephemeral i don't like logging passwords
    let create_lobby_span = tracing::info_span!("create_lobby", lobby_id = request.lobby_name);
    let _enter = create_lobby_span.enter();
    let request_id = request_id_uuid.to_string();

    // validate request
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
    tracing::info!("Valid request format");

    // check conflict
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
                    error: Some("Internal Server Error. Please try again later.".to_string()),
                }),
            );
        }
    }

    // create lobby based on input
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
    let lobby_entry =
        PasswordProtectedLobby::new(lobby_name.clone(), lobby_password.clone(), new_lobby);

    // add lobby to manager
    if let Err(e) = state.manager().write().await.add(&lobby_name, lobby_entry) {
        tracing::error!("Failed to create lobby: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(CreateLobbyResponse {
                request_id,
                error: Some("Internal Server Error. Please try again later.".to_string()),
            }),
        );
    }
    tracing::info!("Lobby successfully created");
    tokio::spawn(
        async move {
            // in case a user creates a lobby but logs off,
            // spawn a task to delete the lobby if it is empty after the grace period
            // if the lobby gets manually deleted, it will just no-op
            sleep(state.config().lobby_cleanup_grace_period).await;
            let cleanup_request = DeleteLobbyRequest {
                force: false, // do not delete if there are players
                lobby_password,
                host_password,
            };
            super::delete_lobby(
                State(state),
                Path(lobby_name),
                Extension(request_id_uuid), // technically unused
                Json(cleanup_request),
            )
            .await
        } // inherit the span to differentiate between other delete calls
        .instrument(create_lobby_span.clone()),
    );
    (
        StatusCode::CREATED,
        Json(CreateLobbyResponse {
            request_id,
            error: None,
        }),
    )
}

#[cfg(test)]
pub mod create_lobby_test_util {
    use std::sync::Arc;

    use super::*;
    use crate::{
        game::{
            Jeopardy,
            player::{JeopardyPlayer, JeopardyPlayerEvent},
        },
        server::{JeopardyServer, JeopardyServerState, ServerConfig, TestDefault},
        web::handlers::validators::nonzero_ascii::NonZeroAsciiValidator,
    };
    use stagecrew::manager::{Manager, ManagerEntry, test_manager_constructs::TestManager};
    use tokio::sync::mpsc;

    // helper function for the positive test case
    // primarily used for delete_lobby tests as they need a lobby to delete one
    pub async fn new_test_server(create_lobby: Option<CreateLobbyRequest>) -> JeopardyServerState {
        // GIVEN
        let state = Arc::new(JeopardyServer::from_config(ServerConfig::test_default()));

        if let Some(request) = create_lobby {
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
        state
    }

    // create a jeopardy server with a test player given their ID
    // returns the server state and mpsc::Receiver<_> to listen for broadcasts
    pub async fn new_test_server_with_player(
        create_lobby: CreateLobbyRequest,
        player_id: &str,
    ) -> (JeopardyServerState, mpsc::Receiver<JeopardyPlayerEvent>) {
        // GIVEN
        let state = new_test_server(Some(create_lobby.clone())).await;
        let (tx, rx) = mpsc::channel(1);
        let player = JeopardyPlayer::new(player_id.to_string(), tx);
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
            state.manager().write().await.set_never_fail();
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
            assert!(lobby_exists);
            state.manager().write().await.reset(); // don't count the create lobby operations towards permits
        }
        state
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod create_lobby_tests {

    use std::time::Duration;

    use super::*;
    use crate::{
        server::TestDefault,
        web::handlers::create_lobby::create_lobby_test_util::{
            new_test_server, new_test_server_with_player, new_test_server_with_test_manager,
        },
    };
    use stagecrew::manager::Manager;

    #[tokio::test]
    async fn GIVEN_valid_create_lobby_request_WHEN_create_lobby_THEN_success() {
        new_test_server(Some(CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        }))
        .await;
    }

    #[tokio::test]
    async fn GIVEN_invalid_format_create_lobby_request_WHEN_create_lobby_THEN_error() {
        // GIVEN
        let valid_lobby_name = "lobby_name".to_string();
        let valid_lobby_password = "lobby_password".to_string();
        let valid_host_password = "host_password".to_string();
        let state = new_test_server(None).await;

        for i in 0..3 {
            // switch which field has the invalid format
            let lobby_name = if i == 0 {
                "".to_string()
            } else {
                valid_lobby_name.clone()
            };
            let lobby_password = if i == 1 {
                "".to_string()
            } else {
                valid_lobby_password.clone()
            };
            let host_password = if i == 2 {
                "".to_string()
            } else {
                valid_host_password.clone()
            };

            // WHEN
            let (status_code, response) = super::create_lobby(
                State(state.clone()),
                Extension(Uuid::new_v4()),
                Json(CreateLobbyRequest {
                    lobby_name,
                    lobby_password,
                    host_password,
                    config: JeopardyConfig::test_default(),
                }),
            )
            .await;

            // THEN
            assert_eq!(status_code, StatusCode::BAD_REQUEST);
            assert!(matches!(
                response,
                Json(CreateLobbyResponse {
                    error: Some(..),
                    ..
                })
            ));
            let lobby_exists = state
                .manager()
                .write()
                .await
                .has(&valid_lobby_name)
                .unwrap();
            assert_eq!(false, lobby_exists); // lobby was not created
        }
    }

    #[tokio::test]
    async fn GIVEN_invalid_jeopardy_config_WHEN_create_lobby_THEN_error() {
        // GIVEN
        let state = new_test_server(None).await;
        let lobby_name = "lobby_name".to_string();

        // WHEN
        let (status_code, response) = super::create_lobby(
            State(state.clone()),
            Extension(Uuid::new_v4()),
            Json(CreateLobbyRequest {
                lobby_name: lobby_name.clone(),
                lobby_password: "lobby_password".to_string(),
                host_password: "host_password".to_string(),
                config: JeopardyConfig::invalid_default(), // test function (not in actual API)
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::BAD_REQUEST);
        assert!(matches!(
            response,
            Json(CreateLobbyResponse {
                error: Some(..),
                ..
            })
        ));
        let lobby_exists = state.manager().read().await.has(&lobby_name).unwrap();
        assert_eq!(false, lobby_exists); // was not created
    }

    #[tokio::test]
    async fn GIVEN_duplicate_create_lobby_request_WHEN_create_lobby_THEN_error() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(), // create a lobby to conflict with
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;
        let manager_len_before = state.manager().read().await.len().unwrap();

        // WHEN
        // attempt to create lobby again
        let (status_code, result) = super::create_lobby(
            State(state.clone()),
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
        // ensure unchanged (can't use has(..) bc it's a duplicate request)
        let manager_len_after = state.manager().read().await.len().unwrap();
        assert_eq!(manager_len_before, manager_len_after);
    }

    // internal server error checks

    #[tokio::test]
    async fn GIVEN_failing_manager_during_check_conflict_WHEN_create_lobby_THEN_error() {
        // GIVEN
        let state = new_test_server_with_test_manager(None).await;
        state.manager().write().await.set_always_fail(); // prevent lobby lookup from passing
        let lobby_name = "lobby_name".to_string();

        // WHEN
        let (status_code, result) = super::create_lobby(
            State(state.clone()),
            Extension(Uuid::new_v4()),
            Json(CreateLobbyRequest {
                lobby_name: lobby_name.clone(),
                lobby_password: "lobby_password".to_string(),
                host_password: "host_password".to_string(),
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
        state.manager().write().await.set_never_fail();
        let lobby_exists = state.manager().read().await.has(&lobby_name).unwrap();
        assert_eq!(false, lobby_exists); // was not created
    }

    #[tokio::test]
    async fn GIVEN_failing_manager_during_create_lobby_WHEN_create_lobby_THEN_error() {
        // GIVEN
        let state = new_test_server_with_test_manager(None).await;
        state.manager().write().await.set_fail_after_n(1); // let the lobby lookup pass but let the creation fail
        let lobby_name = "lobby_name".to_string();

        // WHEN
        let (status_code, result) = super::create_lobby(
            State(state.clone()),
            Extension(Uuid::new_v4()),
            Json(CreateLobbyRequest {
                lobby_name: lobby_name.clone(),
                lobby_password: "lobby_password".to_string(),
                host_password: "host_password".to_string(),
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
        state.manager().write().await.set_never_fail();
        let lobby_exists = state.manager().read().await.has(&lobby_name).unwrap();
        assert_eq!(false, lobby_exists); // was not created
    }

    // grace period cleanup tests

    #[tokio::test]
    async fn GIVEN_empty_lobby_WHEN_grace_period_cleanup_THEN_ok() {
        // GIVEN
        let lobby_name = "lobby_name".to_string();
        let state = new_test_server(Some(CreateLobbyRequest {
            lobby_name: lobby_name.clone(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        }))
        .await;

        // WHEN
        // wait for lobby to be deleted
        let grace_period = state.config().lobby_cleanup_grace_period + Duration::from_secs(1);
        sleep(grace_period).await;

        // THEN
        let lobby_exists = state.manager().read().await.has(&lobby_name).unwrap();
        assert_eq!(false, lobby_exists); // gone bc it was empty
    }

    #[tokio::test]
    async fn GIVEN_nonempty_lobby_WHEN_grace_period_cleanup_THEN_ok() {
        // GIVEN
        let lobby_name = "lobby_name".to_string();
        let request = CreateLobbyRequest {
            lobby_name: lobby_name.clone(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let (state, _) = new_test_server_with_player(request, "test_player").await;

        // WHEN
        // wait for lobby cleanup task to wake up and no-op
        let after_grace_period = state.config().lobby_cleanup_grace_period + Duration::from_secs(1);
        sleep(after_grace_period).await;

        // THEN
        let lobby_exists = state.manager().read().await.has(&lobby_name).unwrap();
        assert!(lobby_exists);
    }
}
