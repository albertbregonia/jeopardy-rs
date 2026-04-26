use axum::{Router, routing::get};

use crate::{
    global::JeopardyGlobalState, handlers::player::websocket_upgrader, routes::LOGIN_PATH,
};

pub fn routes() -> Router<JeopardyGlobalState> {
    Router::new().route(LOGIN_PATH, get(websocket_upgrader))
}
