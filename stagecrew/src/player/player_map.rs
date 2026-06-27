use std::collections::HashMap;

use super::{Player, ReadPlayerCollection, WritePlayerCollection};

// implementation of ReadPlayerCollection / WritePlayerCollection traits for a HashMap

#[derive(Default)]
pub struct PlayerMap<P: Player> {
    map: HashMap<String, P>,
}

impl<P: Player> PlayerMap<P> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }
}

impl<P: Player> From<HashMap<String, P>> for PlayerMap<P> {
    fn from(map: HashMap<String, P>) -> Self {
        Self { map }
    }
}

impl<P: Player> ReadPlayerCollection<P> for PlayerMap<P> {
    fn contains(&self, id: &str) -> bool {
        self.map.contains_key(id)
    }

    fn get(&self, id: &str) -> Option<&P> {
        self.map.get(id)
    }

    fn get_mut(&mut self, id: &str) -> Option<&mut P> {
        self.map.get_mut(id)
    }

    fn len(&self) -> usize {
        self.map.len()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn iter(&self) -> Box<dyn Iterator<Item = &P> + '_> {
        Box::new(self.map.values())
    }
}

impl<P: Player> WritePlayerCollection<P> for PlayerMap<P> {
    fn add(&mut self, id: String, player: P) -> Option<P> {
        self.map.insert(id, player)
    }

    fn remove(&mut self, id: &str) -> Option<P> {
        self.map.remove(id)
    }
}
