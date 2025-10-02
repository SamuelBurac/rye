use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

pub mod anthropic;

// Generic LLM trait
#[async_trait]
pub trait LLMProvider: Send + Sync {
    async fn generate_response_stream(
        &self,
        messages: &[(String, String)],
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<String, Box<dyn std::error::Error + Send>>> + Send>>,
        Box<dyn std::error::Error>,
    >;

    async fn generate_title(
        &self,
        user_message: &str,
    ) -> Result<String, Box<dyn std::error::Error>>;
}
