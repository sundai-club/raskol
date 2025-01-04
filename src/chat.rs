#[derive(serde::Serialize, serde::Deserialize)]
pub struct Req {
    pub model: String,
    pub messages: Vec<Msg>,
}

impl Req {
    pub fn tokens_estimate(&self) -> usize {
        self.messages.iter().map(Msg::tokens_estimate).sum()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Msg {
    pub role: String,
    pub content: String,

    // XXX Without skipping we get JSON `"name": null`, which Groq rejects,
    //     but accepts when it is instead omitted from the structure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Msg {
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
