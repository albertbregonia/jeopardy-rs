use std::{env, sync::Arc};

pub mod web;
use axum::Router;
use tokio::{net::TcpListener, sync::RwLock};
use tower_http::services::ServeDir;
pub use web::*;

pub mod game;
pub use game::*;

use crate::global::GlobalState;

mod global;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let global_state = Arc::new(RwLock::new(GlobalState::new()));
    let static_dir = env::var(STATIC_DIR_ENV_KEY).unwrap_or(DEFAULT_STATIC_DIR.to_string());

    let app = Router::new()
        .merge(routes::player::routes())
        .merge(routes::host::routes())
        .with_state(global_state)
        .fallback_service(ServeDir::new(static_dir));

    let server_port: u16 = env::var(SERVER_PORT_ENV_KEY)
        .map(|port_string| {
            port_string
                .parse()
                .expect("Unable to convert SERVER_PORT to u16")
        })
        .unwrap_or(DEFAULT_SERVER_PORT);

    let listener = TcpListener::bind(format!("0.0.0.0:{server_port}"))
        .await
        .expect("Unable to start TcpListener for web server");

    tracing::info!("Server started on http://127.0.0.1:{server_port}");
    axum::serve(listener, app)
        .await
        .expect("Failed to start axum server");
}
