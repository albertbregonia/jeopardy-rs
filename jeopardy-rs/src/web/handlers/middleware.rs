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

#[cfg(test)]
#[allow(non_snake_case)]
mod middleware_tests {
    use std::sync::Arc;
    use tower::util::ServiceExt;

    use axum::{Router, body::Body, extract::State, http::Request, middleware, routing::get};
    use tokio::sync::mpsc;
    use uuid::Uuid;

    use crate::web::handlers::middleware::request_id_middleware;

    #[tokio::test]
    async fn GIVEN_middlware_WHEN_request_THEN_ok() {
        // create a channel that we can listen to if the server panics
        let (tx, mut rx) = mpsc::channel(1);

        // sample handler to ensure that our middleware has inserted the request id
        let test_handler = move |State(tx): State<Arc<mpsc::Sender<_>>>, req: Request<Body>| async move {
            let result = tokio::spawn(async move {
                let request_id = req
                    .extensions()
                    .get::<Uuid>();
                    assert!(matches!(
                        request_id, // ensure the request extensions have a uuid / request id
                        Some(Uuid{..})
                    ));
            }).await;
            tx.send(result).await.unwrap();
        };

        let request = Request::builder().uri("/").body(Body::empty()).unwrap();

        // run oneshot web server
        tokio::spawn(
            Router::new()
            .route("/", get(test_handler))
            .layer(middleware::from_fn(request_id_middleware))
            .with_state(Arc::new(tx))
            .oneshot(request)
        );
        // if the server panics, we panic too (test fails)
        // otherwise, test passes and we unwrap a unit
        rx.recv().await.unwrap().unwrap();
    }
}