use std::{error::Error, marker::PhantomData};

use bytes::Bytes;
use serde::{Serialize, de::DeserializeOwned};
use thiserror::Error;

#[cfg(feature = "axum")]
pub mod axum;

#[derive(Debug, Error)]
pub enum JsonConnError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),

    // we do type erasure here bc this is the error boundary
    // we don't care specifically how the underlying dependency broke
    // we just care that it broke and cannot fulfill our operation
    #[error(transparent)]
    Dependency(#[from] Box<dyn Error + Send + Sync + 'static>),
}

pub struct JsonConn<T, I, O>
where
    T: TextTransport,
    I: DeserializeOwned,
    O: Serialize,
{
    _input_type_bound: PhantomData<I>,
    _output_type_bound: PhantomData<O>,
    transport: T,
}

#[derive(Debug, Clone)]
pub struct ErrorReason {
    pub internal_error: bool,
    pub reason: String,
}

// we use `impl Future<>` instead of `async fn` as per compiler suggestion / backwards compatibility
pub trait TextTransport {
    type Error: Error + Send + Sync + 'static;
    fn read_text(&mut self) -> impl Future<Output = Option<Result<Bytes, Self::Error>>>;
    fn send_text(&mut self, msg: &str) -> impl Future<Output = Result<(), Self::Error>>;
    fn disconnect(
        self,
        reason: Option<ErrorReason>,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}

// JsonConn is just a thin wrapper on the TextTransport trait
// mainly to decouple from websocket specifically
// this can be implemented with straight sockets / webrtc channels / etc.
impl<T, I, O> JsonConn<T, I, O>
where
    T: TextTransport,
    I: DeserializeOwned,
    O: Serialize,
{
    pub fn new(transport: T) -> Self {
        Self {
            _input_type_bound: PhantomData,
            _output_type_bound: PhantomData,
            transport,
        }
    }

    pub async fn read_json(&mut self) -> Option<Result<I, JsonConnError>> {
        let raw_msg = match self.transport.read_text().await? {
            Ok(bin) => bin,
            Err(e) => return Some(Err(JsonConnError::Dependency(e.into()))),
        };
        let deserialized = match serde_json::from_slice(&raw_msg) {
            Ok(bin) => bin,
            Err(e) => return Some(Err(e.into())),
        };
        Some(Ok(deserialized))
    }

    pub async fn send_json(&mut self, payload: &O) -> Result<(), JsonConnError> {
        let serialized = serde_json::to_string(payload)?;
        self.transport
            .send_text(&serialized)
            .await
            .map_err(|e| JsonConnError::Dependency(e.into()))
    }

    pub async fn disconnect(self, reason: Option<ErrorReason>) -> Result<(), JsonConnError> {
        // NOTE: this consumes the object (effectively dropping the underlying socket)
        // this is also just a re-export of TextTransport disconnect
        self.transport
            .disconnect(reason)
            .await
            .map_err(|e| JsonConnError::Dependency(e.into()))?;
        Ok(())
    }
}

// publicly accessible mock for use in unit tests
// receiver: allows us to mock JSON requests coming in
// sender: allows us to see what gets sent to the client
#[cfg(feature = "test-util")]
pub mod json_conn_test_constructs {
    use super::*;
    use tokio::sync::mpsc::{self, Receiver, Sender};

    pub struct MockTextTransport<I: Serialize> {
        pub fail_during_read_text: bool, // bool to manually induce a failure during read_text()
        pub input_receiver: Receiver<I>,
        pub output_sender: Sender<String>,
    }

    #[derive(Debug, Error)]
    pub enum MockError {
        #[error(transparent)]
        Derivative(#[from] Box<dyn Error + Send + Sync + 'static>),
        #[error("MockError Generic error")]
        Generic,
    }

    impl<I: Serialize> TextTransport for MockTextTransport<I> {
        type Error = MockError;
        /// given a mpsc::channel bound to a serializable type,
        /// JSON serializes the message and returns the bytes
        /// to be read by `JsonConn` and deserialized
        async fn read_text(&mut self) -> Option<Result<Bytes, Self::Error>> {
            if self.fail_during_read_text {
                return Some(Err(MockError::Generic));
            }
            let msg = self.input_receiver.recv().await?;
            let serialized = serde_json::to_vec(&msg).unwrap();
            Some(Ok(Bytes::from(serialized)))
        }

        async fn send_text(&mut self, msg: &str) -> Result<(), Self::Error> {
            self.output_sender
                .send(msg.to_string())
                .await
                .map_err(|e| MockError::Derivative(e.into()))?;
            Ok(())
        }

        async fn disconnect(self, _reason: Option<ErrorReason>) -> Result<(), Self::Error> {
            // receiver and sender will be dropped automatically
            if self.output_sender.is_closed() {
                return Err(MockError::Generic);
            };
            Ok(())
        }
    }

    /// Create a `JsonConn` using `MockTextTransport` that uses a `mpsc::channel` in place of a websocket.
    /// The input is serialized by `TextTransport::read_text()` then deserialized by `JsonConn::read_json()`
    /// effectively simulating the other end of a websocket sending a JSON payload.
    ///
    /// The output is serialized by `JsonConn::send_json()` then sent over the `mpsc::channel` using `TextTransport::send_text()`.
    /// The receiving end is still text to readily compare received output with expected serialized output
    /// (some types we may only expect serialization but not deserialization)
    pub fn new_test_json_conn<I: Serialize + DeserializeOwned, O: Serialize>(
        fail_during_read_text: bool,
        buffer_size: usize,
    ) -> (
        JsonConn<MockTextTransport<I>, I, O>,
        mpsc::Sender<I>,
        mpsc::Receiver<String>,
    ) {
        let (input_sender, input_receiver) = mpsc::channel(buffer_size);
        let (output_sender, output_receiver) = mpsc::channel(buffer_size);
        let mock_conn = JsonConn::new(MockTextTransport {
            fail_during_read_text,
            input_receiver,
            output_sender,
        });
        (mock_conn, input_sender, output_receiver)
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod json_conn_tests {
    use crate::conn::json_conn_test_constructs::new_test_json_conn;

    use super::*;
    use serde::{Deserialize, Deserializer};

    #[derive(Debug, PartialEq, Clone, Serialize)]
    enum TestType {
        VariantA,
        VariantB, // serializes but does not deserialize
    }

    impl<'de> Deserialize<'de> for TestType {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let s = String::deserialize(deserializer)?;

            match s.as_str() {
                "VariantA" => Ok(TestType::VariantA),
                "VariantB" => Err(serde::de::Error::custom(
                    // `VariantB` gives us a hook to make deserialization fail
                    "VariantB cannot be deserialized",
                )),
                _ => Err(serde::de::Error::unknown_variant(&s, &["VariantA"])),
            }
        }
    }

    // these unit tests simply test `JsonConn`: the wrapper over the `TextTransport`
    // as any other tests would be implementation specific

    // read_json() tests

    #[tokio::test]
    async fn GIVEN_input_WHEN_read_json_THEN_ok() {
        // GIVEN
        let (mut mock_conn, input_sender, _) = new_test_json_conn::<TestType, TestType>(false, 1);
        let request = TestType::VariantA; // will be serialized by `MockTextTransport` to then be deserialized by `JsonConn`
        input_sender.send(request.clone()).await.unwrap();

        // WHEN
        let payload = mock_conn.read_json().await.unwrap().unwrap();

        // THEN
        assert_eq!(payload, request); // ensure we get the same payload back
    }

    #[tokio::test]
    async fn GIVEN_closed_client_conn_WHEN_read_json_THEN_ok() {
        // GIVEN
        let (mut mock_conn, input_sender, _) = new_test_json_conn::<TestType, TestType>(false, 1);
        drop(input_sender); // TextTransport::read_text() returns `None`: clean disconnect

        // WHEN
        let result = mock_conn.read_json().await;

        // THEN
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn GIVEN_read_text_error_WHEN_read_json_THEN_error() {
        // GIVEN - `fail_during_read_text` is set to true so when read_text() is called, it will fail and propagate upward to read_json()
        let (mut mock_conn, input_sender, _) = new_test_json_conn::<TestType, TestType>(true, 1);
        input_sender.send(TestType::VariantA).await.unwrap();

        // WHEN
        let result = mock_conn.read_json().await.unwrap();

        // THEN
        assert!(matches!(result, Err(JsonConnError::Dependency(..))));
    }

    #[tokio::test]
    async fn GIVEN_deserialize_error_WHEN_read_json_THEN_error() {
        // GIVEN
        let (mut mock_conn, input_sender, _) = new_test_json_conn::<TestType, TestType>(false, 1);
        // this should serialize but not deserialize so it fails in read_json()
        input_sender.send(TestType::VariantB).await.unwrap();

        // WHEN
        let result = mock_conn.read_json().await.unwrap();

        // THEN
        assert!(matches!(result, Err(JsonConnError::Json(..))));
    }

    // send_json() tests

    #[tokio::test]
    async fn GIVEN_payload_WHEN_send_json_THEN_ok() {
        // GIVEN
        let (mut mock_conn, _, mut output_receiver) =
            new_test_json_conn::<TestType, TestType>(false, 1);
        let payload = TestType::VariantA;

        // WHEN
        mock_conn.send_json(&payload).await.unwrap();

        // THEN
        let received = output_receiver.recv().await.unwrap();
        let expected = serde_json::to_string(&TestType::VariantA).unwrap();
        assert_eq!(received, expected);
    }

    #[tokio::test]
    async fn GIVEN_closed_client_conn_WHEN_send_json_THEN_error() {
        // GIVEN
        let (mut mock_conn, _, output_receiver) =
            new_test_json_conn::<TestType, TestType>(false, 1);
        drop(output_receiver);

        // WHEN
        let result = mock_conn.send_json(&TestType::VariantA).await;

        // THEN
        assert!(matches!(result, Err(JsonConnError::Dependency(..))));
    }

    // disconnect() tests

    #[tokio::test]
    async fn GIVEN_mock_conn_WHEN_disconnect_THEN_ok() {
        // GIVEN
        let (mock_conn, _, _output_receiver) = new_test_json_conn::<TestType, TestType>(false, 1);

        // WHEN
        // Option<ErrorReason> isn't important here bc it's merely passed
        // to TextTransport::disconnect(..) for impl specific handling
        let result = mock_conn.disconnect(None).await;

        // THEN
        assert!(result.is_ok()); // simply ensure it passes as any other validation would be impl specific
    }

    #[tokio::test]
    async fn GIVEN_disconnected_mock_conn_WHEN_disconnect_THEN_error() {
        // GIVEN
        let (mock_conn, _, _) = new_test_json_conn::<TestType, TestType>(false, 1); // drop sender so disconnect() fails

        // WHEN
        let result = mock_conn.disconnect(None).await;

        // THEN
        assert!(matches!(result, Err(JsonConnError::Dependency(..))));
    }
}
