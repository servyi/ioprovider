
use anyhow::Result;
use async_trait::async_trait;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::mock::SequenceMock;
use crate::provider::IOProvider;

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum LlmRole {
    System,
    User,
    Assistant,
}

impl std::fmt::Display for LlmRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmRole::System => write!(f, "system"),
            LlmRole::User => write!(f, "user"),
            LlmRole::Assistant => write!(f, "assistant"),
        }
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone)]
pub struct LlmMessage {
    pub role: LlmRole,
    pub content: String,
}

impl LlmMessage {
    pub fn system(content: &str) -> Self {
        Self { role: LlmRole::System, content: content.to_string() }
    }
    pub fn user(content: &str) -> Self {
        Self { role: LlmRole::User, content: content.to_string() }
    }
    pub fn assistant(content: &str) -> Self {
        Self { role: LlmRole::Assistant, content: content.to_string() }
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub model: String,
    pub messages: Vec<LlmMessage>,
}

/// Mock LLM that returns pre-configured responses in sequence.
///
/// Each call to `invoke` returns the next response, regardless of the input.
///
/// ```
/// use servyi_ioprovider::{MockLlm, IOProvider, llm::{LlmRequest, LlmMessage}};
///
/// # tokio_test::block_on(async {
/// let llm = MockLlm::new(vec!["Hello!".into(), "Goodbye!".into()]);
/// let req = LlmRequest {
///     model: "test".into(),
///     messages: vec![LlmMessage::user("hi")],
/// };
/// assert_eq!(llm.invoke(req.clone()).await.unwrap(), "Hello!");
/// assert_eq!(llm.invoke(req).await.unwrap(), "Goodbye!");
/// # });
/// ```
pub struct MockLlm {
    inner: SequenceMock<LlmRequest, String>,
}

impl MockLlm {
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            inner: SequenceMock::new(responses),
        }
    }

    pub fn requests(&self) -> Vec<LlmRequest> {
        self.inner.inputs()
    }

    pub fn remaining(&self) -> usize {
        self.inner.remaining()
    }
}

#[async_trait]
impl IOProvider<LlmRequest, String> for MockLlm {
    async fn invoke(&self, input: LlmRequest) -> Result<String> {
        self.inner.invoke(input).await
    }
}
