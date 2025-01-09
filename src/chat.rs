use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use tiktoken::{self, ChatMessage as TiktokenChatMessage, ChatRequest as TiktokenChatRequest};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct Req {
    pub model: String,
    pub messages: Vec<Msg>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct Msg {
    pub role: String,
    pub content: MsgContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub enum MsgContent {
    String(String),
    Array(Vec<ContentItem>),
}

#[derive(Serialize, Deserialize, ToSchema)]
#[serde(tag = "type")]
pub enum ContentItem {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    Image { image_url: ImageUrl },
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ImageUrl {
    pub url: String,
}

impl Req {
    pub fn tokens_estimate(&self) -> usize {
        // For non-OpenAI models, fall back to a simpler estimation
        if !self.model.starts_with("gpt-") {
            return self.messages.iter().map(Msg::tokens_estimate).sum();
        }

        // For OpenAI models, use tiktoken's accurate counting
        let tiktoken_request = TiktokenChatRequest {
            model: self.model.clone(),
            messages: self.messages.iter().map(|msg| TiktokenChatMessage {
                role: msg.role.clone(),
                content: msg.content.to_string(),
                name: msg.name.clone(),
            }).collect(),
        };

        tiktoken::count_request(&tiktoken_request) as usize
    }
}

impl Msg {
    fn tokens_estimate(&self) -> usize {
        match &self.content {
            MsgContent::String(content) => {
                let word_count = content.split_whitespace().count();
                (word_count + (word_count / 5)).max(1)
            }
            MsgContent::Array(content) => {
                content.iter().map(|item| match item {
                    ContentItem::Text { text } => {
                        let word_count = text.split_whitespace().count();
                        (word_count + (word_count / 5)).max(1)
                    }
                    ContentItem::Image { .. } => {
                        // Conservative estimate for image tokens
                        1024
                    }
                }).sum()
            }
        }
    }
}

impl ToString for MsgContent {
    fn to_string(&self) -> String {
        match self {
            MsgContent::String(content) => content.clone(),
            MsgContent::Array(content) => {
                content.iter()
                    .filter_map(|item| match item {
                        ContentItem::Text { text } => Some(text.clone()),
                        ContentItem::Image { .. } => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
    }
}
