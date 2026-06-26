use stagecrew::player::Player;

pub struct JeopardyPlayer {
    name: String,
}

impl Player for JeopardyPlayer {
    fn id(&self) -> &str {
        &self.name
    }
}
