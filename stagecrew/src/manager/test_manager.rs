// helper struct to induce failures in a manager for testing
// it is a thin wrapper around MapManager with a counter
// to induce failure after n manager operations
#[cfg(feature = "test-util")]
pub mod test_manager_constructs {
    use std::sync::Mutex;

    use thiserror::Error;

    use crate::manager::{Manager, ManagerEntry, ManagerError, MapManager};

    #[derive(Debug, Error)]
    pub enum TestManagerError {
        #[error("test induced failure")]
        TestInducedError,
    }

    pub struct TestManager<E: ManagerEntry> {
        manager: MapManager<E>,
        // this is lowk terrible
        // tl;dr i need indirection to make this mutable bc of the Manager trait
        // - i cant use a RefCell/Cell bc they aren't Send (tokio)
        // - i can't use a tokio::Mutex/RwLock/Arc bc Manager trait isn't async
        // - i don't want to change Manager to async bc it doesn't need to be
        // - i use a std::sync::Mutex to get indirect mutability + Send, albeit blocking/unwrap
        // this is a test so it's fine
        failure_config: Mutex<FailureConfig>,
    }

    #[derive(Default, Clone, Copy)]
    struct FailureConfig {
        valid_operation_count: usize,
        fail_after_n: usize,
    }

    impl<E: ManagerEntry> Default for TestManager<E> {
        fn default() -> Self {
            Self {
                manager: Default::default(),
                failure_config: Mutex::new(FailureConfig::default()),
            }
        }
    }

    impl<E: ManagerEntry> TestManager<E> {
        pub fn fail(&self) -> bool {
            let config = self.failure_config.lock().unwrap();
            config.valid_operation_count >= config.fail_after_n
        }
        pub fn set_fail_after_n(&mut self, n: usize) {
            self.failure_config.lock().unwrap().fail_after_n = n;
        }
        pub fn set_always_fail(&mut self) {
            let mut config = self.failure_config.lock().unwrap();
            config.valid_operation_count = config.fail_after_n;
        }
        pub fn reset(&mut self) {
            self.failure_config.lock().unwrap().valid_operation_count = 0;
        }
        pub fn set_never_fail(&mut self) {
            self.reset();
            self.set_fail_after_n(usize::MAX);
        }
    }

    impl<E: ManagerEntry> Manager for TestManager<E> {
        type Entry = E;

        fn has(&self, id: &str) -> Result<bool, ManagerError> {
            let failure = Err(ManagerError::Dependency(
                TestManagerError::TestInducedError.into(),
            ));
            if self.fail() {
                return failure;
            }
            let result = self.manager.has(id);
            self.failure_config.lock().unwrap().valid_operation_count += 1;
            result
        }

        fn get(&self, id: &str) -> Result<&Self::Entry, ManagerError> {
            let failure = Err(ManagerError::Dependency(
                TestManagerError::TestInducedError.into(),
            ));
            if self.fail() {
                return failure;
            }
            let result = self.manager.get(id);
            self.failure_config.lock().unwrap().valid_operation_count += 1;
            result
        }

        fn add(&mut self, id: &str, entry: Self::Entry) -> Result<(), ManagerError> {
            let failure = Err(ManagerError::Dependency(
                TestManagerError::TestInducedError.into(),
            ));
            if self.fail() {
                return failure;
            }
            let result = self.manager.add(id, entry);
            self.failure_config.lock().unwrap().valid_operation_count += 1;
            result
        }

        fn remove(&mut self, id: &str) -> Result<Self::Entry, ManagerError> {
            let failure = Err(ManagerError::Dependency(
                TestManagerError::TestInducedError.into(),
            ));
            if self.fail() {
                return failure;
            }
            let result = self.manager.remove(id);
            self.failure_config.lock().unwrap().valid_operation_count += 1;
            result
        }

        fn len(&self) -> Result<usize, ManagerError> {
            let failure = Err(ManagerError::Dependency(
                TestManagerError::TestInducedError.into(),
            ));
            if self.fail() {
                return failure;
            }
            let result = self.manager.len();
            self.failure_config.lock().unwrap().valid_operation_count += 1;
            result
        }

        fn is_empty(&self) -> Result<bool, ManagerError> {
            let failure = Err(ManagerError::Dependency(
                TestManagerError::TestInducedError.into(),
            ));
            if self.fail() {
                return failure;
            }
            let result = self.manager.is_empty();
            self.failure_config.lock().unwrap().valid_operation_count += 1;
            result
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod test_manager_tests {
    use crate::manager::{ManagerEntry, ManagerError};
    use crate::{
        lobby::lobby_test_constructs::TestGame,
        manager::{Manager, PasswordProtectedLobby, test_manager_constructs::TestManager},
    };

    #[tokio::test]
    async fn GIVEN_always_fail_WHEN_manager_THEN_ok() {
        // GIVEN
        let mut manager = TestManager::<PasswordProtectedLobby<TestGame>>::default();
        manager.set_always_fail(); // every operation should fail

        // WHEN / THEN
        assert!(matches!(manager.has(""), Err(ManagerError::Dependency(..))));
        assert!(matches!(manager.get(""), Err(ManagerError::Dependency(..))));
        assert!(matches!(manager.len(), Err(ManagerError::Dependency(..))));
        assert!(matches!(
            manager.is_empty(),
            Err(ManagerError::Dependency(..))
        ));
        assert!(matches!(
            manager.add(
                "",
                PasswordProtectedLobby::with_test_game("".to_string(), "".to_string())
            ),
            Err(ManagerError::Dependency(..))
        ));
        assert!(matches!(
            manager.remove(""),
            Err(ManagerError::Dependency(..))
        ));
    }

    #[tokio::test]
    async fn GIVEN_never_fail_WHEN_manager_THEN_ok() {
        // GIVEN
        let mut manager = TestManager::<PasswordProtectedLobby<TestGame>>::default();
        manager.set_never_fail();

        // WHEN
        for _ in 0..u16::MAX {
            // i can't do usize::MAX bc it would take too long
            // these should all pass normally
            assert!(matches!(manager.is_empty(), Ok(true)));
            assert!(matches!(
                manager.add(
                    "",
                    PasswordProtectedLobby::with_test_game("".to_string(), "".to_string())
                ),
                Ok(())
            ));
            assert!(matches!(manager.has(""), Ok(true)));
            assert!(matches!(
                manager.len(),
                Ok(len) if len == 1
            ));
            assert!(matches!(manager.is_empty(), Ok(false)));
            assert!(matches!(
                manager.remove(""),
                Ok(entry) if entry.id() == ""
            ));
        }
    }

    #[tokio::test]
    async fn GIVEN_n_operations_WHEN_manager_THEN_ok() {
        // GIVEN
        let mut manager = TestManager::<PasswordProtectedLobby<TestGame>>::default();
        let n = 600; // 600 so that n/6 operations divides nicely
        manager.set_fail_after_n(n);

        // WHEN
        for _ in 0..n / 6 {
            assert!(matches!(manager.is_empty(), Ok(true)));
            // these should all pass normally
            assert!(matches!(
                manager.add(
                    "",
                    PasswordProtectedLobby::with_test_game("".to_string(), "".to_string())
                ),
                Ok(())
            ));
            assert!(manager.has("").is_ok());
            assert!(matches!(
                manager.len(),
                Ok(len) if len == 1
            ));
            assert!(matches!(manager.is_empty(), Ok(false)));
            assert!(matches!(
                manager.remove(""),
                Ok(entry) if entry.id() == ""
            ));
        }

        // THEN
        assert!(matches!(manager.has(""), Err(ManagerError::Dependency(..))));
    }

    #[tokio::test]
    async fn GIVEN_n_operations_WHEN_reset_THEN_ok() {
        // GIVEN
        let mut manager = TestManager::<PasswordProtectedLobby<TestGame>>::default();
        let entry = PasswordProtectedLobby::with_test_game("".to_string(), "".to_string());
        manager.set_fail_after_n(1); // dummy value so we can add
        manager.add("", entry).unwrap(); // add dummy lobby for get(..) to run against 
        manager.reset();

        for _ in 0..2 {
            let n = 100;
            manager.set_fail_after_n(n);
            for _ in 0..n {
                assert!(matches!(
                    manager.get(""),
                    Ok(entry) if entry.id() == ""
                ));
            }
            // THEN
            assert!(matches!(manager.get(""), Err(ManagerError::Dependency(..))));
            // WHEN
            manager.reset(); // this should allow the loop to run twice, internal counter is reset
        }
    }

    // technically, we should test every variant to be exhaustive
    // but this is too simple to be worth the trouble
}
