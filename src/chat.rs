#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChatReq {
    pub model: String,
    pub messages: Vec<ChatMsg>,
}

impl ChatReq {
    pub fn tokens_estimate(&self) -> usize {
        self.messages.iter().map(|msg| msg.tokens_estimate()).sum()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
    pub name: String,
}

impl ChatMsg {
    // The simplest estimation suggested by ChatGPT: (char count / 4).
    fn tokens_estimate(&self) -> usize {
        let alphanum_char_count = self
            .content
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .count();
        alphanum_char_count.saturating_div(4)
        // TODO Consider using toktoken after cleaning it up:
        //      https://github.com/xandkar/tiktoken
    }
}
