use std::collections::HashMap;

use super::{Manager, ManagerEntry, ManagerError};

pub struct MapManager<E: ManagerEntry> {
    lobbies: HashMap<String, E>,
}

impl<E: ManagerEntry> MapManager<E> {
    pub fn new() -> Self {
        Self {
            lobbies: HashMap::new(),
        }
    }
}

impl<E: ManagerEntry> Default for MapManager<E> {
    fn default() -> Self {
        Self {
            lobbies: Default::default(),
        }
    }
}

impl<E: ManagerEntry> Manager for MapManager<E> {
    type Entry = E;
    fn has(&self, id: &str) -> Result<bool, ManagerError> {
        Ok(self.lobbies.contains_key(id))
    }

    fn get(&self, id: &str) -> Result<&Self::Entry, ManagerError> {
        self.lobbies
            .get(id)
            .ok_or_else(|| ManagerError::EntryNotFound(id.to_string()))
    }

    fn add(&mut self, id: &str, entry: Self::Entry) -> Result<(), ManagerError> {
        if self.lobbies.contains_key(id) {
            return Err(ManagerError::EntryIDConflict(id.to_string()));
        }
        self.lobbies.insert(id.to_string(), entry);
        Ok(())
    }

    fn remove(&mut self, id: &str) -> Result<Self::Entry, ManagerError> {
        self.lobbies
            .remove(id)
            .ok_or_else(|| ManagerError::EntryNotFound(id.to_string()))
    }

    /// infallible, however the signature cannot guarantee
    fn len(&self) -> Result<usize, ManagerError> {
        Ok(self.lobbies.len())
    }

    fn is_empty(&self) -> Result<bool, ManagerError> {
        Ok(self.len()? == 0)
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod map_manager_tests {
    use crate::manager::{Manager, ManagerEntry, ManagerError, MapManager, PasswordProtectedLobby};

    // although we don't use await anywhere,
    // the underlying lobby needs a tokio runtime therefore: tokio::test

    #[tokio::test]
    async fn GIVEN_entry_WHEN_add_THEN_ok() {
        // GIVEN
        let mut manager = MapManager::default();
        let id = "1";
        let entry = PasswordProtectedLobby::with_test_game(id.to_string(), "password".to_string());

        // preconditions
        let len_before = manager.len().unwrap();
        let has_entry = manager.has(id).unwrap();
        assert_eq!(false, has_entry); // does not exist before add

        // WHEN
        manager.add(id, entry).unwrap();

        // THEN
        let len_after = manager.len().unwrap();
        assert_eq!(len_before + 1, len_after); // added

        let has_entry = manager.has(id).unwrap();
        assert!(has_entry); // has() now reflects the insertion
    }

    #[tokio::test]
    async fn GIVEN_duplicate_entry_id_WHEN_add_THEN_error() {
        // GIVEN
        let mut manager = MapManager::default();
        let id = "1";
        let password = "password";
        let entry = PasswordProtectedLobby::with_test_game(id.to_string(), password.to_string());
        manager.add(id, entry).unwrap();

        // preconditions
        let len_before = manager.len().unwrap();
        let has_entry = manager.has(id).unwrap();
        assert!(has_entry);

        // WHEN
        let duplicate =  // duplicate id but diff password to check overwrite
            PasswordProtectedLobby::with_test_game(id.to_string(), "password2".to_string());
        let result = manager.add(id, duplicate);

        // THEN
        assert!(matches!(
            result,
            Err(ManagerError::EntryIDConflict(conflict_id)) if conflict_id == id
        ));

        let len_after = manager.len().unwrap();
        assert_eq!(len_before, len_after); // unchanged

        let has_entry = manager.has(id).unwrap();
        assert!(has_entry); // existing entry persists

        let entry = manager.get(id).unwrap();
        assert!(entry.is_correct_password(password)) // ensure not overwritten, would be false if so
    }

    #[tokio::test]
    async fn GIVEN_existing_entry_WHEN_remove_THEN_ok() {
        // GIVEN
        let mut manager = MapManager::default();
        let id = "1";
        let entry = PasswordProtectedLobby::with_test_game(id.to_string(), "password".to_string());
        manager.add(id, entry).unwrap();

        // preconditions
        let len_before = manager.len().unwrap();
        let has_entry = manager.has(id).unwrap();
        assert!(has_entry);

        // WHEN
        let entry = manager.remove(id).unwrap();

        // THEN
        assert_eq!(id, entry.id()); // returned entry is the expected entry

        let has_entry = manager.has(id).unwrap();
        assert_eq!(false, has_entry); // has() reflects the removal

        let len_after = manager.len().unwrap();
        assert_eq!(len_before - 1, len_after); // len reflects removed
    }

    #[tokio::test]
    async fn GIVEN_nonexistant_entry_id_WHEN_remove_THEN_error() {
        // GIVEN
        let mut manager = MapManager::default();
        let valid_id = "1";
        let entry =
            PasswordProtectedLobby::with_test_game(valid_id.to_string(), "password".to_string());
        manager.add(valid_id, entry).unwrap(); // add dummy player just to ensure !empty
        let invalid_id = "2";

        // preconditions
        let len_before = manager.len().unwrap();
        let has_entry = manager.has(invalid_id).unwrap();
        assert_eq!(false, has_entry);

        // WHEN
        let result = manager.remove(invalid_id);

        // THEN
        assert!(matches!(
            result,
            Err(ManagerError::EntryNotFound(id)) if id == invalid_id
        ));

        let len_after = manager.len().unwrap();
        assert_eq!(len_before, len_after); // unchanged

        let has_entry = manager.has(invalid_id).unwrap();
        assert_eq!(false, has_entry); // unchanged
    }

    #[tokio::test]
    async fn GIVEN_existing_entry_WHEN_get_THEN_ok() {
        // GIVEN
        let mut manager = MapManager::default();
        let id = "1";
        let entry = PasswordProtectedLobby::with_test_game(id.to_string(), "password".to_string());
        manager.add(id, entry).unwrap();

        // preconditions
        let has_entry = manager.has(id).unwrap();
        assert!(has_entry);

        // WHEN
        let entry = manager.get(id).unwrap();

        // THEN
        assert_eq!(id, entry.id());
    }

    #[tokio::test]
    async fn GIVEN_nonexistant_entry_id_WHEN_get_THEN_error() {
        // GIVEN
        let mut manager = MapManager::default();
        let valid_id = "1";
        let invalid_id = "2";
        let entry =
            PasswordProtectedLobby::with_test_game(valid_id.to_string(), "password".to_string());
        manager.add(valid_id, entry).unwrap();

        // preconditions
        let has_entry = manager.has(invalid_id).unwrap();
        assert_eq!(false, has_entry);

        // WHEN
        let result = manager.get(invalid_id);

        // THEN
        assert!(matches!(
            result,
            Err(ManagerError::EntryNotFound(id)) if id == invalid_id
        ));
    }

    #[tokio::test] // positive and negative cases
    async fn GIVEN_entry_id_WHEN_has_THEN_ok() {
        // we use has() throughout the test suite to ensure post-conditions; ensure it responds how we expect

        // GIVEN
        let mut manager = MapManager::default();
        let valid_id = "1";
        let invalid_id = "2";
        let entry =
            PasswordProtectedLobby::with_test_game(valid_id.to_string(), "password".to_string());
        manager.add(valid_id, entry).unwrap();

        // WHEN
        let has_valid_entry = manager.has(valid_id).unwrap();
        let has_invalid_entry = manager.has(invalid_id).unwrap();

        // THEN
        assert!(has_valid_entry);
        assert_eq!(false, has_invalid_entry);
    }

    #[tokio::test] // is_empty / len tests
    async fn GIVEN_manager_WHEN_len_THEN_ok() {
        // GIVEN
        let mut manager = MapManager::new();
        let is_empty = manager.is_empty().unwrap();
        assert!(is_empty); // newly created, must be empty
        assert_eq!(0, manager.len().unwrap()); // redundant but ensures len() matches is_empty()

        // WHEN
        let expected_len = 10;
        for i in 0..expected_len {
            let id = i.to_string();
            let entry = PasswordProtectedLobby::with_test_game(id.clone(), "password".to_string());
            manager.add(&id, entry).unwrap();
        }

        // THEN
        let observed_len = manager.len().unwrap();
        assert_eq!(observed_len, expected_len);

        let is_empty = manager.is_empty().unwrap();
        assert_eq!(false, is_empty);
    }
}
