//! Integration tests exercise the mocks directly (sync invoke).
//! A panic here IS the test failure signal, so `unwrap` is acceptable in
//! this file only (see servyi-lints.toml).
#![cfg_attr(test, allow(clippy::unwrap_used))]

use servyi_ioprovider::{
    Fuzz, IOProvider, FuzzProvider,
    command::{
        CommandRequest, EXIT_SUCCESS as EXIT_SUCCESS_CODE, SMT_RESP_SAT, SMT_RESP_UNKNOWN,
        SMT_RESP_UNSAT, SMT_SAT_QUERY,
    },
    filesystem::{FsRequest, FsResult},
    llm::{LlmMessage, LlmRequest, VERDICT_REASONABLE},
    MockCommand, MockFileSystem, MockLlm,
};

/// Size of the unstructured buffer handed to every fuzz draw.
const FUZZ_BUFFER_BYTES: usize = 1024;
/// Fill bytes steering the arbitrary draws (0 and 5 pick different
/// mock branches; 42 just makes the stream distinct).
const LOW_ENTROPY_FILL: u8 = 0;
const MIXED_ENTROPY_FILL: u8 = 5;
const DISTINCT_FILL: u8 = 42;
/// The model name the mock ignores.
const TEST_MODEL_NAME: &str = "test";
/// Solver program under test.
const TEST_SOLVER_PROGRAM: &str = "z3";
/// Prompt scenarios steering the mock LLM.
const JUDGE_PROMPT_TEXT: &str = "Determine if the verification is REASONABLE";
const FORMALIZER_PROMPT_TEXT: &str = "Produce SMT-LIB2 formulas in smt2 blocks";
const SANITY_PROMPT_TEXT: &str = "Is this REASONABLE?";
/// Output markers the assertions look for.
const FEEDBACK_TOPIC: &str = "verification";
const SMT_LOGIC_KEYWORD: &str = "set-logic";
/// Sample file content and paths for the filesystem mocks.
const WRITTEN_SAMPLE_CONTENT: &str = "hello";
const READ_SAMPLE_PATH_TXT: &str = "/test/file.rs";
const WRITE_SAMPLE_PATH_TXT: &str = "/test/out.txt";
/// Seed sweep and the deterministic stream mixing.
const FIRST_SEED: u64 = 0;
const SEED_COUNT: u64 = 50;
const SEED_STRIDE: u64 = 257;
const LOW_BYTE_MASK: u64 = 0xFF;
/// Distinct seeds for the drop-in and shared-stream tests.
const DROP_IN_TEST_SEED: u64 = 42;
const SHARED_STREAM_SEED: u64 = 99;
/// Distinct mock answers the seed sweep must produce at minimum.
const MIN_DISTINCT_OUTCOMES: usize = 2;


#[test]
fn test_fuzz_llm_judge_prompt() {
    let llm = MockLlm::new(vec![]);
    let data = vec![LOW_ENTROPY_FILL; FUZZ_BUFFER_BYTES];
    let mut u = arbitrary::Unstructured::new(&data);

    let req = LlmRequest {
        model: TEST_MODEL_NAME.into(),
        messages: vec![LlmMessage::user(JUDGE_PROMPT_TEXT)],
    };

    let resp = llm.fuzz(&req, &mut u);
    assert!(
        resp == VERDICT_REASONABLE || resp.contains(FEEDBACK_TOPIC),
        "judge prompt should produce REASONABLE or verification feedback"
    );
}

#[test]
fn test_fuzz_llm_smt_prompt() {
    let llm = MockLlm::new(vec![]);
    let data = vec![MIXED_ENTROPY_FILL; FUZZ_BUFFER_BYTES];
    let mut u = arbitrary::Unstructured::new(&data);

    let req = LlmRequest {
        model: TEST_MODEL_NAME.into(),
        messages: vec![LlmMessage::user(FORMALIZER_PROMPT_TEXT)],
    };

    let resp = llm.fuzz(&req, &mut u);
    assert!(
        resp.contains(SMT_SAT_QUERY) || resp.contains(SMT_LOGIC_KEYWORD),
        "formalizer prompt should produce SMT-like output"
    );
}

#[test]
fn test_fuzz_solver_outcomes() {
    let cmd = MockCommand::new();

    let req = CommandRequest {
        program: TEST_SOLVER_PROGRAM.into(),
        args: vec![],
        stdin: Some(SMT_SAT_QUERY.into()),
        working_dir: None,
    };

    let mut outcomes = std::collections::HashSet::new();
    for seed in FIRST_SEED..SEED_COUNT {
        let data = (FIRST_SEED..FUZZ_BUFFER_BYTES as u64)
            .map(|i| ((seed * SEED_STRIDE + i) & LOW_BYTE_MASK) as u8)
            .collect::<Vec<_>>();
        let mut u = arbitrary::Unstructured::new(&data);
        let result = cmd.fuzz(&req, &mut u);
        assert_eq!(result.exit_code, EXIT_SUCCESS_CODE, "z3 should succeed");
        let out = result.stdout.trim();
        assert!(
            out == SMT_RESP_UNSAT.trim()
                || out == SMT_RESP_SAT.trim()
                || out == SMT_RESP_UNKNOWN.trim(),
            "z3 should return sat/unsat/unknown, got: {out}"
        );
        let _new = outcomes.insert(out.to_string());
    }
    assert!(outcomes.len() >= MIN_DISTINCT_OUTCOMES, "should produce multiple outcomes");
}

#[test]
fn test_fuzz_filesystem_read() {
    let fs = MockFileSystem::new();
    let data = vec![DISTINCT_FILL; FUZZ_BUFFER_BYTES];
    let mut u = arbitrary::Unstructured::new(&data);

    let req = FsRequest::Read {
        path: std::path::PathBuf::from(READ_SAMPLE_PATH_TXT),
    };

    let result = fs.fuzz(&req, &mut u);
    assert!(matches!(result, FsResult::Content(_)));
}

#[test]
fn test_fuzz_filesystem_write() {
    let fs = MockFileSystem::new();
    let data = vec![LOW_ENTROPY_FILL; FUZZ_BUFFER_BYTES];
    let mut u = arbitrary::Unstructured::new(&data);

    let req = FsRequest::Write {
        path: std::path::PathBuf::from(WRITE_SAMPLE_PATH_TXT),
        content: WRITTEN_SAMPLE_CONTENT.into(),
    };

    assert_eq!(fs.fuzz(&req, &mut u), FsResult::Written);
}

#[test]
fn test_fuzz_provider_drop_in() {
    let provider = FuzzProvider::with_seed(MockLlm::new(vec![]), DROP_IN_TEST_SEED);

    let req = LlmRequest {
        model: TEST_MODEL_NAME.into(),
        messages: vec![LlmMessage::user(SANITY_PROMPT_TEXT)],
    };

    let response = (provider.invoke(req)).unwrap();
    assert!(!response.is_empty());
}

#[test]
fn test_stream_cursor_advances() {
    use servyi_ioprovider::FuzzData;
    use std::sync::Arc;


    let stream: Arc<std::sync::Mutex<FuzzData>> = servyi_ioprovider::fuzz_stream_from_seed(SHARED_STREAM_SEED);
    let llm = FuzzProvider::with_stream(MockLlm::new(vec![]), Arc::clone(&stream));
    let cmd = FuzzProvider::with_stream(MockCommand::new(), Arc::clone(&stream));

    let req = LlmRequest {
        model: TEST_MODEL_NAME.into(),
        messages: vec![LlmMessage::user(FORMALIZER_PROMPT_TEXT)],
    };
    let _response = (llm.invoke(req)).unwrap();
    let pos_after_llm = stream.lock().unwrap().pos();
    assert!(pos_after_llm > FIRST_SEED as usize, "cursor must advance after an invoke");

    let creq = CommandRequest {
        program: TEST_SOLVER_PROGRAM.into(),
        args: vec![],
        stdin: Some(SMT_SAT_QUERY.into()),
        working_dir: None,
    };
    let _result = (cmd.invoke(creq)).unwrap();
    let pos_after_cmd = stream.lock().unwrap().pos();
    assert!(
        pos_after_cmd > pos_after_llm,
        "second provider must consume from the same shared stream"
    );
}

