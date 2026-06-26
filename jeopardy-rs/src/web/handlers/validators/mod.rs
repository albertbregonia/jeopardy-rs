pub mod nonzero_ascii;

/// trait used to be a unifying abstraction over what constitutes "valid" strings
/// aka valid lobby IDs / usernames / passwords etc.
pub trait CredsValidator {
    fn is_valid_lobby_id(&self, id: &str) -> bool;
    fn is_valid_username(&self, username: &str) -> bool;
    fn is_valid_host_password(&self, password: &str) -> bool;
    fn is_valid_lobby_password(&self, password: &str) -> bool;
}
