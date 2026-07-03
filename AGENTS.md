# Agents

## Commands

- Build: `cargo build`
- Test: `cargo test`
- Lint: `cargo clippy -- -W clippy::all`

## Code Quality

- Never bypass compiler warnings with `#[allow(...)]` or similar suppression attributes.
- No code duplication. Use good abstractions and put common code into logically self-contained submodules.
- Tests must never document or assert known buggy behavior. If a test reveals a bug, fix the code rather than encoding the buggy behavior as an expected result.
