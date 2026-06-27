pub mod player_map;

// all of these are Send + 'static because they work with `ActorLobby`
// which uses an internal tokio::task (async)

// a Player is simply anything with an ID
// generic and extensible for any game
pub trait Player: Send + 'static {
    fn id(&self) -> &str;
}

/// Any game shouldn't know the underlying player collection / data structure.
/// It should just have some sort of trait to get players.
/// Therefore, Read/WritePlayers are traits to abstract interfacing with the data structure.
/// Consequently, `Game` manages the players' data but the `Lobby` manages the players
// yes - it does need to be reimplemented per data structure (bad)
// TODO: evaluate if there is a stdlib way / existing trait
pub trait ReadPlayerCollection<P: Player>: Send + 'static {
    fn contains(&self, id: &str) -> bool;
    fn get(&self, id: &str) -> Option<&P>;
    fn get_mut(&mut self, id: &str) -> Option<&mut P>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn iter(&self) -> Box<dyn Iterator<Item = &P> + '_>;
}

pub trait WritePlayerCollection<P: Player>: Send + 'static {
    fn add(&mut self, id: String, player: P) -> Option<P>;
    fn remove(&mut self, id: &str) -> Option<P>;
}
