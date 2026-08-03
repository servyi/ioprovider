use servyi_ioprovider::{
    fuzz_pick, Fuzz, FuzzerState, SimpleFuzzerState,
    command::CommandRequest,
    filesystem::{FsRequest, FsResult},
    llm::{LlmMessage, LlmRequest},
    MockCommand, MockFileSystem, MockLlm,
};

#[test]
fn test_fuzz_llm_judge_prompt() {
    let llm = MockLlm::new(vec![]);
    let mut state = SimpleFuzzerState::new(42);

    let req = LlmRequest {
        model: "test".into(),
        messages: vec![LlmMessage::user("Determine if the verification is REASONABLE")],
    };

    // Run many fuzz iterations — should produce REASONABLE sometimes
    let mut reasonable_count = 0;
    for _ in 0..100 {
        let resp = llm.fuzz(&req, &mut state);
        if resp == "REASONABLE" {
            reasonable_count += 1;
        }
    }
    assert!(reasonable_count > 0, "should produce REASONABLE at least sometimes for judge prompts");
    assert!(reasonable_count < 100, "should not always produce REASONABLE");
}

#[test]
fn test_fuzz_llm_smt_prompt() {
    let llm = MockLlm::new(vec![]);
    let mut state = SimpleFuzzerState::new(99);

    let req = LlmRequest {
        model: "test".into(),
        messages: vec![LlmMessage::user("Produce SMT-LIB2 formulas in smt2 blocks")],
    };

    let resp = llm.fuzz(&req, &mut state);
    assert!(resp.contains("(check-sat)") || resp.contains("set-logic"), "formalizer prompt should produce SMT-like output, got: {resp}");
}

#[test]
fn test_fuzz_solver_sat_unsat() {
    let cmd = MockCommand::new();
    let mut state = SimpleFuzzerState::new(7);

    let req = CommandRequest {
        program: "z3".into(),
        args: vec![],
        stdin: Some("(check-sat)".into()),
        working_dir: None,
    };

    let mut outcomes = std::collections::HashSet::new();
    for _ in 0..50 {
        let result = cmd.fuzz(&req, &mut state);
        assert_eq!(result.exit_code, 0, "z3 should succeed");
        let out = result.stdout.trim();
        assert!(
            out == "unsat" || out == "sat" || out == "unknown",
            "z3 should return sat/unsat/unknown, got: {out}"
        );
        outcomes.insert(out.to_string());
    }
    assert!(outcomes.len() >= 2, "should produce at least 2 different outcomes over 50 runs, got: {outcomes:?}");
}

#[test]
fn test_fuzz_python() {
    let cmd = MockCommand::new();
    let mut state = SimpleFuzzerState::new(123);

    let req = CommandRequest {
        program: "python3".into(),
        args: vec![],
        stdin: None,
        working_dir: None,
    };

    for _ in 0..20 {
        let result = cmd.fuzz(&req, &mut state);
        // Python should mostly succeed
        assert!(result.exit_code == 0 || result.exit_code == 1);
    }
}

#[test]
fn test_fuzz_filesystem_read() {
    let fs = MockFileSystem::new();
    let mut state = SimpleFuzzerState::new(55);

    let req = FsRequest::Read {
        path: std::path::PathBuf::from("/test/file.rs"),
    };

    for _ in 0..10 {
        let result = fs.fuzz(&req, &mut state);
        match result {
            FsResult::Content(_) => {}
            other => panic!("Read should produce Content, got: {other:?}"),
        }
    }
}

#[test]
fn test_fuzz_filesystem_write_always_succeeds() {
    let fs = MockFileSystem::new();
    let mut state = SimpleFuzzerState::new(1);

    let req = FsRequest::Write {
        path: std::path::PathBuf::from("/test/out.txt"),
        content: "hello".into(),
    };

    for _ in 0..10 {
        assert_eq!(fs.fuzz(&req, &mut state), FsResult::Written);
    }
}

#[test]
fn test_fuzzer_state_deterministic() {
    let mut a = SimpleFuzzerState::new(42);
    let mut b = SimpleFuzzerState::new(42);
    for _ in 0..100 {
        assert_eq!(a.gen_bool(), b.gen_bool());
        assert_eq!(a.gen_range(0, 1000), b.gen_range(0, 1000));
    }
}

#[test]
fn test_fuzz_pick() {
    let mut state = SimpleFuzzerState::new(42);
    let items = vec!["a", "b", "c", "d", "e"];
    let mut seen = std::collections::HashSet::new();
    for _ in 0..100 {
        let picked = fuzz_pick(&mut state, &items);
        seen.insert(*picked);
    }
    assert!(seen.len() >= 3, "should pick at least 3 different items over 100 runs");
}
