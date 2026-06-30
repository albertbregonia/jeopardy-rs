use axum::{
    Router,
    routing::{delete, patch, post},
};

use crate::{
    server::JeopardyServerState,
    web::handlers::{create_lobby, delete_lobby, handle_host_command},
};

pub const LOBBY_PATH: &str = "/lobbies"; // with POST, GET with websocket to join
pub const LOBBY_DELETE_PATH: &str = "/lobbies/{lobby_id}"; //  DELETE to delete lobby
pub const HOST_API_PATH: &str = "/lobbies/{lobby_id}/admin"; // PATCH - to update game state

pub fn routes() -> Router<JeopardyServerState> {
    Router::new()
        .route(LOBBY_PATH, post(create_lobby))
        .route(LOBBY_DELETE_PATH, delete(delete_lobby))
        .route(HOST_API_PATH, patch(handle_host_command))
}
