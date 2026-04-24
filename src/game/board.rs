pub struct Board {

}

impl Board {
    pub fn new() -> Self {
        Self {
            // TODO:
            // - questions/answer
            // - double jeopardy
            // - possibly customizable question count?
        }
    }
}

pub struct Question {
    question: String,
    answer: String,
}