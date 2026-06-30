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
        let mut manager = MapManager::new();
        let id = "1".to_string();
        let entry = PasswordProtectedLobby::with_test_game(id.clone(), "".to_string());
        let len_before = manager.len().unwrap();

        // WHEN
        manager.add(&id, entry).unwrap();

        // THEN
        let result = manager.has(&id);
        assert!(matches!(
            result,
            Ok(exists) if exists
        ));
        let len_after = manager.len().unwrap();
        assert_eq!(len_before + 1, len_after); // added
    }

    #[tokio::test]
    async fn GIVEN_duplicate_entry_id_WHEN_add_THEN_error() {
        // GIVEN
        let mut manager = MapManager::default();
        let id = "1".to_string();
        let entry = PasswordProtectedLobby::with_test_game(id.clone(), "".to_string());
        manager.add(&id, entry).unwrap();
        assert!(manager.has(&id).unwrap());
        let len_before = manager.len().unwrap();

        // WHEN
        let duplicate = PasswordProtectedLobby::with_test_game(id.clone(), "".to_string());
        let result = manager.add(&id, duplicate);

        // THEN
        assert!(matches!(
            result,
            Err(ManagerError::EntryIDConflict(id)) if id == id
        ));
        let len_after = manager.len().unwrap();
        assert_eq!(len_before, len_after); // unchanged
    }

    #[tokio::test]
    async fn GIVEN_existing_entry_WHEN_remove_THEN_ok() {
        // GIVEN
        let mut manager = MapManager::default();
        let id = "1".to_string();
        let entry = PasswordProtectedLobby::with_test_game(id.clone(), "".to_string());
        manager.add(&id, entry).unwrap();
        assert!(manager.has(&id).unwrap());
        let len_before = manager.len().unwrap();

        // WHEN
        let result = manager.remove(&id);

        // THEN
        assert!(matches!(
            result,
            Ok(entry) if entry.id() == id
        ));
        let len_after = manager.len().unwrap();
        assert_eq!(len_before - 1, len_after); // removed
    }

    #[tokio::test]
    async fn GIVEN_nonexistant_entry_id_WHEN_remove_THEN_error() {
        // GIVEN
        let mut manager = MapManager::default();
        let id = "1".to_string();
        let entry = PasswordProtectedLobby::with_test_game(id.clone(), "".to_string());
        manager.add(&id, entry).unwrap();
        assert!(manager.has(&id).unwrap());
        let len_before = manager.len().unwrap();

        // WHEN
        let invalid_id = "2";
        assert_eq!(manager.has(&invalid_id).unwrap(), false);
        let result = manager.remove(invalid_id);

        // THEN
        assert!(matches!(
            result,
            Err(ManagerError::EntryNotFound(id)) if id == invalid_id
        ));
        let len_after = manager.len().unwrap();
        assert_eq!(len_before, len_after); // unchanged
    }

    #[tokio::test]
    async fn GIVEN_existing_entry_WHEN_get_THEN_ok() {
        // GIVEN
        let mut manager = MapManager::default();
        let id = "1".to_string();
        let entry = PasswordProtectedLobby::with_test_game(id.clone(), "".to_string());
        manager.add(&id, entry).unwrap();
        assert!(manager.has(&id).unwrap());

        // WHEN
        let result = manager.get(&id);

        // THEN
        assert!(matches!(
            result,
            Ok(entry) if entry.id() == id
        ))
    }

    #[tokio::test]
    async fn GIVEN_nonexistant_entry_id_WHEN_get_THEN_error() {
        // GIVEN
        let mut manager = MapManager::default();
        let id = "1".to_string();
        let entry = PasswordProtectedLobby::with_test_game(id.clone(), "".to_string());
        manager.add(&id, entry).unwrap();
        assert!(manager.has(&id).unwrap());

        // WHEN
        let invalid_id = "2";
        assert_eq!(manager.has(&invalid_id).unwrap(), false);
        let result = manager.get(invalid_id);

        // THEN
        assert!(matches!(
            result,
            Err(ManagerError::EntryNotFound(id)) if id == invalid_id
        ))
    }

    #[tokio::test]
    async fn GIVEN_entry_WHEN_is_empty_THEN_ok() {
        // GIVEN
        let mut manager = MapManager::default();
        
        // WHEN / THEN
        assert!(manager.is_empty().unwrap()); // is_empty is infallible for MapManager so no negative tests

        let id = "1".to_string();
        let entry = PasswordProtectedLobby::with_test_game(id.clone(), "".to_string());
        manager.add(&id, entry).unwrap();
        
        assert_eq!(false, manager.is_empty().unwrap());
    }
}
