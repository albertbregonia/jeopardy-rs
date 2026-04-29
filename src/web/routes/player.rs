use axum::{
    Router,
    routing::{get, post},
};

use crate::{
    global::JeopardyGlobalState,
    handlers::player::{create_lobby, websocket_upgrader},
    routes::{CREATE_LOBBY_PATH, LOGIN_PATH},
};

pub fn routes() -> Router<JeopardyGlobalState> {
    Router::new()
        .route(LOGIN_PATH, get(websocket_upgrader))
        .route(CREATE_LOBBY_PATH, post(create_lobby))
}
