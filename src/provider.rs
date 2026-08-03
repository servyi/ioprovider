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

/// Random data source for fuzzing.
///
/// Provides primitives that fuzz implementations use to generate
/// realistic-looking outputs without external interaction.
pub trait FuzzerState {
    fn gen_bool(&mut self) -> bool;
    fn gen_u8(&mut self) -> u8;
    fn gen_range(&mut self, min: usize, max: usize) -> usize;
    fn gen_string(&mut self, max_len: usize) -> String;
}

/// Pick a random element from a slice.
pub fn fuzz_pick<'a, T>(state: &mut dyn FuzzerState, items: &'a [T]) -> &'a T {
    &items[state.gen_range(0, items.len())]
}

/// Simple PRNG-based FuzzerState for testing.
pub struct SimpleFuzzerState {
    seed: u64,
}

impl SimpleFuzzerState {
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    fn next(&mut self) -> u64 {
        // xorshift64
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed = x;
        x
    }
}

impl FuzzerState for SimpleFuzzerState {
    fn gen_bool(&mut self) -> bool {
        self.next() & 1 == 1
    }

    fn gen_u8(&mut self) -> u8 {
        self.next() as u8
    }

    fn gen_range(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        min + (self.next() as usize % (max - min))
    }

    fn gen_string(&mut self, max_len: usize) -> String {
        let len = self.gen_range(0, max_len);
        (0..len)
            .map(|_| {
                let n = self.gen_range(32, 127); // printable ASCII
                char::from_u32(n as u32).unwrap_or(' ')
            })
            .collect()
    }
}

/// Trait for generating realistic fake outputs for fuzzing state machines.
///
/// Implement this alongside `IOProvider<I, O>` to enable automatic fuzzing
/// of state machines without real I/O. The `fuzz` method should produce
/// outputs that are structurally valid for `O` — realistic enough that the
/// state machine can process them without panicking on malformed data.
pub trait Fuzz<O> {
    fn fuzz(&self, state: &mut dyn FuzzerState) -> O;
}
