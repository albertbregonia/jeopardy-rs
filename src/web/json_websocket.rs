use std::{error::Error, marker::PhantomData};

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum JsonWebsocketError {
    #[error("No next message available on WebSocket.")]
    EndOfStream,
    #[error("Encountered an error from underlying websocket: {0}.")]
    Underlying(Box<dyn Error + Send>), // type erasure occurs here but that's ok bc you would handle that internally per impl, not higher up
    #[error("Encountered an underlying JSON serialization/deserialization error: {0}.")]
    Json(#[from] serde_json::Error),
    #[error("Encountered the wrong type of WebSocket message when expecting another.")]
    UnexpectedMsg,
}

pub struct JsonWebSocket<I, O>
where
    I: DeserializeOwned,
    O: Serialize,
{
    _input_type_bound: PhantomData<I>,
    _output_type_bound: PhantomData<O>,
    // lowk doing it this way to force type erasure is annoying and verbose
    // compared to doing this with generics ;_;
    // bc i have to use async_trait, Box<> + Send + 'static, etc.
    // but top level, it should not matter what specific SocketLike impl is being used.
    // functionality is represented by the trait, end of story.
    socket: Box<dyn SocketLike + Send + 'static>,
}

#[async_trait::async_trait]
pub trait SocketLike {
    async fn read_text(&mut self) -> Result<Bytes, JsonWebsocketError>;
    async fn send_text(&mut self, msg: &str) -> Result<(), JsonWebsocketError>;
    async fn disconnect(
        self: Box<Self>,
        close_code: u16,
        msg: Option<&str>,
    ) -> Result<(), JsonWebsocketError>;
}

#[async_trait::async_trait]
impl SocketLike for WebSocket {
    async fn read_text(&mut self) -> Result<Bytes, JsonWebsocketError> {
        let raw_msg = self
            .next()
            .await
            .ok_or(JsonWebsocketError::EndOfStream)?
            .map_err(|e| JsonWebsocketError::Underlying(e.into_inner()))?;
        match raw_msg {
            Message::Text(utf8_bytes) => Ok(utf8_bytes.into()),
            // NOTE: ping/pongs are not implemented bc we are actively streaming data for the game
            _ => Err(JsonWebsocketError::UnexpectedMsg), // encompasses CloseFrame
        }
    }

    async fn send_text(&mut self, msg: &str) -> Result<(), JsonWebsocketError> {
        let msg = Message::Text(msg.into());
        self.send(msg)
            .await
            .map_err(|e| JsonWebsocketError::Underlying(e.into_inner()))?;
        Ok(())
    }

    async fn disconnect(
        mut self: Box<Self>,
        close_code: u16,
        msg: Option<&str>,
    ) -> Result<(), JsonWebsocketError> {
        if let Some(msg) = msg {
            let close_msg = Message::Close(Some(CloseFrame {
                code: close_code,
                reason: msg.into(),
            }));
            self.send(close_msg)
                .await
                .map_err(|e| JsonWebsocketError::Underlying(e.into_inner()))?;
        }
        // spec-wise this should wait for the close frame response but it's fine to close() here
        // this function consumes `self` and drops the connection either way
        self.close()
            .await
            .map_err(|e| JsonWebsocketError::Underlying(e.into_inner()))?;
        Ok(())
    }
}

impl<I, O> JsonWebSocket<I, O>
where
    I: DeserializeOwned,
    O: Serialize,
{
    pub fn new(socket: impl SocketLike + Send + 'static) -> Self {
        Self {
            _input_type_bound: PhantomData,
            _output_type_bound: PhantomData,
            socket: Box::new(socket),
        }
    }

    pub async fn read_json(&mut self) -> Result<I, JsonWebsocketError> {
        let raw_msg = self.socket.read_text().await?;
        let deserialized = serde_json::from_slice(&raw_msg)?;
        Ok(deserialized)
    }

    pub async fn send_json(&mut self, payload: &O) -> Result<(), JsonWebsocketError> {
        let serialized = serde_json::to_string(payload)?;
        self.socket.send_text(&serialized).await
    }

    pub async fn disconnect(
        self,
        close_code: u16,
        msg: Option<&str>,
    ) -> Result<(), JsonWebsocketError> {
        // NOTE: this consumes the object (effectively dropping the underlying socket)
        // this is also just a re-export of SocketLike's disconnect
        self.socket.disconnect(close_code, msg).await
    }
}
