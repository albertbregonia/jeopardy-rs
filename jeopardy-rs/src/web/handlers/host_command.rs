use axum::{
    Extension, Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    game::{JeopardyCommand, JeopardyCommandResponse, JeopardyError},
    server::{CredsValidatorGeneric, GenericJeopardyServerState, ManagerGeneric},
    web::handlers::validators::CredsValidator,
};
use stagecrew::manager::{ManagerEntry, ManagerError};

#[derive(Debug, Deserialize)]
pub struct HostRequest {
    pub lobby_password: String,
    pub command: JeopardyCommand,
}

#[derive(Serialize)]
pub struct HostResponse {
    pub request_id: String,
    #[serde(serialize_with = "serialize_host_response_result")]
    pub result: Result<JeopardyCommandResponse, String>, // command response or error msg
}

const HOST_RESPONSE_RESULT_ERROR_KEY: &str = "error";
const HOST_RESPONSE_RESULT_VALUE_KEY: &str = "value";

fn serialize_host_response_result<S: serde::Serializer>(
    result: &Result<JeopardyCommandResponse, String>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    // we want to use result internally
    // but outwardly, we don't want to have API calls have to handle
    // "Err" and "Ok" Rust formats bc that's too low level.
    // therefore, make it standard JSON and use Option<> so we get nulls
    serde_json::json!({
        HOST_RESPONSE_RESULT_VALUE_KEY: result.as_ref().ok(),
        HOST_RESPONSE_RESULT_ERROR_KEY: result.as_ref().err(),
    })
    .serialize(serializer)
}

fn is_valid_host_request(
    validator: &impl CredsValidator,
    lobby_id: &str,
    request: &HostRequest,
) -> bool {
    let lobby_id_ok = validator.is_valid_lobby_id(lobby_id);
    let lobby_pw_ok = validator.is_valid_lobby_password(&request.lobby_password);
    let special = match &request.command {
        JeopardyCommand::Host { host_password, .. } => {
            let host_pw_ok = validator.is_valid_host_password(host_password);
            tracing::info!("Valid host password?: {host_pw_ok}");
            host_pw_ok
        }
        JeopardyCommand::Player { player_id, .. } => {
            let username_ok = validator.is_valid_username(player_id);
            tracing::info!("Valid target player ID?: {username_ok}");
            username_ok
        }
    };
    tracing::info!("Valid lobby ID?: {lobby_id_ok} | lobby password?: {lobby_pw_ok}");
    lobby_id_ok && lobby_pw_ok && special
}

/// Top level handler for host commands to control the Jeopardy game.
/// Important note: the host is NOT considered a player and does not have a live connection (websocket)
/// This is because the host merely sends requests and seldom ingests them (eg. buzzer queue). Therefore,
/// it is much easier to simply use HTTP requests 99% percent of the time than rather manage the connection.
/// For ingesting events (ie. buzzer queue), we can poll every second or so (players are not going to have microsecond reaction time)
/// whenever there is a question shown and then disable polling after the answer is shown.
/// Therefore, this design choice is much more optimized.
/// This also means that the host doesn't count towards the lobby being empty.
/// Therefore, if the lobby is deleted bc empty, then their call will simply fail and they can remake.
pub async fn handle_host_command<M: ManagerGeneric, C: CredsValidatorGeneric>(
    State(state): State<GenericJeopardyServerState<M, C>>,
    Path(lobby_id): Path<String>,
    Extension(request_id): Extension<Uuid>,
    Json(request): Json<HostRequest>,
) -> (StatusCode, Json<HostResponse>) {
    let request_id = request_id.to_string();
    // we want to log commands in the span but not passwords
    let command_str_for_span = match request.command {
        JeopardyCommand::Host { ref command, .. } => format!("{command:?}"),
        ref other => format!("{other:?}"),
    };
    let host_span = tracing::info_span!(
        "handle_host_command",
        lobby_id = lobby_id,
        command = command_str_for_span,
    );
    let _enter = host_span.enter();

    // validate request
    if !is_valid_host_request(state.validator(), &lobby_id, &request) {
        let error_msg = "Invalid format given for one or more parameters of host command request";
        tracing::warn!(error_msg);
        return (
            StatusCode::BAD_REQUEST,
            Json(HostResponse {
                request_id,
                result: Err(error_msg.to_string()),
            }),
        );
    }
    tracing::info!("Valid request format");

    // get target lobby
    let HostRequest {
        lobby_password,
        command,
    } = request;
    let manager_rg = state.manager().read().await;
    let lobby = match manager_rg.get(&lobby_id) {
        Ok(lobby) => lobby,
        Err(e) => match e {
            ManagerError::EntryNotFound(_) => {
                tracing::warn!("Lobby not found: {e}");
                return (
                    StatusCode::NOT_FOUND,
                    Json(HostResponse {
                        request_id,
                        result: Err(
                            "Lobby not found. Possibly deleted due to inactivity.".to_string()
                        ),
                    }),
                );
            }
            other => {
                tracing::error!("Failed to perform lobby lookup for host command: {other}");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(HostResponse {
                        request_id,
                        result: Err("Internal Server Error. Please try again.".to_string()),
                    }),
                );
            }
        },
    };
    tracing::info!("Successfully found target lobby");

    // authZ
    if !lobby.is_correct_password(&lobby_password) {
        tracing::warn!("Incorrect lobby password");
        return (
            StatusCode::UNAUTHORIZED,
            Json(HostResponse {
                request_id,
                result: Err("Invalid lobby password".to_string()),
            }),
        );
    }
    tracing::info!("Correct lobby password");

    // send command and get response
    // Result<Result<>> because the call could fail
    // and then the inner is the response from the game
    let result = lobby.lobby().send_game_event_and_wait(command).await;
    drop(manager_rg); // release read guard now that we've already sent our request to the lobby
    tracing::info!("Sent command to lobby");

    let response = match result {
        Ok(response) => response,
        Err(e) => {
            // we don't have to care about the type here
            // - UserIDConflict should not happen
            // - ActorShutdown could happen but we should never have an entry
            //   in the manager without an active lobby (therefore, 500)
            // log and emit 500 either way
            tracing::error!("Unexpected lobby error during host command handling: {e}");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(HostResponse {
                    request_id,
                    result: Err("Internal Server Error. Please try again later.".to_string()),
                }),
            );
        }
    };
    match response {
        Ok(response) => {
            tracing::info!("Command successful. Response: {response:?}");
            (
                StatusCode::OK,
                Json(HostResponse {
                    request_id,
                    result: Ok(response),
                }),
            )
        }
        Err(e) => match e {
            // handle user error
            JeopardyError::IncorrectHostPassword => {
                tracing::warn!("Incorrect host password. Command unauthorized.");
                (
                    StatusCode::UNAUTHORIZED,
                    Json(HostResponse {
                        request_id,
                        result: Err(e.to_string()),
                    }),
                )
            }
            other => {
                tracing::warn!("User error received from Jeopardy game: {other}");
                (
                    StatusCode::BAD_REQUEST,
                    Json(HostResponse {
                        request_id,
                        result: Err(other.to_string()),
                    }),
                )
            }
        },
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod host_command_tests {

    use axum::{
        Extension, Json,
        extract::{Path, State},
        http::StatusCode,
    };
    use stagecrew::manager::{Manager, ManagerEntry};
    use uuid::Uuid;

    use crate::{
        game::{
            JeopardyCommand, JeopardyCommandResponse,
            commands::{
                host::{HostCommand, HostCommandResponse},
                player::PlayerCommand,
            },
            jeopardy::{board::Board, config::JeopardyConfig, final_jeopardy::FinalJeopardy},
        },
        server::TestDefault,
        web::handlers::{
            create_lobby::{
                CreateLobbyRequest,
                create_lobby_test_util::{
                    new_test_server, new_test_server_with_player, new_test_server_with_test_manager,
                },
            },
            host_command::{
                HOST_RESPONSE_RESULT_ERROR_KEY, HOST_RESPONSE_RESULT_VALUE_KEY, HostRequest,
                HostResponse,
            },
        },
    };

    // positive test cases

    #[tokio::test]
    async fn GIVEN_valid_host_command_WHEN_handle_host_command_THEN_ok() {
        // GIVEN

        // create 2 boards to run commands against
        let boards = vec![
            Board::test_default_from_counts(1, 1),
            Board::test_default_from_counts(5, 5),
        ];
        let config = JeopardyConfig::new(boards.clone(), FinalJeopardy::test_default()).unwrap();

        // create lobby with player to run commands against
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config,
        };
        let player_id = "test_player".to_string();
        let (state, _) =
            new_test_server_with_player(create_lobby_request.clone(), player_id.clone()).await;

        let commands = [
            HostCommand::GetBuzzerQueue,
            HostCommand::ClearBuzzerQueue,
            HostCommand::ShowCurrentAnswer,
            HostCommand::ShowFinalJeopardyQuestion,
            HostCommand::ShowFinalJeopardyAnswer,
            HostCommand::SetPoints {
                player_id: player_id.clone(),
                points: 100,
            },
            HostCommand::UpdatePoints {
                player_id,
                delta: -1,
            },
        ];

        for command in commands {
            let host_request = HostRequest {
                lobby_password: create_lobby_request.lobby_password.clone(),
                command: JeopardyCommand::Host {
                    host_password: create_lobby_request.host_password.clone(),
                    command,
                },
            };
            // WHEN
            let (status, response) = super::handle_host_command(
                State(state.clone()),
                Path(create_lobby_request.lobby_name.clone()),
                Extension(Uuid::new_v4()),
                Json(host_request),
            )
            .await;

            // THEN - simply test that it responses OK, the values are validated at the handler level
            assert_eq!(status, StatusCode::OK);
            assert!(matches!(
                response,
                Json(HostResponse {
                    result: Ok(JeopardyCommandResponse::Host(..)),
                    ..
                })
            ))
        }

        // test that every `ShowQuestion` passes - more exahustive than really necessary tbh
        for board_index in 0..boards.len() {
            let board = &boards[board_index];
            for category_index in 0..board.categories().len() {
                let category = &board.categories()[category_index];
                for question_index in 0..category.questions().len() {
                    let host_request = HostRequest {
                        lobby_password: create_lobby_request.lobby_password.clone(),
                        command: JeopardyCommand::Host {
                            host_password: create_lobby_request.host_password.clone(),
                            command: HostCommand::ShowQuestion {
                                board_index,
                                category_index,
                                question_index,
                            },
                        },
                    };
                    // WHEN
                    let (status, response) = super::handle_host_command(
                        State(state.clone()),
                        Path(create_lobby_request.lobby_name.clone()),
                        Extension(Uuid::new_v4()),
                        Json(host_request),
                    )
                    .await;

                    // THEN
                    assert_eq!(status, StatusCode::OK);
                    assert!(matches!(
                        response,
                        Json(HostResponse {
                            result: Ok(JeopardyCommandResponse::Host(..)),
                            ..
                        })
                    ))
                }
            }
        }

        // test that every `ShowBoard` passes
        for board_index in 0..boards.len() {
            let host_request = HostRequest {
                lobby_password: create_lobby_request.lobby_password.clone(),
                command: JeopardyCommand::Host {
                    host_password: create_lobby_request.host_password.clone(),
                    command: HostCommand::ShowBoard { board_index },
                },
            };
            // WHEN
            let (status, response) = super::handle_host_command(
                State(state.clone()),
                Path(create_lobby_request.lobby_name.clone()),
                Extension(Uuid::new_v4()),
                Json(host_request),
            )
            .await;

            // THEN
            assert_eq!(status, StatusCode::OK);
            assert!(matches!(
                response,
                Json(HostResponse {
                    result: Ok(JeopardyCommandResponse::Host(..)),
                    ..
                })
            ))
        }
    }

    #[tokio::test]
    async fn GIVEN_valid_player_command_WHEN_handle_host_command_THEN_ok() {
        // GIVEN
        // create lobby with player to run commands against
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let player_id = "test_player".to_string();
        let (state, _) =
            new_test_server_with_player(create_lobby_request.clone(), player_id.clone()).await;

        let commands = [
            PlayerCommand::Buzz,
            PlayerCommand::GetFreeResponse,
            PlayerCommand::GetPoints,
            PlayerCommand::GetScoreboard,
            PlayerCommand::GetWager,
            PlayerCommand::SetWager(0),
            PlayerCommand::Refresh,
            PlayerCommand::SetFreeResponse("free_response".to_string()),
        ];

        for command in commands {
            let host_request = HostRequest {
                lobby_password: create_lobby_request.lobby_password.clone(),
                command: JeopardyCommand::Player {
                    player_id: player_id.clone(),
                    command,
                },
            };
            // WHEN
            let (status, response) = super::handle_host_command(
                State(state.clone()),
                Path(create_lobby_request.lobby_name.clone()),
                Extension(Uuid::new_v4()),
                Json(host_request),
            )
            .await;

            // THEN - simply test that it responses OK, the values are validated at the handler level
            assert_eq!(status, StatusCode::OK);
            assert!(matches!(
                response,
                Json(HostResponse {
                    result: Ok(JeopardyCommandResponse::Player(..)),
                    ..
                })
            ))
        }
    }

    // bad request tests

    #[tokio::test]
    async fn GIVEN_invalid_format_host_command_WHEN_handle_host_command_THEN_error() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let player_id = "test_player".to_string(); // we need a player to run PlayerCommand against
        let (state, _) =
            new_test_server_with_player(create_lobby_request.clone(), player_id.clone()).await;

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
            let (status_code, response) = super::handle_host_command(
                State(state.clone()),
                Path(lobby_name.clone()),
                Extension(Uuid::new_v4()),
                Json(HostRequest {
                    lobby_password: lobby_password.clone(),
                    command: JeopardyCommand::Host {
                        host_password: host_password.clone(),
                        command: HostCommand::GetBuzzerQueue,
                    },
                }),
            )
            .await;

            // THEN
            assert_eq!(status_code, StatusCode::BAD_REQUEST);
            assert!(matches!(
                response,
                Json(HostResponse {
                    result: Err(..),
                    ..
                })
            ));

            // if we're testing invalid lobby name / password
            // repeat the test with a player command instead of a host command
            // bc the host can induce a command for a player as well as their host commands
            // ensure that errors out the same way
            if lobby_name == "" || lobby_password == "" {}
        }
    }

    #[tokio::test]
    async fn GIVEN_invalid_format_player_command_WHEN_handle_host_command_THEN_error() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let player_id = "test_player".to_string(); // we need a player to run PlayerCommand against
        let (state, _) =
            new_test_server_with_player(create_lobby_request.clone(), player_id.clone()).await;

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
            let player_id = if i == 2 {
                "".to_string()
            } else {
                player_id.clone()
            };

            // WHEN
            let (status_code, response) = super::handle_host_command(
                State(state.clone()),
                Path(lobby_name.clone()),
                Extension(Uuid::new_v4()),
                Json(HostRequest {
                    lobby_password: lobby_password.clone(),
                    command: JeopardyCommand::Player {
                        player_id: player_id.clone(),
                        command: PlayerCommand::Buzz,
                    },
                }),
            )
            .await;

            // THEN
            assert_eq!(status_code, StatusCode::BAD_REQUEST);
            assert!(matches!(
                response,
                Json(HostResponse {
                    result: Err(..),
                    ..
                })
            ));
        }
    }

    #[tokio::test]
    async fn GIVEN_nonexistant_lobby_WHEN_handle_host_command_THEN_error() {
        // GIVEN
        // create lobby with player to run commands against
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;

        // WHEN
        let (status_code, response) = super::handle_host_command(
            State(state),
            Path("MISSING".to_string()),
            Extension(Uuid::new_v4()),
            Json(HostRequest {
                lobby_password: create_lobby_request.lobby_password,
                command: JeopardyCommand::Host {
                    host_password: create_lobby_request.host_password,
                    command: HostCommand::GetBuzzerQueue,
                },
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::NOT_FOUND);
        assert!(matches!(
            response,
            Json(HostResponse {
                result: Err(..),
                ..
            })
        ));
    }

    #[tokio::test]
    async fn GIVEN_invalid_lobby_password_WHEN_handle_host_command_THEN_error() {
        // GIVEN
        // create lobby with player to run commands against
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;

        // WHEN
        let (status_code, response) = super::handle_host_command(
            State(state),
            Path(create_lobby_request.lobby_name),
            Extension(Uuid::new_v4()),
            Json(HostRequest {
                lobby_password: "INCORRECT".to_string(),
                command: JeopardyCommand::Host {
                    host_password: create_lobby_request.host_password,
                    command: HostCommand::GetBuzzerQueue,
                },
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::UNAUTHORIZED);
        assert!(matches!(
            response,
            Json(HostResponse {
                result: Err(..),
                ..
            })
        ));
    }

    #[tokio::test]
    async fn GIVEN_invalid_host_password_WHEN_handle_host_command_THEN_error() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;

        // WHEN
        let (status_code, response) = super::handle_host_command(
            State(state),
            Path(create_lobby_request.lobby_name),
            Extension(Uuid::new_v4()),
            Json(HostRequest {
                lobby_password: create_lobby_request.lobby_password,
                command: JeopardyCommand::Host {
                    host_password: "INCORRECT".to_string(),
                    command: HostCommand::GetBuzzerQueue,
                },
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::UNAUTHORIZED);
        assert!(matches!(
            response,
            Json(HostResponse {
                result: Err(..),
                ..
            })
        ));
    }

    #[tokio::test]
    async fn GIVEN_invalid_command_args_WHEN_handle_host_command_THEN_error() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server(Some(create_lobby_request.clone())).await;

        // WHEN
        let (status_code, response) = super::handle_host_command(
            State(state),
            Path(create_lobby_request.lobby_name),
            Extension(Uuid::new_v4()),
            Json(HostRequest {
                lobby_password: create_lobby_request.lobby_password,
                command: JeopardyCommand::Host {
                    host_password: create_lobby_request.host_password,
                    // JeopardyConfig::test_default() only makes 1 board so index 1000 def fails
                    command: HostCommand::GetAnswer {
                        board_index: 1000,
                        category_index: 0,
                        question_index: 0,
                    },
                },
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::BAD_REQUEST);
        assert!(matches!(
            response,
            Json(HostResponse {
                result: Err(..),
                ..
            })
        ));
    }

    // internal server errors

    #[tokio::test]
    async fn GIVEN_failing_manager_during_lobby_lookup_WHEN_handle_host_command_THEN_error() {
        // GIVEN
        let create_lobby_request = CreateLobbyRequest {
            lobby_name: "lobby_name".to_string(),
            lobby_password: "lobby_password".to_string(),
            host_password: "host_password".to_string(),
            config: JeopardyConfig::test_default(),
        };
        let state = new_test_server_with_test_manager(Some(create_lobby_request.clone())).await;
        state.manager().write().await.set_always_fail(); // ensure lobby lookup fails

        // WHEN
        let (status_code, response) = super::handle_host_command(
            State(state),
            Path(create_lobby_request.lobby_name),
            Extension(Uuid::new_v4()),
            Json(HostRequest {
                lobby_password: create_lobby_request.lobby_password,
                command: JeopardyCommand::Host {
                    host_password: create_lobby_request.host_password,
                    command: HostCommand::GetBuzzerQueue,
                },
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(matches!(
            response,
            Json(HostResponse {
                result: Err(..),
                ..
            })
        ));
    }

    // lobby shutdown

    #[tokio::test]
    async fn GIVEN_shutdown_lobby_WHEN_handle_host_command_THEN_error() {
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
            .write()
            .await
            .get(&create_lobby_request.lobby_name)
            .unwrap()
            .lobby()
            .shutdown()
            .await
            .unwrap(); // shut down lobby so it fails

        // WHEN
        let (status_code, response) = super::handle_host_command(
            State(state),
            Path(create_lobby_request.lobby_name),
            Extension(Uuid::new_v4()),
            Json(HostRequest {
                lobby_password: create_lobby_request.lobby_password,
                command: JeopardyCommand::Host {
                    host_password: create_lobby_request.host_password,
                    command: HostCommand::GetBuzzerQueue,
                },
            }),
        )
        .await;

        // THEN
        assert_eq!(status_code, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(matches!(
            response,
            Json(HostResponse {
                result: Err(..),
                ..
            })
        ));
    }

    // custom serde tests

    #[test]
    fn GIVEN_host_response_ok_result_WHEN_serialize_result_THEN_ok() {
        // GIVEN
        let response = HostResponse {
            request_id: "test_request_id".to_string(), // we dont check this bc it's default serialization
            result: Ok(JeopardyCommandResponse::Host(HostCommandResponse::Success)),
        };

        // WHEN
        let serialized = serde_json::to_value(&response).unwrap();
        let serialized = serialized.as_object().unwrap().get("result").unwrap();
        let error_v = serialized.get(HOST_RESPONSE_RESULT_ERROR_KEY).unwrap();
        let value_v = serialized.get(HOST_RESPONSE_RESULT_VALUE_KEY).unwrap();

        // THEN
        assert!(error_v.is_null());
        assert!(value_v.is_object())
    }

    #[test]
    fn GIVEN_host_response_err_result_WHEN_serialize_result_THEN_ok() {
        // GIVEN
        let error_msg = "error".to_string();
        let response = HostResponse {
            request_id: "test_request_id".to_string(), // we dont check this bc it's default serialization
            result: Err(error_msg.clone()),
        };

        // WHEN
        let serialized = serde_json::to_value(&response).unwrap();
        let serialized = serialized.as_object().unwrap().get("result").unwrap();
        let serialized_error = serialized.get(HOST_RESPONSE_RESULT_ERROR_KEY).unwrap();
        let serialized_value = serialized.get(HOST_RESPONSE_RESULT_VALUE_KEY).unwrap();

        // THEN
        assert_eq!(error_msg, serialized_error.as_str().unwrap());
        assert!(serialized_value.is_null());
    }
}
