use axum::{Router, routing::post};

use crate::{server::JeopardyServerState, web::handlers::create_lobby};

pub const LOBBY_CREATE_PATH: &str = "/lobbies"; // with POST
pub const LOBBY_LIFECYLCE_PATH: &str = "/lobbies/{lobby_id}"; //  DELETE to delete lobby, GET with websocket to join
pub const HOST_API_PATH: &str = "/lobbies/{lobby_id}/admin"; // PATCH - to update game state

pub fn routes() -> Router<JeopardyServerState> {
    Router::new().route(LOBBY_CREATE_PATH, post(create_lobby))
}
