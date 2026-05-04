pub mod requests;
pub use requests::*;

pub mod game;
pub mod handlers;
pub mod json_websocket;
pub mod routes;

pub const SERVER_PORT_ENV_KEY: &str = "PORT";
pub const DEFAULT_SERVER_PORT: u16 = 8080;

pub const STATIC_DIR_ENV_KEY: &str = "STATIC_DIR";
pub const DEFAULT_STATIC_DIR: &str = "./src/static";
