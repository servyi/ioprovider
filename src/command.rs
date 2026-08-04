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
        Self { stdout: stdout.to_string(), stderr: String::new(), exit_code: 0 }
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
            .unwrap()
            .entry(program.to_string())
            .or_default()
            .push_back(result);
    }

    pub fn requests(&self) -> Vec<CommandRequest> {
        self.inputs.lock().unwrap().clone()
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
        self.inputs.lock().unwrap().push(input);
        let mut map = self.by_program.lock().unwrap();
        match map.get_mut(&program) {
            Some(queue) if !queue.is_empty() => Ok(queue.pop_front().unwrap()),
            _ => Err(anyhow!("MockCommand: no response configured for '{program}'")),
        }
    }
}

impl Fuzz<CommandRequest, CommandResult> for MockCommand {
    fn fuzz(&self, input: &CommandRequest, u: &mut arbitrary::Unstructured) -> CommandResult {
        match input.program.as_str() {
            "z3" | "cvc5" | "yices" => {
                let formula = input.stdin.as_deref().unwrap_or("");
                if formula.contains("(check-sat)") {
                    match u.int_in_range(0u8..=9).unwrap_or(5) {
                        0..=3 => CommandResult::success("unsat\n"),
                        4..=6 => CommandResult::success("sat\n"),
                        _ => CommandResult::success("unknown\n"),
                    }
                } else {
                    CommandResult::success("ok\n")
                }
            }
            "python3" | "python" => {
                if u.int_in_range(0u8..=9).unwrap_or(0) < 8 {
                    CommandResult::success("(check-sat)\n")
                } else {
                    CommandResult::failure("SyntaxError", 1)
                }
            }
            _ => {
                if u.int_in_range(0u8..=9).unwrap_or(0) < 7 {
                    CommandResult::success("ok\n")
                } else {
                    CommandResult::failure("error", 1)
                }
            }
        }
    }
}
