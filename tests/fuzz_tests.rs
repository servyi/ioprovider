//! Integration tests exercise the mocks through a real async runtime.
//! A panic here IS the test failure signal, so `unwrap` is acceptable in
//! this file only (see servyi-lints.toml).
#![cfg_attr(test, allow(clippy::unwrap_used))]

use servyi_ioprovider::{
    Fuzz, IOProvider, FuzzProvider,
    command::CommandRequest,
    filesystem::{FsRequest, FsResult},
    llm::{LlmMessage, LlmRequest},
    MockCommand, MockFileSystem, MockLlm,
};

#[test]
fn test_fuzz_llm_judge_prompt() {
    let llm = MockLlm::new(vec![]);
    let data = vec![0u8; 1024];
    let mut u = arbitrary::Unstructured::new(&data);

    let req = LlmRequest {
        model: "test".into(),
        messages: vec![LlmMessage::user("Determine if the verification is REASONABLE")],
    };

    let resp = llm.fuzz(&req, &mut u);
    assert!(
        resp == "REASONABLE" || resp.contains("verification"),
        "judge prompt should produce REASONABLE or verification feedback"
    );
}

#[test]
fn test_fuzz_llm_smt_prompt() {
    let llm = MockLlm::new(vec![]);
    let data = vec![5u8; 1024];
    let mut u = arbitrary::Unstructured::new(&data);

    let req = LlmRequest {
        model: "test".into(),
        messages: vec![LlmMessage::user("Produce SMT-LIB2 formulas in smt2 blocks")],
    };

    let resp = llm.fuzz(&req, &mut u);
    assert!(
        resp.contains("(check-sat)") || resp.contains("set-logic"),
        "formalizer prompt should produce SMT-like output"
    );
}

#[test]
fn test_fuzz_solver_outcomes() {
    let cmd = MockCommand::new();

    let req = CommandRequest {
        program: "z3".into(),
        args: vec![],
        stdin: Some("(check-sat)".into()),
        working_dir: None,
    };

    let mut outcomes = std::collections::HashSet::new();
    for seed in 0..50 {
        let data = (0..1024).map(|i| ((seed * 257 + i) & 0xFF) as u8).collect::<Vec<_>>();
        let mut u = arbitrary::Unstructured::new(&data);
        let result = cmd.fuzz(&req, &mut u);
        assert_eq!(result.exit_code, 0, "z3 should succeed");
        let out = result.stdout.trim();
        assert!(
            out == "unsat" || out == "sat" || out == "unknown",
            "z3 should return sat/unsat/unknown, got: {out}"
        );
        let _new = outcomes.insert(out.to_string());
    }
    assert!(outcomes.len() >= 2, "should produce multiple outcomes");
}

#[test]
fn test_fuzz_filesystem_read() {
    let fs = MockFileSystem::new();
    let data = vec![42u8; 1024];
    let mut u = arbitrary::Unstructured::new(&data);

    let req = FsRequest::Read {
        path: std::path::PathBuf::from("/test/file.rs"),
    };

    let result = fs.fuzz(&req, &mut u);
    assert!(matches!(result, FsResult::Content(_)));
}

#[test]
fn test_fuzz_filesystem_write() {
    let fs = MockFileSystem::new();
    let data = vec![0u8; 1024];
    let mut u = arbitrary::Unstructured::new(&data);

    let req = FsRequest::Write {
        path: std::path::PathBuf::from("/test/out.txt"),
        content: "hello".into(),
    };

    assert_eq!(fs.fuzz(&req, &mut u), FsResult::Written);
}

#[test]
fn test_fuzz_provider_drop_in() {
    let provider = FuzzProvider::with_seed(MockLlm::new(vec![]), 42);

    let req = LlmRequest {
        model: "test".into(),
        messages: vec![LlmMessage::user("Is this REASONABLE?")],
    };

    let rt = tokio::runtime::Runtime::new().unwrap();
    let response = rt.block_on(provider.invoke(req)).unwrap();
    assert!(!response.is_empty());
}

#[test]
fn test_stream_cursor_advances() {
    use servyi_ioprovider::FuzzData;
    use std::sync::Arc;

    let rt = tokio::runtime::Runtime::new().unwrap();

    let stream: Arc<std::sync::Mutex<FuzzData>> = servyi_ioprovider::fuzz_stream_from_seed(99);
    let llm = FuzzProvider::with_stream(MockLlm::new(vec![]), Arc::clone(&stream));
    let cmd = FuzzProvider::with_stream(MockCommand::new(), Arc::clone(&stream));

    let req = LlmRequest {
        model: "test".into(),
        messages: vec![LlmMessage::user("Produce SMT-LIB2 formulas in smt2 blocks")],
    };
    let _response = rt.block_on(llm.invoke(req)).unwrap();
    let pos_after_llm = stream.lock().unwrap().pos();
    assert!(pos_after_llm > 0, "cursor must advance after an invoke");

    let creq = CommandRequest {
        program: "z3".into(),
        args: vec![],
        stdin: Some("(check-sat)".into()),
        working_dir: None,
    };
    let _result = rt.block_on(cmd.invoke(creq)).unwrap();
    let pos_after_cmd = stream.lock().unwrap().pos();
    assert!(
        pos_after_cmd > pos_after_llm,
        "second provider must consume from the same shared stream"
    );
}

