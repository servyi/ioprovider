use std::marker::PhantomData;
use std::sync::{Arc, Mutex};

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

/// Owned fuzz byte buffer with a live read cursor.
///
/// Holds the raw bytes and tracks how many have been consumed. An
/// [`Unstructured`] view is reconstructed over the unconsumed tail on each
/// draw (via [`draw`](Self::draw)), so this is a borrow-checker-friendly,
/// self-contained alternative to a self-referential `Unstructured` field.
///
/// Designed to be shared across several [`FuzzProvider`]s via an
/// `Arc<Mutex<FuzzData>>`: every `invoke` consumes bytes from the same
/// stream, so one `cargo-fuzz` input deterministically drives all providers.
pub struct FuzzData {
    data: Vec<u8>,
    pos: usize,
}

impl FuzzData {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, pos: 0 }
    }

    /// Generate `len` deterministic pseudo-random bytes from `seed`
    /// using a SplitMix64-style PRNG (no external dependency).
    pub fn from_seed(seed: u64, len: usize) -> Self {
        let mut state = seed;
        let data = (0..len)
            .map(|_| {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (state >> 33) as u8
            })
            .collect();
        Self { data, pos: 0 }
    }

    /// Number of bytes consumed so far by all draws on this buffer.
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Number of unconsumed bytes remaining.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Rewind the cursor to the start (bytes are retained).
    pub fn reset(&mut self) {
        self.pos = 0;
    }

    /// Run `f` against an [`Unstructured`] view of the unconsumed tail,
    /// advancing the cursor by exactly the number of bytes `f` consumes.
    pub fn draw<O>(&mut self, f: impl FnOnce(&mut Unstructured) -> O) -> O {
        let before = self.remaining();
        let (out, consumed) = {
            let mut u = Unstructured::new(&self.data[self.pos..]);
            let out = f(&mut u);
            (out, before - u.len())
        };
        self.pos += consumed;
        out
    }
}

/// Build a shared fuzz stream from `seed`, ready to be cloned into multiple
/// [`FuzzProvider`]s via [`FuzzProvider::with_stream`].
pub fn fuzz_stream_from_seed(seed: u64) -> Arc<Mutex<FuzzData>> {
    Arc::new(Mutex::new(FuzzData::from_seed(seed, 4096)))
}

/// Build a shared fuzz stream from an owned byte buffer. This is the entry
/// point for a `cargo-fuzz` harness, which passes its `&[u8]` input here:
///
/// ```ignore
/// fuzz_target!(|data: &[u8]| {
///     let stream = fuzz_stream_from_bytes(data.to_vec());
///     let llm = FuzzProvider::with_stream(MockLlm::new(vec![]), stream.clone());
///     // ...drive the state machine...
/// });
/// ```
pub fn fuzz_stream_from_bytes(bytes: Vec<u8>) -> Arc<Mutex<FuzzData>> {
    Arc::new(Mutex::new(FuzzData::new(bytes)))
}

/// Drop-in fuzzing wrapper: implements `IOProvider<I, O>` by calling
/// the inner `Fuzz<I, O>` implementation.
///
/// Multiple providers can share a single fuzz stream by cloning the same
/// `Arc`. Every `invoke` consumes bytes from the shared stream, so one
/// `cargo-fuzz` input deterministically drives all providers.
///
/// ```
/// use std::sync::Arc;
/// use servyi_ioprovider::{
///     fuzz_stream_from_seed, FuzzProvider, IOProvider, MockLlm,
///     llm::{LlmRequest, LlmMessage},
/// };
///
/// # tokio_test::block_on(async {
/// // Several providers can share one fuzz stream by cloning the Arc:
/// let stream = fuzz_stream_from_seed(42);
/// let llm = FuzzProvider::with_stream(MockLlm::new(vec![]), Arc::clone(&stream));
///
/// let req = LlmRequest {
///     model: "test".into(),
///     messages: vec![LlmMessage::user("Is this REASONABLE?")],
/// };
/// // provider implements IOProvider<LlmRequest, String> — drop-in replacement
/// let response = llm.invoke(req).await.unwrap();
/// assert!(!response.is_empty());
/// # });
/// ```
pub struct FuzzProvider<F, I, O> {
    fuzz: F,
    stream: Arc<Mutex<FuzzData>>,
    _phantom: PhantomData<fn(I) -> O>,
}

impl<F, I, O> FuzzProvider<F, I, O>
where
    F: Fuzz<I, O>,
{
    /// Create a provider backed by a freshly time-seeded stream.
    pub fn new(fuzz: F) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        Self::with_seed(fuzz, seed)
    }

    /// Create a provider backed by a deterministic stream derived from `seed`.
    pub fn with_seed(fuzz: F, seed: u64) -> Self {
        Self::with_stream(fuzz, fuzz_stream_from_seed(seed))
    }

    /// Create a provider that draws from a shared `stream`.
    /// All providers built from clones of the same `Arc` consume bytes
    /// from the same underlying stream.
    pub fn with_stream(fuzz: F, stream: Arc<Mutex<FuzzData>>) -> Self {
        Self { fuzz, stream, _phantom: PhantomData }
    }

    /// Access the shared fuzz stream (e.g. to hand it to another provider).
    pub fn stream(&self) -> Arc<Mutex<FuzzData>> {
        Arc::clone(&self.stream)
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
        let mut guard = self.stream.lock().unwrap();
        Ok(guard.draw(|u| self.fuzz.fuzz(&input, u)))
    }
}
