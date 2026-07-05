#![allow(non_snake_case)]
// integ tests in rust cannot mirror the src directory directly
// therefore, this is simply at the root

use std::sync::Arc;

use axum::{
    Router,
    extract::{State, WebSocketUpgrade, ws::WebSocket},
    response::Response,
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use stagecrew::conn::{ErrorReason, JsonConn, JsonConnError};
use tokio::{net::TcpStream, sync::mpsc, task::JoinError};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream,
    tungstenite::{
        Message, Utf8Bytes,
        protocol::{CloseFrame, frame::coding::CloseCode},
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestType;

async fn ws_handler<F, Fut>(
    State(state): State<Arc<(F, mpsc::Sender<Result<(), JoinError>>)>>,
    ws: WebSocketUpgrade,
) -> Response
where
    // this was so complicated bc of async trait guarantees
    // but tl;dr this means "give me a function that tests a JsonConn"
    // i could make this a trait but then re-implementing the signature every time is complicated
    //
    // we can leverage the rust compiler's inference and
    // simply write this twice in the web server creator and then websocket handler
    F: Fn(JsonConn<WebSocket, TestType, TestType>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    ws.on_upgrade(|ws| async move {
        let (tests, reply) = state.as_ref();
        let json_ws = JsonConn::<WebSocket, TestType, TestType>::new(ws);
        // as panicking in this thread due to failed assert!(..) does not result in a panic for the test
        // we must create a new task as a test harness that returns the panic somehow
        // therefore, we use an mpsc::channel to send back the result if it failed
        let result = tokio::spawn(tests(json_ws)).await;
        reply.send(result).await.unwrap();
    })
}

/// given a Fn that represents the tests to run on the server-side when a JsonConn is created,
/// create a single-use websocket server along with a client and an mpsc::Receiver<>
///
/// for each test, the client websocket sends messages to the server,
/// the server constructs the JsonConn and passes it to the test Fn.
/// any panic!()s (eg. by assert!(..) or unwrap(..), etc.)
/// get returned to the tokio::test over the mpsc::Receiver<> so that the test can fail
///
/// tl;dr this integ test harness is a lot of passing between threads
/// so that the tests can act as both the server and the client to test for errors
async fn setup_websocket_client_and_server<F, Fut>(
    tests: F,
) -> (
    WebSocketStream<MaybeTlsStream<TcpStream>>,
    mpsc::Receiver<Result<(), JoinError>>,
)
where
    F: Fn(JsonConn<WebSocket, TestType, TestType>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    // bind random port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    // start server
    let (tx, rx) = mpsc::channel(1); // create channel to send web server panics back to tests
    let router = Router::new()
        .route("/", get(ws_handler))
        .with_state(Arc::new((tests, tx)));
    tokio::spawn(async move { axum::serve(listener, router).await });
    // connect to server
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}"))
        .await
        .unwrap();
    (ws, rx)
}

// NOTE: we are testing the underlying TextTransport implementation for axum WebSocket
// JSON errors and such will not be tested

#[tokio::test]
async fn GIVEN_test_type_WHEN_read_json_THEN_ok() {
    // GIVEN
    let test_payload = TestType;
    let fn_test_payload = test_payload.clone(); // move to setup function
    let (mut ws, mut rx) = setup_websocket_client_and_server(move |mut json_conn| {
        let test_payload = fn_test_payload.clone(); // each future needs a clone
        async move {
            // THEN
            let payload = json_conn.read_json().await.unwrap().unwrap();
            assert_eq!(payload, test_payload);
        }
    })
    .await;

    // WHEN
    let serialized = serde_json::to_string(&test_payload).unwrap();
    ws.send(Message::text(serialized)).await.unwrap();

    // THEN - panic if the web server panics
    rx.recv()
        .await
        .expect("web server sender closed")
        .expect("web server panicked");
}

#[tokio::test]
async fn GIVEN_close_WHEN_read_json_THEN_ok() {
    // GIVEN
    let (mut ws, mut rx) = setup_websocket_client_and_server(|mut json_conn| async move {
        // THEN
        // read_json() should receive a close message, close, and return None
        assert!(json_conn.read_json().await.is_none());

        // all successive reads should return None (according to axum ws docs)
        assert!(json_conn.read_json().await.is_none());
    })
    .await;

    // WHEN - clean disconnect with close message
    let close_frame = CloseFrame {
        code: CloseCode::Normal,
        reason: Utf8Bytes::from_static("test induced"),
    };
    ws.close(Some(close_frame.clone())).await.unwrap();

    // THEN - panic if the web server panics
    rx.recv()
        .await
        .expect("web server sender closed")
        .expect("web server panicked");

    // https://docs.rs/axum/latest/axum/extract/ws/enum.Message.html#variant.Close
    let close_msg = ws.next().await.unwrap().unwrap();
    assert!(matches!(
        close_msg,
        Message::Close(Some(CloseFrame { code, reason })) // we should get our close frame echoed by axum
            if code == close_frame.code && reason == close_frame.reason
    ));
    // further attempts to read should return None (according to axum docs)
    assert!(ws.next().await.is_none());
}

#[tokio::test]
async fn GIVEN_dropped_client_WHEN_read_json_THEN_error() {
    // GIVEN
    let (ws, mut rx) = setup_websocket_client_and_server(|mut json_conn| async move {
        // THEN
        assert!(matches!(
            json_conn.read_json().await, // this should axum error bc the connection was dropped abruptly
            Some(Err(JsonConnError::Dependency(..)))
        ));
    })
    .await;

    // WHEN
    drop(ws); // disconnect without close message

    // THEN - panic if the web server panics
    rx.recv()
        .await
        .expect("web server sender closed")
        .expect("web server panicked");
}

#[tokio::test]
async fn GIVEN_ping_msg_WHEN_read_json_THEN_error() {
    // GIVEN
    let (mut ws, mut rx) = setup_websocket_client_and_server(|mut json_conn| async move {
        // THEN
        assert!(matches!(
            json_conn.read_json().await, // ping is unsupported
            Some(Err(JsonConnError::Dependency(..)))
        ));
    })
    .await;

    // WHEN
    ws.send(Message::Ping("".into())).await.unwrap();

    // THEN - panic if the web server panics
    rx.recv()
        .await
        .expect("web server sender closed")
        .expect("web server panicked");
}

#[tokio::test]
async fn GIVEN_test_type_WHEN_send_json_THEN_ok() {
    // GIVEN
    let test_payload = TestType;
    let fn_test_payload = test_payload.clone(); // move to setup function
    let (mut ws, mut rx) = setup_websocket_client_and_server(move |mut json_conn| {
        let payload = fn_test_payload.clone(); // each future needs a clone
        async move {
            // WHEN
            json_conn.send_json(&payload).await.unwrap();
        }
    })
    .await;

    // THEN - panic if the web server panics
    rx.recv()
        .await
        .expect("web server sender closed")
        .expect("web server panicked");

    // ensure payload matches
    let Message::Text(raw_msg) = ws.next().await.unwrap().unwrap() else {
        panic!("Non-text message received during send_json() test");
    };
    let deserialized = serde_json::from_str::<TestType>(&raw_msg).unwrap();
    assert_eq!(deserialized, test_payload);
}

#[tokio::test]
async fn GIVEN_closed_client_WHEN_send_json_THEN_error() {
    // GIVEN
    let (ws, mut rx) = setup_websocket_client_and_server(|mut json_conn| async move {
        // THEN
        let _ = json_conn.read_json().await; // dummy read call to ensure that send doesn't occur before we drop
        assert!(matches!(
            json_conn.send_json(&TestType).await,
            Err(JsonConnError::Dependency(..))
        ));
    })
    .await;

    // WHEN
    drop(ws);

    // THEN - panic if the web server panics
    rx.recv()
        .await
        .expect("web server sender closed")
        .expect("web server panicked");
}

#[tokio::test]
async fn GIVEN_internal_error_reason_WHEN_disconnect_THEN_ok() {
    // GIVEN
    let expected_reason = "test";
    let reason = ErrorReason {
        internal_error: true,
        reason: expected_reason.to_string(),
    };
    let (mut ws, mut rx) = setup_websocket_client_and_server(move |json_conn| {
        let reason = reason.clone(); // needed bc we need to use an Fn with State<>
        async move {
            // WHEN
            json_conn.disconnect(Some(reason)).await.unwrap();
        }
    })
    .await;

    // THEN - panic if the web server panics
    rx.recv()
        .await
        .expect("web server sender closed")
        .expect("web server panicked");

    // ensure close msg matches
    let close_msg = ws.next().await.unwrap().unwrap();
    assert!(matches!(
        close_msg,
        Message::Close(Some(CloseFrame { code, reason }))
            if code == CloseCode::Error && reason == expected_reason
    ));
}

#[tokio::test]
async fn GIVEN_user_error_reason_WHEN_disconnect_THEN_ok() {
    // GIVEN
    let expected_reason = "test";
    let reason = ErrorReason {
        internal_error: false, // user error
        reason: expected_reason.to_string(),
    };
    let (mut ws, mut rx) = setup_websocket_client_and_server(move |json_conn| {
        let reason = reason.clone(); // needed bc we need to use an Fn with State<>
        async move {
            // WHEN
            json_conn.disconnect(Some(reason)).await.unwrap();
        }
    })
    .await;

    // THEN - panic if the web server panics
    rx.recv()
        .await
        .expect("web server sender closed")
        .expect("web server panicked");

    // ensure close msg matches
    let close_msg = ws.next().await.unwrap().unwrap();
    assert!(matches!(
        close_msg,
        Message::Close(Some(CloseFrame { code, reason }))
            if code == CloseCode::Invalid && reason == expected_reason
    ));
}

#[tokio::test]
async fn GIVEN_no_reason_WHEN_disconnect_THEN_ok() {
    // GIVEN
    let (mut ws, mut rx) = setup_websocket_client_and_server(move |json_conn| async move {
        // WHEN
        json_conn.disconnect(None).await.unwrap();
    })
    .await;

    // THEN - panic if the web server panics
    rx.recv()
        .await
        .expect("web server sender closed")
        .expect("web server panicked");

    let close_msg = ws.next().await.unwrap().unwrap();
    assert!(matches!(
        close_msg,
        Message::Close(..) // ensure we simply get a close msg, default axum functionality
    ));
}
