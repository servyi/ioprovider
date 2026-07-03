use anyhow::Result;
use async_trait::async_trait;

/// The core trait: a provider that takes input `I` and produces output `O`.
///
/// All providers are `Send + Sync` so they can be shared across threads.
/// The `invoke` method is async to support network/subprocess-backed providers.
#[async_trait]
pub trait IOProvider<I, O>: Send + Sync {
    async fn invoke(&self, input: I) -> Result<O>;
}
