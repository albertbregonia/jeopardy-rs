pub mod requests;
pub use requests::*;

pub mod routes;
pub mod json_websocket;
pub mod lobby;

pub const SERVER_PORT_ENV_KEY: &'static str = "PORT";
pub const DEFAULT_SERVER_PORT: u16 = 8080;

pub const STATIC_DIR_ENV_KEY: &'static str = "STATIC_DIR";
pub const DEFAULT_STATIC_DIR: &'static str = "./src/static";

pub const HOST_PASSWORD_ENV_KEY: &'static str = "HOST_PASSWORD";
pub const DEFAULT_HOST_PASSWORD: &'static str = "CHANGE_me123";