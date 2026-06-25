# stagecrew

`stagecrew` or *Stage Crew* is a Rust library built on `tokio` for ephemeral, in-memory, thread-safe, game lobbies.
- Each lobby follows the actor model in which each `Lobby` struct contains a `mpsc::Sender` handle to a dedicated task that handles players and game state.
- Simply create a struct for game state and implement the trait `Game` to hook into the functionality.
- Also includes traits for managing collections of lobbies (`Manager`) and small wrappers to contain metadata about each lobby (`ManagerEntry`)
- I built this because I wanted a lightweight, reusable library for my multiplayer web games and I named it stage crew because this library provides the setup for the *actors*.

# Example
```rust
// TODO: imports and sample code
```