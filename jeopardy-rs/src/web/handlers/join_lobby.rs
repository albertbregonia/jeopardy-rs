use axum::{
    Extension,
    extract::{State, WebSocketUpgrade},
    response::IntoResponse,
};
use stagecrew::conn::{JsonConn, TextTransport};
use tracing::Instrument;
use uuid::Uuid;

use crate::{
    server::{CredsValidatorGeneric, JeopardyServerStateGeneric, ManagerGeneric},
    web::handlers::player::{PlayerRequest, PlayerResponse},
};

/// Top level handler for websocket creation.
/// To play a Jeopardy game, players need to connect with a websocket and join a lobby.
/// After upgrading the HTTP request to a websocket, the first message is expected
/// to be the login credentials for a lobby. From then on, the connection is managed by
/// a helper wrapper type `PlayerConn` and `handle_websocket(..)` manages it's lifetime.
pub async fn join_lobby<M: ManagerGeneric, C: CredsValidatorGeneric>(
    State(state): State<JeopardyServerStateGeneric<M, C>>,
    Extension(request_id): Extension<Uuid>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let span = tracing::Span::current();
    ws.on_failed_upgrade(move |e| {
        let _ = span.enter(); // default span is fine here
        tracing::error!("Failed to upgrade websocket: {e}");
    })
    .on_upgrade(move |socket| async move {
        // allow websocket to inherit request id from middleware
        // the default span is not important at this point
        let span = tracing::info_span!(
            "dataplane",
            request_id=%request_id,
        );
        handle_websocket(state, JsonConn::new(socket))
            .instrument(span)
            .await;
    })
}

// axum does not expose a mock websocket. therefore,
// the canonical way is to test a helper instead of the upgrader with a real websocket
// therefore, we use a generic JsonConn to allow for easy unit testing
pub async fn handle_websocket<M, C, T>(
    _state: JeopardyServerStateGeneric<M, C>,
    _json_ws: JsonConn<T, PlayerRequest, PlayerResponse>,
) where
    M: ManagerGeneric,
    C: CredsValidatorGeneric,
    T: TextTransport,
{
    tracing::info!("New websocket connection created");
}
