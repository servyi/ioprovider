use std::collections::VecDeque;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::provider::{Fuzz, IOProvider};

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
    responses: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<LlmRequest>>,
}

impl MockLlm {
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn requests(&self) -> Vec<LlmRequest> {
        self.requests.lock().unwrap().clone()
    }

    pub fn remaining(&self) -> usize {
        self.responses.lock().unwrap().len()
    }
}

#[async_trait]
impl IOProvider<LlmRequest, String> for MockLlm {
    async fn invoke(&self, input: LlmRequest) -> Result<String> {
        self.requests.lock().unwrap().push(input);
        self.responses
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| anyhow!("MockLlm exhausted"))
    }
}

impl Fuzz<LlmRequest, String> for MockLlm {
    /// Generates a plausible LLM response based on the request.
    /// If responses are still queued, returns the next one.
    /// Otherwise generates a response that's plausible for the conversation context.
    fn fuzz(&self, input: &LlmRequest, u: &mut arbitrary::Unstructured) -> String {
        if let Some(resp) = self.responses.lock().unwrap().pop_front() {
            return resp;
        }

        let last_user = input.messages.iter()
            .rev()
            .find(|m| m.role == LlmRole::User)
            .map(|m| m.content.as_str())
            .unwrap_or("");

        let lower = last_user.to_lowercase();
        if lower.contains("reasonable") || lower.contains("verify") {
            if u.arbitrary().unwrap_or(true) {
                "REASONABLE".to_string()
            } else {
                "The verification has issues with formula precision.".to_string()
            }
        } else if lower.contains("smt") || lower.contains("formula") || lower.contains("smtlib") {
            let formulas = [
                "(set-logic ALL)\n(declare-const x Int)\n(assert (> x 0))\n(check-sat)\n",
                "(set-logic ALL)\n(declare-const x Int)\n(assert (and (> x 0) (< x 100)))\n(check-sat)\n",
                "(set-logic ALL)\n(check-sat)\n",
            ];
            let idx = u.int_in_range(0..=formulas.len() - 1).unwrap_or(0);
            format!("```\n{}\n```", formulas[idx])
        } else if lower.contains("split") || lower.contains("piece") {
            "// Start point: test.rs:1\n// Handover point: test.rs:5\nreturn;\n".to_string()
        } else {
            let len = u.int_in_range(0..=200usize).unwrap_or(50);
            let bytes = u.bytes(len).unwrap_or(&[]);
            String::from_utf8_lossy(bytes).to_string()
        }
    }
}
