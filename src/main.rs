use std::{env, sync::Arc};

pub mod web;
use axum::{Json, Router, extract::{State, WebSocketUpgrade, ws::WebSocket}, response::IntoResponse, routing::{get, post}};
use tokio::{net::TcpListener, sync::RwLock};
use tower_http::services::ServeDir;
pub use web::*;

pub mod game;
pub use game::*;

use crate::{routes::{ADMIN_API_PATH, LOGIN_PATH}, web::{json_websocket::JsonWebSocket, lobby::{LobbyManager, LobbyMap}}};

pub struct GlobalState<M: LobbyManager> {
    manager: M
}

// ngl I don't like this...
// in the way that, now, the type has to be specified 
// wherever in use as opposed to being abstracted fully
// ie. if I use something else other than LobbyMap I have to fix the specifier
// nbd... but it is annoying 
// and I can't use a Box<dyn> bc it violates the 'static constraint
impl <M> GlobalState<M> 
where M: LobbyManager {
    pub fn new(manager: M) -> Self {
        Self { manager }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let lobby_map = LobbyMap::new();
    let global_state = Arc::new(RwLock::new(GlobalState::new(lobby_map)));
    let static_dir = env::var(STATIC_DIR_ENV_KEY)
        .unwrap_or(DEFAULT_STATIC_DIR.to_string());

    let app = Router::new()
        .route(LOGIN_PATH, get(websocket_upgrader))
        .route(ADMIN_API_PATH, post(admin_handler))
        .with_state(global_state)
        .fallback_service(ServeDir::new(static_dir));

    let server_port: u16 = env::var(SERVER_PORT_ENV_KEY)
        .map(|port_string| port_string.parse()
        .expect("Unable to convert SERVER_PORT to u16"))
        .unwrap_or(DEFAULT_SERVER_PORT);

    let listener = TcpListener::bind(format!("0.0.0.0:{server_port}"))
        .await
        .expect("Unable to start TcpListener for web server");

    tracing::info!("Server started on http://127.0.0.1:{server_port}");
    axum::serve(listener, app).await
        .expect("Failed to start axum server");
}

pub async fn websocket_upgrader(
    State(global_state): State<Arc<RwLock<GlobalState<LobbyMap>>>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // NOTE: a websocket is needed for players bc a live connection is needed for
    // bidirectional input (buzzer, live game state, etc.)
    ws.on_upgrade(|socket| websocket_handler(socket, global_state))
}

pub async fn websocket_handler(socket: WebSocket, global_state: Arc<RwLock<GlobalState<LobbyMap>>>) {
    // TODO:
    // 1. create websocket
    // 2. receive/validate player and lobby info
    // 3. use connection for lobby/game events

    // let json_ws = JsonWebSocket::new(socket);
}

async fn admin_handler(
    State(global_state): State<Arc<RwLock<GlobalState<LobbyMap>>>>,
    Json(request): Json<HostRequest>,
) -> impl IntoResponse {
    // TODO: handle admin API - host of the game and will control points, questions, etc.
    // 1. prereq: establish the model / object shapes (request format, response format, etc)
    // 2. authN only, no authZ, HTTP only bc requests are infrequent
}
