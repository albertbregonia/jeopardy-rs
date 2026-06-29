use axum::{Extension, Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Deserialize;
use uuid::Uuid;

use crate::server::{CredsValidatorGeneric, GenericJeopardyServerState, ManagerGeneric};

#[derive(Debug, Deserialize, Clone)]
pub struct CreateLobbyRequest {}

pub async fn create_lobby<M: ManagerGeneric, C: CredsValidatorGeneric>(
    State(state): State<GenericJeopardyServerState<M, C>>,
    Extension(request_id): Extension<Uuid>,
    Json(request): Json<CreateLobbyRequest>,
) -> impl IntoResponse {
    (StatusCode::SERVICE_UNAVAILABLE, "under construction!")
}
