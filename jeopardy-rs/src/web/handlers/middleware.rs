use axum::{body::Body, extract::Request};
use tracing::Instrument;
use uuid::Uuid;

// simple middleware to log details about the request
// and create a request_id for better tracing
pub async fn request_id_middleware(
    mut req: Request<Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let request_id = Uuid::new_v4();
    let span = tracing::info_span!(
        "controlplane",
        %request_id,
        method = %req.method(),
        path = %req.uri().path(),
    );
    // needed for websocket
    // websocket holds request id from upgrade for better tracing
    req.extensions_mut().insert(request_id);
    next.run(req).instrument(span).await
}
