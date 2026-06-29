use std::{env, sync::Arc};

use axum::{Router, middleware};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

use crate::{
    server::{JeopardyServer, ServerConfig},
    web::{
        DEFAULT_SERVER_PORT, DEFAULT_STATIC_DIR, SERVER_PORT_ENV_KEY, STATIC_DIR_ENV_KEY,
        handlers::middleware::request_id_middleware, routes,
    },
};

mod game;
mod server;
mod web;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let config = ServerConfig::from_env().await.unwrap_or_default();
    let server = JeopardyServer::from_config(config);

    let static_dir = env::var(STATIC_DIR_ENV_KEY).unwrap_or(DEFAULT_STATIC_DIR.to_string());
    let server_port: u16 = env::var(SERVER_PORT_ENV_KEY)
        .map(|port_string| {
            port_string
                .parse()
                .expect("Unable to convert SERVER_PORT to u16")
        })
        .unwrap_or(DEFAULT_SERVER_PORT);

    let app = Router::new()
        .merge(routes::routes())
        .layer(middleware::from_fn(request_id_middleware))
        .with_state(Arc::new(server))
        .fallback_service(ServeDir::new(static_dir));

    let listener = TcpListener::bind(format!("0.0.0.0:{server_port}"))
        .await
        .expect("Unable to start TcpListener for web server");

    tracing::info!("Server started on http://127.0.0.1:{server_port}");
    axum::serve(listener, app)
        .await
        .expect("Failed to start web server");
}
