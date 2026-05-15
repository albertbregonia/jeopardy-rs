use std::{error::Error, marker::PhantomData};

use axum::extract::ws::{CloseFrame, Message, WebSocket, close_code};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;
#[cfg(test)]
use tokio::sync::mpsc::Sender;

#[derive(Debug, Error)]
pub enum JsonWebsocketError {
    #[error("{0}")]
    User(#[from] UserError),
    #[error("{0}")]
    Internal(#[from] InternalError),
}

#[derive(Debug, Error)]
pub enum UserError {
    #[error("Encountered the wrong type of WebSocket message when expecting another.")]
    UnexpectedMsg,
}

#[derive(Debug, Error)]
pub enum InternalError {
    #[error("No next message available on WebSocket.")]
    EndOfStream,
    #[error("Encountered an error from underlying websocket: {0}.")]
    Underlying(Box<dyn Error + Send>), // type erasure occurs here but that's ok bc you would handle that internally per impl, not higher up
    #[error("Encountered an underlying JSON serialization/deserialization error: {0}.")]
    Json(#[from] serde_json::Error),
}

pub struct JsonWebSocket<W, I, O>
where
    W: TextTransport,
    I: DeserializeOwned,
    O: Serialize,
{
    _input_type_bound: PhantomData<I>,
    _output_type_bound: PhantomData<O>,
    socket: W,
}

pub trait TextTransport {
    fn read_text(&mut self) -> impl Future<Output = Result<Bytes, JsonWebsocketError>>;
    fn send_text(&mut self, msg: &str) -> impl Future<Output = Result<(), JsonWebsocketError>>;
    fn disconnect(
        self,
        user_error: bool,
        msg: Option<&str>,
    ) -> impl Future<Output = Result<(), JsonWebsocketError>>;
}

impl TextTransport for WebSocket {
    async fn read_text(&mut self) -> Result<Bytes, JsonWebsocketError> {
        let raw_msg = self
            .next()
            .await
            .ok_or(InternalError::EndOfStream)?
            .map_err(|e| InternalError::Underlying(e.into_inner()))?;
        match raw_msg {
            Message::Text(utf8_bytes) => Ok(utf8_bytes.into()),
            // NOTE: ping/pongs are not implemented bc we are actively streaming data for the game
            _ => Err(JsonWebsocketError::User(UserError::UnexpectedMsg)), // encompasses CloseFrame
        }
    }

    async fn send_text(&mut self, msg: &str) -> Result<(), JsonWebsocketError> {
        let msg = Message::Text(msg.into());
        self.send(msg)
            .await
            .map_err(|e| InternalError::Underlying(e.into_inner()))?;
        Ok(())
    }

    async fn disconnect(
        mut self,
        user_error: bool,
        msg: Option<&str>,
    ) -> Result<(), JsonWebsocketError> {
        if let Some(msg) = msg {
            let close_msg = Message::Close(Some(CloseFrame {
                code: if user_error {
                    close_code::INVALID
                } else {
                    close_code::ERROR
                },
                reason: msg.into(),
            }));
            self.send(close_msg)
                .await
                .map_err(|e| InternalError::Underlying(e.into_inner()))?;
        }
        // spec-wise this should wait for the close frame response but it's fine to close() here
        // this function consumes `self` and drops the connection either way
        self.close()
            .await
            .map_err(|e| InternalError::Underlying(e.into_inner()))?;
        Ok(())
    }
}

impl<W, I, O> JsonWebSocket<W, I, O>
where
    W: TextTransport,
    I: DeserializeOwned,
    O: Serialize,
{
    pub fn new(socket: W) -> Self {
        Self {
            _input_type_bound: PhantomData,
            _output_type_bound: PhantomData,
            socket,
        }
    }

    pub async fn read_json(&mut self) -> Result<I, JsonWebsocketError> {
        let raw_msg = self.socket.read_text().await?;
        let deserialized = serde_json::from_slice(&raw_msg).map_err(InternalError::Json)?;
        Ok(deserialized)
    }

    pub async fn send_json(&mut self, payload: &O) -> Result<(), JsonWebsocketError> {
        let serialized = serde_json::to_string(payload).map_err(InternalError::Json)?;
        self.socket.send_text(&serialized).await
    }

    pub async fn disconnect(
        self,
        user_error: bool,
        msg: Option<&str>,
    ) -> Result<(), JsonWebsocketError> {
        // NOTE: this consumes the object (effectively dropping the underlying socket)
        // this is also just a re-export of TextTransport disconnect
        self.socket.disconnect(user_error, msg).await
    }
}

// publicly accessible mock for use in unit tests
// TODO: we might want to change mock_socket.msg to be another channel in the future
// that way we can really mock a socket which various requests
#[cfg(test)]
pub struct MockSocket<T: Serialize> {
    pub msg: T,
    pub sender: Sender<String>,
}

#[cfg(test)]
impl<T> TextTransport for MockSocket<T>
where
    T: Serialize,
{
    async fn read_text(&mut self) -> Result<Bytes, JsonWebsocketError> {
        let serialized = serde_json::to_vec(&self.msg).map_err(|e| InternalError::Json(e))?;
        Ok(Bytes::from(serialized))
    }

    async fn send_text(&mut self, msg: &str) -> Result<(), JsonWebsocketError> {
        self.sender
            .send(msg.to_string())
            .await
            .map_err(|e| InternalError::Underlying(Box::new(e)))?;
        Ok(())
    }

    async fn disconnect(
        self,
        _user_error: bool,
        _msg: Option<&str>,
    ) -> Result<(), JsonWebsocketError> {
        Ok(())
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {

    // TODO: unit tests for the axum implementation using mockall

    use tokio::sync::mpsc;

    use crate::{
        PlayerRequest,
        json_websocket::{JsonWebSocket, MockSocket},
    };

    #[tokio::test]
    async fn GIVEN_json_string_WHEN_read_json_THEN_ok() {
        // GIVEN
        let request = PlayerRequest::Login {
            username: "test_username".to_string(),
            lobby_name: "test_lobby_name".to_string(),
            password: "test_password".to_string(),
        };
        let (sender, _r) = mpsc::channel(1);
        let mut mock_ws = JsonWebSocket::<_, _, String>::new(MockSocket {
            msg: request,
            sender,
        });
        // WHEN
        let deserialized = mock_ws.read_json().await.unwrap();
        // THEN
        assert!(matches!(deserialized, PlayerRequest::Login { .. }));
    }
}
