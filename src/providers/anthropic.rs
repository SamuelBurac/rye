use super::LLMProvider;
use async_trait::async_trait;
use futures::stream::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::pin::Pin;

#[derive(Serialize)]
struct AnthropicMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    stream: bool,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: String,
}

#[derive(Deserialize, Debug)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    delta: Option<Delta>,
}

#[derive(Deserialize, Debug)]
struct Delta {
    text: Option<String>,
}

pub struct AnthropicProvider {
    client: Client,
    api_key: String,
    model: String,
}

impl AnthropicProvider {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let api_key = env::var("ANTHROPIC_API_KEY")
            .map_err(|_| "ANTHROPIC_API_KEY environment variable not set")?;

        let model =
            env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

        Ok(Self {
            client: Client::new(),
            api_key,
            model,
        })
    }
}

#[async_trait]
impl LLMProvider for AnthropicProvider {
    async fn generate_response_stream(
        &self,
        messages: &[(String, String)],
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<String, Box<dyn std::error::Error + Send>>> + Send>>,
        Box<dyn std::error::Error>,
    > {
        let mut api_messages = Vec::new();

        let system_message = "You are a helpful assistant. Always respond in markdown format. When referring to information you've previously provided in this conversation, reference the relevant sections instead of repeating the information. Be concise and avoid unnecessary repetition.";

        for (role, content) in messages {
            api_messages.push(AnthropicMessage {
                role: role.clone(),
                content: if role == "user" && !messages.is_empty() {
                    format!("{}\n\nSystem instruction: {}", content, system_message)
                } else {
                    content.clone()
                },
            });
        }

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 4096,
            messages: api_messages,
            stream: true,
        };

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let error_text = response.text().await?;
            return Err(format!("API Error: {}", error_text).into());
        }

        let stream = response.bytes_stream().map(|chunk| {
            let bytes = chunk.map_err(|e| -> Box<dyn std::error::Error + Send> { Box::new(e) })?;
            let text = String::from_utf8_lossy(&bytes);

            // Parse SSE events
            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        continue;
                    }

                    if let Ok(event) = serde_json::from_str::<StreamEvent>(data) {
                        if event.event_type == "content_block_delta" {
                            if let Some(delta) = event.delta {
                                if let Some(text) = delta.text {
                                    return Ok(text);
                                }
                            }
                        }
                    }
                }
            }

            Ok(String::new())
        });

        Ok(Box::pin(stream))
    }

    async fn generate_title(
        &self,
        user_message: &str,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let title_prompt = format!(
            "Generate a concise, descriptive title (max 50 characters) for a conversation that starts with this user message: \"{}\"\n\nRespond with ONLY the title, no additional text or formatting.",
            user_message
        );

        let request = AnthropicRequest {
            model: self.model.clone(),
            max_tokens: 100,
            messages: vec![AnthropicMessage {
                role: "user".to_string(),
                content: title_prompt,
            }],
            stream: false,
        };

        let response = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err("Failed to generate title".into());
        }

        let api_response: AnthropicResponse = response.json().await?;

        if let Some(content) = api_response.content.first() {
            Ok(content.text.trim().to_string())
        } else {
            Err("No title generated".into())
        }
    }
}
