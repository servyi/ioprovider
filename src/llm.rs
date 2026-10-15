use std::collections::VecDeque;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
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
/// /// let llm = MockLlm::new(vec!["Hello!".into(), "Goodbye!".into()]);
/// let req = LlmRequest {
///     model: "test".into(),
///     messages: vec![LlmMessage::user("hi")],
/// };
/// assert_eq!(llm.invoke(req.clone()).unwrap(), "Hello!");
/// assert_eq!(llm.invoke(req).unwrap(), "Goodbye!");
/// /// ```
pub struct MockLlm {
    responses: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<LlmRequest>>,
}

// A poisoned lock means some other thread's test already panicked. The
// critical sections only run single atomic collection ops (push / insert /
// pop / clone / len) — nothing panics while holding the lock and the data
// stays structurally valid — so recover it and let the failing test report
// itself instead of raising a confusing secondary panic.
impl MockLlm {
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    /// Returns every request this mock received, in order.
    pub fn requests(&self) -> Vec<LlmRequest> {
        self.requests.lock()
            .unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Returns the number of queued responses not yet consumed.
    pub fn remaining(&self) -> usize {
        self.responses.lock()
            .unwrap_or_else(|e| e.into_inner()).len()
    }
}

/// Prompt topics the mock recognizes, and its canned answers.
const TOPIC_REASONABLE: &str = "reasonable";
const TOPIC_VERIFY: &str = "verify";
const TOPIC_SMT: &str = "smt";
const TOPIC_FORMULA: &str = "formula";
const TOPIC_SMTLIB: &str = "smtlib";
const TOPIC_SPLIT: &str = "split";
const TOPIC_PIECE: &str = "piece";
pub const VERDICT_REASONABLE: &str = "REASONABLE";
const VERDICT_IMPRECISE: &str = "The verification has issues with formula precision.";
/// Canned SMT-LIB formulas of increasing shape.
const SMT_FORMULA_ONE_BOUND: &str =
    "(set-logic ALL)\n(declare-const x Int)\n(assert (> x 0))\n(check-sat)\n";
const SMT_FORMULA_TWO_BOUNDS: &str =
    "(set-logic ALL)\n(declare-const x Int)\n(assert (and (> x 0) (< x 100)))\n(check-sat)\n";
const SMT_FORMULA_BARE_QUERY: &str = "(set-logic ALL)\n(check-sat)\n";
/// Canned split-hint the mock answers for the split topics.
const MOCK_SPLIT_HINT: &str =
    "// Start point: test.rs:1\n// Handover point: test.rs:5\nreturn;\n";
/// Length range of the random fallback fill, and its default.
const RANDOM_FILL_MIN_BYTES: usize = 0;
const RANDOM_FILL_MAX_BYTES: usize = 200;
const DEFAULT_RANDOM_FILL_BYTES: usize = 50;


impl IOProvider<LlmRequest, String> for MockLlm {
    fn invoke(&self, input: LlmRequest) -> Result<String> {
        self.requests
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(input);
        self.responses
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pop_front()
            .ok_or_else(|| anyhow!("MockLlm exhausted"))
    }
}

impl Fuzz<LlmRequest, String> for MockLlm {
    /// Generates a plausible LLM response based on the request.
    /// If responses are still queued, returns the next one.
    /// Otherwise generates a response that's plausible for the conversation context.
    fn fuzz(&self, input: &LlmRequest, u: &mut arbitrary::Unstructured<'_>) -> String {
        if let Some(resp) =
            self.responses.lock()
            .unwrap_or_else(|e| e.into_inner()).pop_front()
        {
            return resp;
        }

        let last_user = input.messages.iter()
            .rev()
            .find(|m| m.role == LlmRole::User)
            .map(|m| m.content.as_str())
            .unwrap_or("");

        let lower = last_user.to_lowercase();

        if lower.contains(TOPIC_REASONABLE) || lower.contains(TOPIC_VERIFY) {
            if u.arbitrary().unwrap_or(true) {
                VERDICT_REASONABLE.to_string()
            } else {
                VERDICT_IMPRECISE.to_string()
            }
        } else if lower.contains(TOPIC_SMT) || lower.contains(TOPIC_FORMULA) || lower.contains(TOPIC_SMTLIB) {
            let formulas = [
                SMT_FORMULA_ONE_BOUND,
                SMT_FORMULA_TWO_BOUNDS,
                SMT_FORMULA_BARE_QUERY,
            ];
            let idx = u.int_in_range(0..=formulas.len() - 1).unwrap_or(0);
            format!("```\n{}\n```", formulas[idx])
        } else if lower.contains(TOPIC_SPLIT) || lower.contains(TOPIC_PIECE) {
            MOCK_SPLIT_HINT.to_string()
        } else {
            let len = u
                .int_in_range(RANDOM_FILL_MIN_BYTES..=RANDOM_FILL_MAX_BYTES)
                .unwrap_or(DEFAULT_RANDOM_FILL_BYTES);
            let bytes = u.bytes(len).unwrap_or(&[]);
            String::from_utf8_lossy(bytes).to_string()
        }
    }
}
