use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use stagecrew::{
    lobby::LobbyError,
    manager::{ManagerEntry, ManagerError, PasswordProtectedLobby},
};
use uuid::Uuid;

use crate::{
    game::{Jeopardy, JeopardyCommand, JeopardyError, commands::host::HostCommand},
    server::{CredsValidatorGeneric, GenericJeopardyServerState, ManagerGeneric},
    web::handlers::validators::CredsValidator,
};

#[derive(Debug, Deserialize)]
pub struct DeleteLobbyRequest {
    pub force: bool, // set true to delete even if players are in the game
    pub lobby_password: String,
    pub host_password: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteLobbyResponse {
    request_id: String,
    error: Option<String>,
}

fn is_valid_delete_lobby_request(
    validator: &impl CredsValidator,
    lobby_id: &str,
    request: &DeleteLobbyRequest,
) -> bool {
    let lobby_id_ok = validator.is_valid_lobby_id(lobby_id);
    let lobby_pw_ok = validator.is_valid_lobby_password(&request.lobby_password);
    let host_pw_ok = validator.is_valid_host_password(&request.host_password);
    tracing::info!(
        "Valid lobby ID?: {lobby_id_ok} | lobby password?: {lobby_pw_ok} | host password?: {host_pw_ok}"
    );
    lobby_id_ok && lobby_pw_ok && host_pw_ok
}

/// Top level handler for deleting a lobby
/// in `JeopardyServer` given the correct credentials.
/// Deletes a lobby immediately if `force`.
/// Otherwise, checks if the lobby has players and errors out if so.
pub async fn delete_lobby<M: ManagerGeneric, C: CredsValidatorGeneric>(
    State(state): State<GenericJeopardyServerState<M, C>>,
    Path(lobby_id): Path<String>,
    Extension(request_id): Extension<Uuid>,
    Json(request): Json<DeleteLobbyRequest>,
) -> (StatusCode, Json<DeleteLobbyResponse>) {
    let request_id = request_id.to_string();
    let delete_lobby_span =
        tracing::info_span!("delete_lobby", lobby_id = lobby_id, force = request.force);
    let _enter = delete_lobby_span.enter();

    // validate request
    if !is_valid_delete_lobby_request(state.validator(), &lobby_id, &request) {
        let error_msg = "Invalid format given for one or more parameters of delete lobby request";
        tracing::warn!(error_msg);
        return (
            StatusCode::BAD_REQUEST,
            Json(DeleteLobbyResponse {
                request_id,
                error: Some(error_msg.to_string()),
            }),
        );
    }
    tracing::info!("Valid request format");

    // validate lobby
    let DeleteLobbyRequest {
        force,
        lobby_password,
        host_password,
    } = request;
    let mut manager_wg = state.manager().write().await;
    let entry = match manager_wg.get(&lobby_id) {
        Ok(lobby) => lobby,
        Err(e) => match e {
            ManagerError::EntryNotFound(_) => {
                tracing::warn!("Requested lobby not found. Possibly already deleted.");
                return (
                    StatusCode::NOT_FOUND,
                    Json(DeleteLobbyResponse {
                        request_id,
                        error: Some(format!("'{lobby_id}' not found. Possibly already deleted.")),
                    }),
                );
            }
            e => {
                tracing::error!("Unexpected manager error during delete lobby: {e}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(DeleteLobbyResponse {
                        request_id,
                        error: Some("Internal Server Error. Please try again later.".to_string()),
                    }),
                );
            }
        },
    };
    tracing::info!("Successfully found requested lobby");

    // authZ
    if !entry.is_correct_password(&lobby_password) {
        tracing::warn!("Incorrect lobby password");
        return (
            StatusCode::UNAUTHORIZED,
            Json(DeleteLobbyResponse {
                request_id,
                error: Some("Delete unauthorized.".to_string()),
            }),
        );
    }
    tracing::info!("Correct lobby password");

    // shutdown lobby if valid
    match shutdown_lobby(entry, force, host_password).await {
        Ok(opt) => {
            if let Some((status, error_msg)) = opt {
                // if we get some sort of response back,
                // that means we got an irrecoverable error
                return (
                    status,
                    Json(DeleteLobbyResponse {
                        request_id,
                        error: Some(error_msg),
                    }),
                );
            }
            // otherwise, continue
        }
        Err(e) => match e {
            LobbyError::ActorShutdown => {
                tracing::warn!("Lobby already shutdown. No-op")
            }
            other => {
                // we log this as an error!(..) bc it SHOULD NOT happen
                // but in both cases if the operation didn't work,
                // we're dropping the lobby anyways so it WILL get shut down
                // so we continue
                tracing::error!("Unexpected lobby error during lobby shutdown: {other}");
            }
        },
    }

    if let Err(e) = manager_wg.remove(&lobby_id) {
        // remove can only fail if the id was not found
        // and since we pre-check, that means something else happened
        tracing::error!("Failed to invoke manager to delete lobby: {e}");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(DeleteLobbyResponse {
                request_id,
                error: Some("Internal Server Error. Please try again later.".to_string()),
            }),
        );
    }
    tracing::info!("Lobby successfully deleted");
    (
        StatusCode::NO_CONTENT,
        Json(DeleteLobbyResponse {
            request_id,
            error: None,
        }),
    )
}

// helper function so i can use ? on the LobbyErrors
async fn shutdown_lobby(
    entry: &PasswordProtectedLobby<Jeopardy>,
    force: bool,
    host_password: String,
) -> Result<Option<(StatusCode, String)>, LobbyError> {
    tracing::info!("Force flag: {force}");
    if !force {
        // if the request is !force and the lobby isn't empty, we cannot delete
        let count = entry.lobby().player_count().await?;
        if count > 0 {
            let error_msg = "Attempted to delete a non-empty lobby without the force flag.";
            tracing::warn!(error_msg);
            return Ok(Some((StatusCode::CONFLICT, error_msg.to_string())));
        }
        tracing::info!("Lobby is empty and will be shutdown and deleted");
    } else {
        tracing::info!("Lobby will be shutdown and deleted even if players remain");
    }

    // dummy command to check if the host password is correct
    let password_result = entry
        .lobby()
        .send_game_event_and_wait(JeopardyCommand::Host {
            host_password,
            command: HostCommand::GetBuzzerQueue,
        })
        .await?;
    if let Err(e) = password_result {
        match e {
            JeopardyError::IncorrectHostPassword => {
                tracing::warn!("Host password incorrect");
            }
            other => {
                // this case, manual deletion of the lobby is disabled
                // therefore, we rely on all players disconnecting to delete the lobby
                tracing::error!(
                    "Unexpected response given when attempting to check host password: {other}. Treating as incorrect password."
                );
            }
        }
        return Ok(Some((
            StatusCode::UNAUTHORIZED,
            "Delete unauthorized.".to_string(),
        )));
    }
    tracing::info!("Host password correct");

    // shutdown lobby - if it's already shutdown somehow
    // we're dropping anyways so it will get cleaned up, more of a formality

    // note: there is no unit test for this case failing
    // bc i cannot induce the shutdown after the password check.
    // however, the function is already unit tested in `stagecrew`
    // and type signatures already guarantee that we handle this. so it's ok
    entry
        .lobby()
        .shutdown()
        .await? // wait for send to actor
        .await?; // wait until actual shutdown
    tracing::info!("Lobby successfully shut down");
    Ok(None)
}

#[cfg(test)]
#[allow(non_snake_case)]
mod delete_lobby_tests {
    use stagecrew::manager::Manager;

    use super::*;
    use crate::{
        game::jeopardy::config::JeopardyConfig,
        server::TestDefault,
        web::handlers::create_lobby::{
            CreateLobbyRequest,
            create_lobby_test_util::{new_test_server, new_test_server_with_test_manager},
        },
    };

    #[tokio::test] // the non-force variant of this test is in create_lobby()
    async fn GIVEN_valid_force_delete_lobby_request_WHEN_delete_lobby_THEN_ok() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;

        // WHEN
        let (status_code, response) = super::delete_lobby(
            State(state.clone()),
            Path(create_lobby_request.lobby_name.clone()),
            Extension(Uuid::new_v4()),
            Json(DeleteLobbyRequest {
                force: true,
                lobby_password: create_lobby_request.lobby_password,
                host_password: create_lobby_request.host_password,
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::NO_CONTENT);
        assert!(matches!(
            response,
            Json(DeleteLobbyResponse { error: None, .. })
        ));
        let lobby_exists = state
            .manager()
            .write()
            .await
            .has(&create_lobby_request.lobby_name)
            .unwrap();
        assert_eq!(false, lobby_exists); // lobby was deleted
    }

    #[tokio::test]
    async fn GIVEN_invalid_format_delete_lobby_request_WHEN_delete_lobby_THEN_error() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;

        for i in 0..3 {
            // switch which field has the invalid format
            let lobby_name = if i == 0 {
                "".to_string()
            } else {
                create_lobby_request.lobby_name.clone()
            };
            let lobby_password = if i == 1 {
                "".to_string()
            } else {
                create_lobby_request.lobby_password.clone()
            };
            let host_password = if i == 2 {
                "".to_string()
            } else {
                create_lobby_request.host_password.clone()
            };

            // WHEN
            let (status_code, response) = super::delete_lobby(
                State(state.clone()),
                Path(lobby_name),
                Extension(Uuid::new_v4()),
                Json(DeleteLobbyRequest {
                    force: true,
                    lobby_password,
                    host_password,
                }),
            )
            .await;

            // THEN
            assert_eq!(status_code, StatusCode::BAD_REQUEST);
            assert!(matches!(
                response,
                Json(DeleteLobbyResponse {
                    error: Some(..),
                    ..
                })
            ));
            let lobby_exists = state
                .manager()
                .write()
                .await
                .has(&create_lobby_request.lobby_name)
                .unwrap();
            assert!(lobby_exists); // lobby was not deleted
        }
    }

    #[tokio::test]
    async fn GIVEN_nonexistant_lobby_delete_lobby_request_WHEN_delete_lobby_THEN_error() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;

        // WHEN
        let (status_code, response) = super::delete_lobby(
            State(state.clone()),
            Path("MISSING".to_string()), // doesn't exist in the map
            Extension(Uuid::new_v4()),
            Json(DeleteLobbyRequest {
                force: true,
                lobby_password: create_lobby_request.lobby_password,
                host_password: create_lobby_request.host_password,
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::NOT_FOUND);
        assert!(matches!(
            response,
            Json(DeleteLobbyResponse {
                error: Some(..),
                ..
            })
        ));
        let lobby_exists = state
            .manager()
            .write()
            .await
            .has(&create_lobby_request.lobby_name)
            .unwrap();
        assert!(lobby_exists); // lobby was not deleted
    }

    // incorrect password tests

    #[tokio::test]
    async fn GIVEN_incorrect_lobby_password_delete_lobby_request_WHEN_delete_lobby_THEN_error() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;

        // WHEN
        let (status_code, response) = super::delete_lobby(
            State(state.clone()),
            Path(create_lobby_request.lobby_name.clone()),
            Extension(Uuid::new_v4()),
            Json(DeleteLobbyRequest {
                force: true,
                lobby_password: "INCORRECT".to_string(),
                host_password: create_lobby_request.host_password,
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::UNAUTHORIZED);
        assert!(matches!(
            response,
            Json(DeleteLobbyResponse {
                error: Some(..),
                ..
            })
        ));
        let lobby_exists = state
            .manager()
            .write()
            .await
            .has(&create_lobby_request.lobby_name)
            .unwrap();
        assert!(lobby_exists); // lobby was not deleted
    }

    #[tokio::test]
    async fn GIVEN_incorrect_host_password_delete_lobby_request_WHEN_delete_lobby_THEN_error() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;

        // WHEN
        let (status_code, response) = super::delete_lobby(
            State(state.clone()),
            Path(create_lobby_request.lobby_name.clone()),
            Extension(Uuid::new_v4()),
            Json(DeleteLobbyRequest {
                force: true,
                lobby_password: create_lobby_request.lobby_password,
                host_password: "INCORRECT".to_string(),
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::UNAUTHORIZED);
        assert!(matches!(
            response,
            Json(DeleteLobbyResponse {
                error: Some(..),
                ..
            })
        ));
        let lobby_exists = state
            .manager()
            .write()
            .await
            .has(&create_lobby_request.lobby_name)
            .unwrap();
        assert!(lobby_exists); // lobby was not deleted
    }

    // lobby error tests (already shutdown during delete)

    #[tokio::test]
    async fn GIVEN_check_host_password_with_shutdown_lobby_WHEN_delete_lobby_THEN_ok() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;
        state
            .manager()
            .read()
            .await
            .get(&create_lobby_request.lobby_name)
            .unwrap()
            .lobby()
            .shutdown() // shutdown lobby so it fails at checking host password and no-ops
            .await
            .unwrap();

        // WHEN
        let (status_code, response) = super::delete_lobby(
            State(state.clone()),
            Path(create_lobby_request.lobby_name.clone()),
            Extension(Uuid::new_v4()),
            Json(DeleteLobbyRequest {
                force: true, // skips the player count check
                lobby_password: create_lobby_request.lobby_password,
                host_password: create_lobby_request.host_password,
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::NO_CONTENT); // delete should still pass
        assert!(matches!(
            response,
            Json(DeleteLobbyResponse { error: None, .. })
        ));
        let lobby_exists = state
            .manager()
            .write()
            .await
            .has(&create_lobby_request.lobby_name)
            .unwrap();
        assert_eq!(false, lobby_exists); // lobby was deleted
    }

    #[tokio::test]
    async fn GIVEN_check_nonempty_with_shutdown_lobby_WHEN_delete_lobby_THEN_ok() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;
        state
            .manager()
            .read()
            .await
            .get(&create_lobby_request.lobby_name)
            .unwrap()
            .lobby()
            .shutdown() // shutdown lobby so it fails at checking player count and no-ops
            .await
            .unwrap();

        // WHEN
        let (status_code, response) = super::delete_lobby(
            State(state.clone()),
            Path(create_lobby_request.lobby_name.clone()),
            Extension(Uuid::new_v4()),
            Json(DeleteLobbyRequest {
                force: false, // will check if the lobby is empty before shutting down
                lobby_password: create_lobby_request.lobby_password,
                host_password: create_lobby_request.host_password,
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::NO_CONTENT); // delete should still pass
        assert!(matches!(
            response,
            Json(DeleteLobbyResponse { error: None, .. })
        ));
        let lobby_exists = state
            .manager()
            .write()
            .await
            .has(&create_lobby_request.lobby_name)
            .unwrap();
        assert_eq!(false, lobby_exists); // lobby was deleted
    }

    // internal server error tests

    #[tokio::test]
    async fn GIVEN_failing_manager_during_check_exist_WHEN_delete_lobby_THEN_error() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server_with_test_manager(Some(create_lobby_request.clone())).await;
        state.manager().write().await.set_always_fail(); // prevent lookup from passing

        // WHEN
        let (status_code, response) = super::delete_lobby(
            State(state.clone()),
            Path(create_lobby_request.lobby_name.clone()),
            Extension(Uuid::new_v4()),
            Json(DeleteLobbyRequest {
                force: true,
                lobby_password: create_lobby_request.lobby_password,
                host_password: create_lobby_request.host_password,
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(matches!(
            response,
            Json(DeleteLobbyResponse {
                error: Some(..),
                ..
            })
        ));
        let mut manager = state.manager().write().await;
        manager.set_never_fail();
        let lobby_exists = manager.has(&create_lobby_request.lobby_name).unwrap();
        assert!(lobby_exists); // lobby was not deleted
    }

    #[tokio::test]
    async fn GIVEN_failing_manager_during_delete_WHEN_delete_lobby_THEN_error() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server_with_test_manager(Some(create_lobby_request.clone())).await;
        state.manager().write().await.set_fail_after_n(1); // allow lookup to pass but delete to fail

        // WHEN
        let (status_code, response) = super::delete_lobby(
            State(state.clone()),
            Path(create_lobby_request.lobby_name.clone()),
            Extension(Uuid::new_v4()),
            Json(DeleteLobbyRequest {
                force: true,
                lobby_password: create_lobby_request.lobby_password,
                host_password: create_lobby_request.host_password,
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(matches!(
            response,
            Json(DeleteLobbyResponse {
                error: Some(..),
                ..
            })
        ));
        let mut manager = state.manager().write().await;
        manager.set_never_fail();
        let lobby_exists = manager.has(&create_lobby_request.lobby_name).unwrap();
        assert!(lobby_exists); // lobby was not deleted
    }
}
