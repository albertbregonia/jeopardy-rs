use axum::{Json, extract::State, response::IntoResponse};

use crate::{HostRequest, global::JeopardyGlobalState};

pub async fn admin_handler(
    State(_global_state): State<JeopardyGlobalState>,
    Json(_request): Json<HostRequest>,
) -> impl IntoResponse {
    // TODO: handle admin API - host of the game and will control points, questions, etc.
    // 1. prereq: establish the model / object shapes (request format, response format, etc)
    // 2. authN only, no authZ, HTTP only bc requests are infrequent
}
