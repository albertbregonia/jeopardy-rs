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
    Dependency(#[from] Box<dyn Error>),
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

// we use `impl Future<>` instead of `async fn` as per compiler suggestion / backwards compatibility
pub trait TextTransport {
    type Error: Error + 'static;
    fn read_text(&mut self) -> impl Future<Output = Option<Result<Bytes, Self::Error>>>;
    fn send_text(&mut self, msg: &str) -> impl Future<Output = Result<(), Self::Error>>;
    fn disconnect(
        self,
        internal_error: bool,
        msg: Option<&str>,
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

    pub async fn disconnect(
        self,
        internal_error: bool,
        msg: Option<&str>,
    ) -> Result<(), JsonConnError> {
        // NOTE: this consumes the object (effectively dropping the underlying socket)
        // this is also just a re-export of TextTransport disconnect
        self.transport
            .disconnect(internal_error, msg)
            .await
            .map_err(|e| JsonConnError::Dependency(e.into()))?;
        Ok(())
    }
}

// publicly accessible mock for use in unit tests
// receiver: allows us to mock JSON requests coming in
// sender: allows us to see what gets sent to the client
#[cfg(test)]
mod json_conn_test_constructs {
    use super::*;
    use tokio::sync::mpsc::{Receiver, Sender};

    pub struct MockConn<T: Serialize> {
        pub receiver: Receiver<T>,
        pub sender: Sender<String>,
    }

    #[derive(Debug, Error)]
    pub enum MockError {
        #[error(transparent)]
        Derivative(#[from] Box<dyn Error>),
        #[error("Generic Error")]
        Generic,
    }

    impl<T: Serialize> TextTransport for MockConn<T> {
        type Error = MockError;
        async fn read_text(&mut self) -> Option<Result<Bytes, Self::Error>> {
            let msg = self.receiver.recv().await?;
            let serialized = match serde_json::to_vec(&msg) {
                Ok(bin) => bin,
                Err(e) => return Some(Err(MockError::Derivative(e.into()))),
            };
            Some(Ok(Bytes::from(serialized)))
        }

        async fn send_text(&mut self, msg: &str) -> Result<(), Self::Error> {
            self.sender
                .send(msg.to_string())
                .await
                .map_err(|e| MockError::Derivative(e.into()))?;
            Ok(())
        }

        async fn disconnect(
            self,
            internal_error: bool,
            _msg: Option<&str>,
        ) -> Result<(), Self::Error> {
            // receiver and sender will be dropped automatically

            // internal error is only used here to induce an error
            if internal_error {
                Err(MockError::Generic)
            } else {
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod json_conn_tests {
    use super::*;
    use crate::conn::json_conn_test_constructs::MockConn;
    use serde::ser::Error;
    use serde::{Deserialize, Deserializer};
    use tokio::sync::mpsc;

    #[derive(Debug, PartialEq, Clone)]
    enum TestType {
        VariantA,
        VariantB(bool),
    }

    impl Serialize for TestType {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            match self {
                TestType::VariantA => serializer.serialize_unit_variant("TestType", 0, "VariantA"),
                TestType::VariantB(should_fail) => {
                    if *should_fail {
                        // `should_fail` gives us a hook to make serialization fail
                        Err(S::Error::custom("VariantB cannot be serialized"))
                    } else {
                        serializer.serialize_bool(*should_fail)
                    }
                }
            }
        }
    }

    impl<'de> Deserialize<'de> for TestType {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
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
    // tbh some of these tests are simply for the sake of testing (ie. disconnect)

    fn new_mock_conn_with_io_hooks() -> (
        JsonConn<MockConn<TestType>, TestType, TestType>,
        mpsc::Sender<TestType>,
        mpsc::Receiver<String>,
    ) {
        let (input_sender, input_receiver) = mpsc::channel(1);
        let (output_sender, output_receiver) = mpsc::channel(1);
        let mock_ws = JsonConn::new(MockConn {
            receiver: input_receiver,
            sender: output_sender,
        });
        (mock_ws, input_sender, output_receiver)
    }

    // read_json() tests

    #[tokio::test]
    async fn GIVEN_input_WHEN_read_json_THEN_ok() {
        // GIVEN
        let (mut mock_ws, input_sender, _) = new_mock_conn_with_io_hooks();
        let request = TestType::VariantA;
        input_sender.send(request.clone()).await.unwrap();

        // WHEN
        let result: Result<TestType, JsonConnError> = mock_ws.read_json().await.unwrap();

        // THEN
        assert!(matches!(
            result,
            Ok(payload) if payload == request,
        ));
    }

    #[tokio::test]
    async fn GIVEN_closed_client_conn_WHEN_read_json_THEN_ok() {
        // GIVEN
        let (mut mock_ws, input_sender, _) = new_mock_conn_with_io_hooks();
        drop(input_sender); // clean disconnect

        // WHEN
        let result = mock_ws.read_json().await;

        // THEN
        assert!(matches!(result, None));
    }

    #[tokio::test]
    async fn GIVEN_read_text_error_WHEN_read_json_THEN_error() {
        // GIVEN
        let (mut mock_ws, input_sender, _) = new_mock_conn_with_io_hooks();
        // this doesn't serialize in MockConn so it errors in read_text()
        input_sender.send(TestType::VariantB(true)).await.unwrap(); 

        // WHEN
        let result = mock_ws.read_json().await.unwrap();

        // THEN
        assert!(matches!(result, Err(JsonConnError::Dependency(..)),));
    }

    #[tokio::test]
    async fn GIVEN_deserialize_error_WHEN_read_json_THEN_error() {
        // GIVEN
        let (mut mock_ws, input_sender, _) = new_mock_conn_with_io_hooks();
        // this should serialize but not deserialize so it fails in read_json()
        input_sender.send(TestType::VariantB(false)).await.unwrap(); 

        // WHEN
        let result = mock_ws.read_json().await.unwrap();

        // THEN
        assert!(matches!(result, Err(JsonConnError::Json(..)),));
    }

    // send_json() tests

    #[tokio::test]
    async fn GIVEN_payload_WHEN_send_json_THEN_ok() {
        // GIVEN
        let (mut mock_ws, _, mut output_receiver) = new_mock_conn_with_io_hooks();
        let payload = TestType::VariantA;

        // WHEN
        mock_ws.send_json(&payload).await.unwrap();

        // THEN
        let received = output_receiver.recv().await.unwrap();
        let deserialized = serde_json::from_str::<TestType>(&received).unwrap();
        assert_eq!(deserialized, payload);
    }

    #[tokio::test]
    async fn GIVEN_closed_client_conn_WHEN_send_json_THEN_error() {
        // GIVEN
        let (mut mock_ws, _, output_receiver) = new_mock_conn_with_io_hooks();
        drop(output_receiver);

        // WHEN
        let result = mock_ws.send_json(&TestType::VariantA).await;

        // THEN
        assert!(matches!(result, Err(JsonConnError::Dependency(..)),));
    }

    // disconnect() tests

    #[tokio::test]
    async fn GIVEN_mock_conn_WHEN_disconnect_THEN_ok() {
        // GIVEN
        let (mock_ws, _, _) = new_mock_conn_with_io_hooks();

        // WHEN
        let result = mock_ws.disconnect(false, None).await;

        // THEN
        assert!(matches!(result, Ok(())));
    }

    #[tokio::test]
    async fn GIVEN_mock_conn_WHEN_disconnect_THEN_error() {
        // GIVEN
        let (mock_ws, _, _) = new_mock_conn_with_io_hooks();

        // WHEN
        let result = mock_ws.disconnect(true, None).await;

        // THEN
        assert!(matches!(result, Err(JsonConnError::Dependency(..))));
    }
}
