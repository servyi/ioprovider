use std::marker::PhantomData;

use anyhow::Result;
use arbitrary::Unstructured;
use async_trait::async_trait;

/// The core trait: a provider that takes input `I` and produces output `O`.
///
/// All providers are `Send + Sync` so they can be shared across threads.
/// The `invoke` method is async to support network/subprocess-backed providers.
#[async_trait]
pub trait IOProvider<I, O>: Send + Sync {
    async fn invoke(&self, input: I) -> Result<O>;
}

/// Trait for generating realistic fake outputs for fuzzing state machines.
///
/// Implement this alongside `IOProvider<I, O>` to enable automatic fuzzing
/// of state machines without real I/O. The `fuzz` method receives the input
/// that would have been passed to `invoke`, so it can generate contextually
/// appropriate outputs.
///
/// Uses `arbitrary::Unstructured` (same backend as `cargo-fuzz`) as the
/// random data source.
pub trait Fuzz<I, O>: Send + Sync {
    fn fuzz(&self, input: &I, u: &mut Unstructured) -> O;
}

/// Drop-in fuzzing wrapper: implements `IOProvider<I, O>` by calling
/// the inner `Fuzz<I, O>` implementation.
///
/// ```
/// use servyi_ioprovider::{FuzzProvider, IOProvider, MockLlm, llm::{LlmRequest, LlmMessage}};
///
/// # tokio_test::block_on(async {
/// let provider = FuzzProvider::with_seed(MockLlm::new(vec![]), 42);
/// let req = LlmRequest {
///     model: "test".into(),
///     messages: vec![LlmMessage::user("Is this REASONABLE?")],
/// };
/// // provider implements IOProvider<LlmRequest, String> — drop-in replacement
/// let response = provider.invoke(req).await.unwrap();
/// assert!(!response.is_empty());
/// # });
/// ```
pub struct FuzzProvider<F, I, O> {
    fuzz: F,
    data: std::sync::Mutex<Vec<u8>>,
    _phantom: PhantomData<fn(I) -> O>,
}

impl<F, I, O> FuzzProvider<F, I, O>
where
    F: Fuzz<I, O>,
{
    pub fn new(fuzz: F) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let data = (0..1024).map(|i| ((seed + i) & 0xFF) as u8).collect();
        Self { fuzz, data: std::sync::Mutex::new(data), _phantom: PhantomData }
    }

    pub fn with_seed(fuzz: F, seed: u64) -> Self {
        let data = (0..1024).map(|i| ((seed.wrapping_mul(6364136223846793005).wrapping_add(i as u64)) >> 33) as u8).collect();
        Self { fuzz, data: std::sync::Mutex::new(data), _phantom: PhantomData }
    }
}

#[async_trait]
impl<F, I, O> IOProvider<I, O> for FuzzProvider<F, I, O>
where
    F: Fuzz<I, O>,
    I: Send + Sync + 'static,
    O: Send + 'static,
{
    async fn invoke(&self, input: I) -> Result<O> {
        let data = self.data.lock().unwrap().clone();
        let mut u = Unstructured::new(&data);
        Ok(self.fuzz.fuzz(&input, &mut u))
    }
}
