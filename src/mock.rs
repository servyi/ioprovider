use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use async_trait::async_trait;

use crate::provider::IOProvider;

/// Mock that returns pre-configured outputs in sequence, ignoring the input.
///
/// Panics if invoked more times than outputs were configured.
///
/// ```
/// use servyi_ioprovider::{SequenceMock, IOProvider};
///
/// # tokio_test::block_on(async {
/// let mock = SequenceMock::<String, String>::new(vec!["ok".into(), "done".into()]);
/// assert_eq!(mock.invoke("req1".into()).await.unwrap(), "ok");
/// assert_eq!(mock.invoke("req2".into()).await.unwrap(), "done");
/// # });
/// ```
pub struct SequenceMock<I, O> {
    outputs: Arc<Mutex<VecDeque<O>>>,
    inputs: Arc<Mutex<Vec<I>>>,
}

impl<I, O> SequenceMock<I, O> {
    pub fn new(outputs: Vec<O>) -> Self {
        Self {
            outputs: Arc::new(Mutex::new(outputs.into())),
            inputs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn inputs(&self) -> Vec<I>
    where
        I: Clone,
    {
        self.inputs.lock().unwrap().clone()
    }

    pub fn remaining(&self) -> usize {
        self.outputs.lock().unwrap().len()
    }
}

impl<I: Send + 'static, O: Send + 'static> Clone for SequenceMock<I, O> {
    fn clone(&self) -> Self {
        Self {
            outputs: self.outputs.clone(),
            inputs: self.inputs.clone(),
        }
    }
}

#[async_trait]
impl<I: Send + Sync + 'static, O: Send + 'static> IOProvider<I, O> for SequenceMock<I, O> {
    async fn invoke(&self, input: I) -> Result<O> {
        self.inputs.lock().unwrap().push(input);
        self.outputs
            .lock()
            .unwrap()
            .pop_front()
            .ok_or_else(|| anyhow!("SequenceMock exhausted"))
    }
}

/// Mock that computes output from input via a user-provided closure.
///
/// Requires `I: Clone` so inputs can be recorded.
///
/// ```
/// use servyi_ioprovider::{FnMock, IOProvider};
///
/// # tokio_test::block_on(async {
/// let mock = FnMock::new(|req: String| req.to_uppercase());
/// assert_eq!(mock.invoke("hello".into()).await.unwrap(), "HELLO");
/// # });
/// ```
pub struct FnMock<I, O, F>
where
    F: Fn(I) -> O + Send + Sync,
{
    func: F,
    inputs: Arc<Mutex<Vec<I>>>,
    _marker: std::marker::PhantomData<fn(I) -> O>,
}

impl<I: Clone, O, F> FnMock<I, O, F>
where
    F: Fn(I) -> O + Send + Sync,
{
    pub fn new(func: F) -> Self {
        Self {
            func,
            inputs: Arc::new(Mutex::new(Vec::new())),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn inputs(&self) -> Vec<I> {
        self.inputs.lock().unwrap().clone()
    }
}

#[async_trait]
impl<I: Clone + Send + Sync + 'static, O: Send + 'static, F: Fn(I) -> O + Send + Sync> IOProvider<I, O>
    for FnMock<I, O, F>
{
    async fn invoke(&self, input: I) -> Result<O> {
        self.inputs.lock().unwrap().push(input.clone());
        Ok((self.func)(input))
    }
}

/// Mock that always returns an error.
///
/// Useful for asserting that a code path is never reached.
pub struct NeverMock {
    message: String,
}

impl NeverMock {
    pub fn new(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }
}

#[async_trait]
impl<I: Send + Sync + 'static, O: Send + 'static> IOProvider<I, O> for NeverMock {
    async fn invoke(&self, _input: I) -> Result<O> {
        Err(anyhow!("NeverMock invoked: {}", self.message))
    }
}

/// Mock that always returns the same output.
pub struct ConstantMock<I, O> {
    output: O,
    inputs: Arc<Mutex<Vec<I>>>,
}

impl<I, O: Clone> ConstantMock<I, O> {
    pub fn new(output: O) -> Self {
        Self {
            output,
            inputs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn inputs(&self) -> Vec<I>
    where
        I: Clone,
    {
        self.inputs.lock().unwrap().clone()
    }
}

#[async_trait]
impl<I: Send + Sync + 'static, O: Clone + Send + Sync + 'static> IOProvider<I, O>
    for ConstantMock<I, O>
{
    async fn invoke(&self, input: I) -> Result<O> {
        self.inputs.lock().unwrap().push(input);
        Ok(self.output.clone())
    }
}
