use axum::{Router, routing::post};

use crate::{global::JeopardyGlobalState, handlers::host::admin_handler, routes::ADMIN_API_PATH};

pub fn routes() -> Router<JeopardyGlobalState> {
    Router::new().route(ADMIN_API_PATH, post(admin_handler))
}
