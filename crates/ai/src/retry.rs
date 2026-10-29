//! Retry wrapper and the concurrency-2 generation queue, ported from
//! `web/src/lib/canvas/canvas-generation-helpers.ts` (`GENERATION_*` constants,
//! `createGenerationQueue`, `runGenerationTaskWithRetry`).

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;

use crate::client::Cancel;
use crate::error::AiError;

/// `GENERATION_CONCURRENCY` (`canvas-generation-helpers.ts:153`).
pub const GENERATION_CONCURRENCY: usize = 2;
/// `GENERATION_MAX_ATTEMPTS` (`canvas-generation-helpers.ts:154`).
pub const GENERATION_MAX_ATTEMPTS: u32 = 3;
/// `GENERATION_RETRY_DELAY_MS` (`canvas-generation-helpers.ts:155`).
pub const GENERATION_RETRY_DELAY_MS: u64 = 2000;

/// `runGenerationTaskWithRetry` (`canvas-generation-helpers.ts:187`): run `task`
/// up to `attempts` times, waiting `delay_ms` between attempts, and stop early on
/// cancellation. A cancel during the delay or the task yields [`AiError::Canceled`].
pub async fn run_with_retry<T, F, Fut>(
    attempts: u32,
    delay_ms: u64,
    cancel: &Cancel,
    mut task: F,
) -> Result<T, AiError>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T, AiError>>,
{
    let attempts = attempts.max(1);
    for attempt in 1..=attempts {
        if cancel.is_canceled() {
            return Err(AiError::Canceled);
        }
        match task(attempt).await {
            Ok(value) => return Ok(value),
            Err(error) => {
                if cancel.is_canceled() {
                    return Err(AiError::Canceled);
                }
                if attempt >= attempts {
                    return Err(error);
                }
                tokio::select! {
                    () = cancel.cancelled() => return Err(AiError::Canceled),
                    () = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
                }
            }
        }
    }
    Err(AiError::Timeout("generation retries exhausted".to_string()))
}

/// `createGenerationQueue` (`canvas-generation-helpers.ts:157`): caps how many
/// generation tasks run at once, waiting for a free slot instead of rejecting.
#[derive(Debug, Clone)]
pub struct GenerationQueue {
    semaphore: Arc<Semaphore>,
}

impl GenerationQueue {
    pub fn new(limit: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(limit.max(1))),
        }
    }

    /// Runs `task` while holding a slot; the slot is released when it resolves.
    pub async fn run<T, F, Fut>(&self, task: F) -> Result<T, AiError>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, AiError>>,
    {
        let permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| AiError::Invalid("generation queue is closed".to_string()))?;
        let result = task().await;
        drop(permit);
        result
    }
}

impl Default for GenerationQueue {
    fn default() -> Self {
        Self::new(GENERATION_CONCURRENCY)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn retry_succeeds_after_transient_failures() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let cancel = Cancel::new();
        let counter = Arc::clone(&attempts);
        let result = run_with_retry(3, 1, &cancel, move |attempt| {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                if attempt < 3 {
                    Err(AiError::Network("transient".to_string()))
                } else {
                    Ok(attempt)
                }
            }
        })
        .await;
        assert_eq!(result, Ok(3));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_gives_up_after_the_attempt_budget() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let cancel = Cancel::new();
        let counter = Arc::clone(&attempts);
        let result = run_with_retry(3, 1, &cancel, move |_| {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<(), _>(AiError::Api("nope".to_string()))
            }
        })
        .await;
        assert!(matches!(result, Err(AiError::Api(_))));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_stops_immediately_when_canceled() {
        let cancel = Cancel::new();
        cancel.cancel();
        let result = run_with_retry(3, 1, &cancel, |_| async { Ok(1) }).await;
        assert_eq!(result, Err(AiError::Canceled));
    }

    #[tokio::test]
    async fn queue_limits_concurrency_to_two() {
        let queue = GenerationQueue::new(2);
        let live = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..4 {
            let queue = queue.clone();
            let live = Arc::clone(&live);
            let peak = Arc::clone(&peak);
            handles.push(tokio::spawn(async move {
                queue
                    .run(|| {
                        let live = Arc::clone(&live);
                        let peak = Arc::clone(&peak);
                        async move {
                            let now = live.fetch_add(1, Ordering::SeqCst) + 1;
                            peak.fetch_max(now, Ordering::SeqCst);
                            tokio::time::sleep(Duration::from_millis(30)).await;
                            live.fetch_sub(1, Ordering::SeqCst);
                            Ok::<_, AiError>(())
                        }
                    })
                    .await
            }));
        }
        for handle in handles {
            assert!(handle.await.is_ok());
        }
        assert_eq!(peak.load(Ordering::SeqCst), 2);
    }
}
