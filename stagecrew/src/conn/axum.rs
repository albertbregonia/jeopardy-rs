use axum::extract::ws::{CloseFrame, Message, WebSocket, close_code};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use thiserror::Error;

use crate::conn::{ErrorReason, TextTransport};

#[derive(Debug, Error)]
pub enum AxumTextTransportError {
    #[error(transparent)]
    Axum(#[from] axum::Error),
    #[error("Received non-text message when expecting text on WebSocket: {0:?}")]
    UnexpectedMessageReceived(Message),
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
            Message::Close(_) => None, // axum internally echoes Message::Close, we simply want to propagate the None to JsonConn
            other => Some(Err(AxumTextTransportError::UnexpectedMessageReceived(
                other,
            ))),
        }
    }

    async fn send_text(&mut self, msg: &str) -> Result<(), Self::Error> {
        let msg = Message::Text(msg.into());
        self.send(msg).await.map_err(AxumTextTransportError::Axum)?;
        Ok(())
    }

    async fn disconnect(mut self, reason: Option<ErrorReason>) -> Result<(), Self::Error> {
        if let Some(ErrorReason {
            internal_error,
            reason,
        }) = reason
        {
            // axum doesn't provide this granular control over the close frame
            // so we re-implement it here
            let code = if internal_error {
                close_code::ERROR
            } else {
                close_code::INVALID
            };
            let close_msg = Message::Close(Some(CloseFrame {
                code,
                reason: reason.into(),
            }));
            self.send(close_msg)
                .await
                .map_err(AxumTextTransportError::Axum)?;
            // then drop to disconnect
        } else {
            self.close().await.map_err(AxumTextTransportError::Axum)?;
        }
        // we don't wait for a close frame here bc that may induce a soft lock
        // which requires use to then think about timeouts, etc.
        Ok(())
    }
}

// NOTE: axum defines the canonical way to "unit test" is simply to use the futures util split()
// as using the trait will allow us to use something like mpsc::channel() in place of an actual websocket.
// i can't do this in my case so we'll rely on an integ test instead
