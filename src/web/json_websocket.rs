use std::{error::Error, marker::PhantomData};

use axum::extract::ws::{CloseFrame, Message, WebSocket, close_code};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

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
    async fn read_text(&mut self) -> Result<Bytes, JsonWebsocketError>;
    async fn send_text(&mut self, msg: &str) -> Result<(), JsonWebsocketError>;
    async fn disconnect(
        self,
        user_error: bool,
        msg: Option<&str>,
    ) -> Result<(), JsonWebsocketError>;
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
        let deserialized = serde_json::from_slice(&raw_msg).map_err(|e| InternalError::Json(e))?;
        Ok(deserialized)
    }

    pub async fn send_json(&mut self, payload: &O) -> Result<(), JsonWebsocketError> {
        let serialized = serde_json::to_string(payload).map_err(|e| InternalError::Json(e))?;
        self.socket.send_text(&serialized).await
    }

    pub async fn disconnect(
        self,
        user_error: bool,
        msg: Option<&str>,
    ) -> Result<(), JsonWebsocketError> {
        // NOTE: this consumes the object (effectively dropping the underlying socket)
        // this is also just a re-export of disconnect bc Box<> might mess with visibility
        self.socket.disconnect(user_error, msg).await
    }
}

#[cfg(test)]
mod tests {}
