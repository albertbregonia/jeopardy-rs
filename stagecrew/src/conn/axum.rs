use axum::extract::ws::{CloseFrame, Message, WebSocket, close_code};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use thiserror::Error;

use crate::conn::TextTransport;

#[derive(Debug, Error)]
pub enum AxumTextTransportError {
    #[error(transparent)]
    Axum(#[from] axum::Error),
    #[error("Received non-text message when expecting text on WebSocket: {0:?}")]
    NonTextMessageReceived(Message),
}

impl TextTransport for WebSocket {
    type Error = AxumTextTransportError;

    /// written to look like axum websocket signature
    /// StreamExt / next() end of websocket returned None => None
    /// Close Message => None
    /// if we get Non-Text or any other error, we return Some(Err)
    async fn read_text(&mut self) -> Option<Result<Bytes, Self::Error>> {
        let result = self.next().await?;
        let raw_msg = match result {
            Err(e) => return Some(Err(e.into())),
            Ok(v) => v,
        };
        match raw_msg {
            Message::Text(utf8_bytes) => Some(Ok(utf8_bytes.into())),
            Message::Close(_) => None, // if we get a close message, clean disconnect
            other => Some(Err(AxumTextTransportError::NonTextMessageReceived(other))),
        }
    }

    async fn send_text(&mut self, msg: &str) -> Result<(), Self::Error> {
        let msg = Message::Text(msg.into());
        self.send(msg).await.map_err(AxumTextTransportError::Axum)?;
        Ok(())
    }

    async fn disconnect(
        mut self,
        internal_error: bool,
        msg: Option<&str>,
    ) -> Result<(), Self::Error> {
        if let Some(msg) = msg {
            let code = if internal_error {
                close_code::ERROR
            } else {
                close_code::INVALID
            };
            let close_msg = Message::Close(Some(CloseFrame {
                code,
                reason: msg.into(),
            }));
            self.send(close_msg)
                .await
                .map_err(AxumTextTransportError::Axum)?;
        }
        // spec-wise this should wait for the close frame response but it's fine to close() here
        // this function consumes `self` and drops the connection either way
        self.close().await.map_err(AxumTextTransportError::Axum)?;
        Ok(())
    }
}

#[cfg(test)]
mod axum_tests {}
