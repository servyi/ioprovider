# servyi-ioprovider

Generic async IOProvider trait and mock implementations for testing.

## Core Trait

```rust
#[async_trait]
pub trait IOProvider<I, O>: Send + Sync {
    async fn invoke(&self, input: I) -> Result<O>;
}
```

A provider takes input `I` and produces output `O` asynchronously. All providers are `Send + Sync`.

## Domain Types

### LLM

- `LlmRequest { model, messages }` — chat completion request
- `LlmMessage { role, content }` — System / User / Assistant roles
- `MockLlm` — returns pre-configured responses in sequence, records all requests

### Command

- `CommandRequest { program, args, stdin, working_dir }` — subprocess execution request
- `CommandResult { stdout, stderr, exit_code }` — execution result
- `MockCommand` — matches by program name, supports queued responses per program

### File System

- `FsRequest` — enum: Read, Write, Exists, Remove, ListDir
- `FsResult` — enum: Content, Written, Exists, Removed, Entries
- `MockFileSystem` — in-memory HashMap-backed, supports all operations

## Usage

```toml
[dependencies]
servyi-ioprovider = { git = "https://github.com/servyi/ioprovider.git" }
```

```rust
use servyi_ioprovider::{IOProvider, MockLlm, llm::{LlmRequest, LlmMessage}};

let llm = MockLlm::new(vec!["Hello!".into(), "Goodbye!".into()]);
let req = LlmRequest {
    model: "test".into(),
    messages: vec![LlmMessage::user("hi")],
};
assert_eq!(llm.invoke(req.clone()).await.unwrap(), "Hello!");
```
