use std::marker::PhantomData;

use axum::extract::ws::{CloseFrame, Message, WebSocket, close_code};
use futures_util::{SinkExt, StreamExt};
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum JsonWebsocketError {
    #[error("Encountered 'None' during attempt to read next message on WebSocket")]
    EndOfStream,
    #[error("Encountered error from underlying websocket: {0}")]
    UnderlyingWebSocketFailure(#[from] axum::Error),
    #[error("Encountered underlying JSON serialization/deserialization error: {0}")]
    JsonFailure(#[from] serde_json::Error)
}

pub struct JsonWebSocket<I, O> {
    _input_type_bound: PhantomData<I>,
    _output_type_bound: PhantomData<O>,
    // NOTE: this is currently tied to the axum::WebSocket
    // it should probably be a trait to decouple.
    socket: WebSocket
}

impl <I, O> JsonWebSocket<I, O>
where 
    I: Serialize + DeserializeOwned,
    O: Serialize + DeserializeOwned {
    
    pub fn new(socket: WebSocket) -> Self {
        Self {
            _input_type_bound: PhantomData,
            _output_type_bound: PhantomData,
            socket
        }
    }

    pub async fn read_msg(&mut self) -> Result<I, JsonWebsocketError> {
        let raw_msg = self.socket
            .next().await
            .ok_or(JsonWebsocketError::EndOfStream)??
            .into_data();
        let deserialized = serde_json::from_slice(&raw_msg)?;
        Ok(deserialized)
    }

    pub async fn write_msg(&mut self, payload: &O) -> Result<(), JsonWebsocketError> {
        let serialized = serde_json::to_string(payload)?;
        let msg = Message::Text(serialized.into());
        self.socket.send(msg).await?;
        Ok(())
    }

    pub async fn handle_error(mut self, error_msg: &str) -> Result<(), JsonWebsocketError> {
        // whenever an error is encountered, use this function to
        // simply close the websocket with the original error msg
        tracing::error!(error_msg);
        let close_msg = Message::Close(Some(CloseFrame {
            code: close_code::ERROR, 
            reason: error_msg.into()
        }));
        self.socket.send(close_msg).await.inspect_err(|e|
            tracing::error!("Failed to send close message with error. Ignoring: {e}")
        )?;
        // spec-wise this should wait for the close frame response but it's fine to close() here
        // this function consumes `self` and drops the connection either way
        self.socket.close().await?;
        Ok(())
    }
}