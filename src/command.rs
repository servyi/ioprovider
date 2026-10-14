use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::provider::{Fuzz, IOProvider};

/// A command execution request (e.g., running a solver, a script, or any CLI tool).
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone)]
pub struct CommandRequest {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: Option<String>,
    pub working_dir: Option<std::path::PathBuf>,
}

/// Exit code of a successful command (the OS convention).
pub const EXIT_SUCCESS: i32 = 0;
/// Exit code the mocks use for a simulated failure.
const EXIT_FAILURE: i32 = 1;

/// The SMT `check-sat` query the mock detects in solver stdin.
pub const SMT_SAT_QUERY: &str = "(check-sat)";
/// Simulated solver answers, in SMT-LIB spelling.
pub const SMT_RESP_UNSAT: &str = "unsat\n";
pub const SMT_RESP_SAT: &str = "sat\n";
pub const SMT_RESP_UNKNOWN: &str = "unknown\n";
/// A bare query echoed back, as the solver mock's "output".
const SMT_RESP_ECHO_QUERY: &str = "(check-sat)\n";
/// Generic acknowledgment the command mock prints on success.
const COMMAND_ACK: &str = "ok\n";
/// Simulated interpreter/mock failure messages.
const PY_MOCK_ERROR: &str = "SyntaxError";
const GENERIC_MOCK_ERROR: &str = "error";
/// Fuzz draw range: a single decimal digit.
const FUZZ_DIGIT_LOW: u8 = 0;
const FUZZ_DIGIT_HIGH: u8 = 9;
/// Fallback draw when the fuzz engine provides nothing.
const FALLBACK_FUZZ_DRAW: u8 = 0;
/// Fallback sat-branch when the fuzz engine provides nothing
/// (middle branch: `unknown`).
const FALLBACK_SAT_BRANCH: u8 = 5;
/// Simulated success rates, in tenths: the python mock succeeds 8 in
/// 10, the generic mock 7 in 10.
const PY_MOCK_SUCCESS_RATE: u8 = 8;
const GENERIC_MOCK_SUCCESS_RATE: u8 = 7;


/// Result of executing a command.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandResult {
    pub fn success(stdout: &str) -> Self {
        Self { stdout: stdout.to_string(), stderr: String::new(), exit_code: EXIT_SUCCESS }
    }

    pub fn failure(stderr: &str, exit_code: i32) -> Self {
        Self { stdout: String::new(), stderr: stderr.to_string(), exit_code }
    }
}

/// Mock command executor with pre-configured responses.
///
/// Responses can be configured by matching the program name, or as a sequence.
///
/// ```
/// use servyi_ioprovider::{MockCommand, IOProvider, command::{CommandRequest, CommandResult}};
///
/// # tokio_test::block_on(async {
/// let mut mock = MockCommand::new();
/// mock.on_program("z3", CommandResult::success("unsat\n"));
///
/// let req = CommandRequest { program: "z3".into(), args: vec![], stdin: None, working_dir: None };
/// let result = mock.invoke(req).await.unwrap();
/// assert_eq!(result.stdout, "unsat\n");
/// # });
/// ```
pub struct MockCommand {
    by_program: Arc<Mutex<HashMap<String, VecDeque<CommandResult>>>>,
    inputs: Arc<Mutex<Vec<CommandRequest>>>,
}

// A poisoned lock means some other thread's test already panicked. The
// critical sections only run single atomic collection ops (push / insert /
// pop / clone / len) — nothing panics while holding the lock and the data
// stays structurally valid — so recover it and let the failing test report
// itself instead of raising a confusing secondary panic.
impl MockCommand {
    pub fn new() -> Self {
        Self {
            by_program: Arc::new(Mutex::new(HashMap::new())),
            inputs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Configure a response for a given program name.
    /// Multiple calls queue responses in sequence for that program.
    pub fn on_program(&mut self, program: &str, result: CommandResult) {
        self.by_program
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(program.to_string())
            .or_default()
            .push_back(result);
    }

    /// Returns every request this mock received, in order.
    pub fn requests(&self) -> Vec<CommandRequest> {
        self.inputs.lock()
            .unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Default for MockCommand {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl IOProvider<CommandRequest, CommandResult> for MockCommand {
    async fn invoke(&self, input: CommandRequest) -> Result<CommandResult> {
        let program = input.program.clone();
        self.inputs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(input);
        let mut map = self.by_program.lock()
            .unwrap_or_else(|e| e.into_inner());
        match map.get_mut(&program) {
            Some(queue) if !queue.is_empty() => {
                Ok(queue.pop_front().expect("queue checked non-empty above"))
            }
            _ => Err(anyhow!("MockCommand: no response configured for '{program}'")),
        }
    }
}

impl Fuzz<CommandRequest, CommandResult> for MockCommand {
    fn fuzz(&self, input: &CommandRequest, u: &mut arbitrary::Unstructured<'_>) -> CommandResult {
        match input.program.as_str() {
            "z3" | "cvc5" | "yices" => {
                let formula = input.stdin.as_deref().unwrap_or("");
                if formula.contains(SMT_SAT_QUERY) {
                    match u.int_in_range(FUZZ_DIGIT_LOW..=FUZZ_DIGIT_HIGH).unwrap_or(FALLBACK_SAT_BRANCH) {
                        0..=3 => CommandResult::success(SMT_RESP_UNSAT),
                        4..=6 => CommandResult::success(SMT_RESP_SAT),
                        _ => CommandResult::success(SMT_RESP_UNKNOWN),
                    }
                } else {
                    CommandResult::success(COMMAND_ACK)
                }
            }
            "python3" | "python" => {
                if u.int_in_range(FUZZ_DIGIT_LOW..=FUZZ_DIGIT_HIGH).unwrap_or(FALLBACK_FUZZ_DRAW) < PY_MOCK_SUCCESS_RATE {
                    CommandResult::success(SMT_RESP_ECHO_QUERY)
                } else {
                    CommandResult::failure(PY_MOCK_ERROR, EXIT_FAILURE)
                }
            }
            _ => {
                if u.int_in_range(FUZZ_DIGIT_LOW..=FUZZ_DIGIT_HIGH).unwrap_or(FALLBACK_FUZZ_DRAW) < GENERIC_MOCK_SUCCESS_RATE {
                    CommandResult::success(COMMAND_ACK)
                } else {
                    CommandResult::failure(GENERIC_MOCK_ERROR, EXIT_FAILURE)
                }
            }
        }
    }
}
