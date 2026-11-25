use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

/// Token bucket rate limiter supporting hierarchical (per-download + global) rate limiting.
#[derive(Clone)]
pub struct RateLimiter {
    inner: Arc<Mutex<LimiterState>>,
}

struct LimiterState {
    bytes_per_second: u64,
    tokens: f64,
    capacity: f64,
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(bytes_per_second: u64) -> Self {
        // If 0, unlimited
        let capacity = if bytes_per_second > 0 {
            // Cap burst capacity to at most 1/4 second or 64 KiB to avoid large initial spikes
            ((bytes_per_second as f64) * 0.25).max(65536.0)
        } else {
            0.0
        };

        Self {
            inner: Arc::new(Mutex::new(LimiterState {
                bytes_per_second,
                tokens: capacity,
                capacity,
                last_refill: Instant::now(),
            })),
        }
    }

    /// Create an unlimited rate limiter
    pub fn unlimited() -> Self {
        Self::new(0)
    }

    /// Update rate limit dynamically
    pub async fn set_limit(&self, bytes_per_second: u64) {
        let mut state = self.inner.lock().await;
        state.bytes_per_second = bytes_per_second;
        if bytes_per_second > 0 {
            state.capacity = ((bytes_per_second as f64) * 0.25).max(65536.0);
            state.tokens = state.tokens.min(state.capacity);
        }
        state.last_refill = Instant::now();
    }

    pub async fn limit(&self) -> u64 {
        let state = self.inner.lock().await;
        state.bytes_per_second
    }

    /// Asynchronously acquire tokens for `bytes` count.
    /// If speed limit is 0 (unlimited), returns immediately.
    pub async fn acquire(&self, bytes: usize) {
        if bytes == 0 {
            return;
        }

        let wait_duration = {
            let mut state = self.inner.lock().await;
            if state.bytes_per_second == 0 {
                return;
            }

            let now = Instant::now();
            let elapsed = now.duration_since(state.last_refill).as_secs_f64();
            state.last_refill = now;

            // Refill tokens
            state.tokens = (state.tokens + elapsed * (state.bytes_per_second as f64)).min(state.capacity);

            let needed = bytes as f64;
            if state.tokens >= needed {
                state.tokens -= needed;
                return;
            }

            // Need to wait until enough tokens are replenished
            let deficit = needed - state.tokens;
            let wait_secs = deficit / (state.bytes_per_second as f64);
            // Deduct tokens now (tokens can become negative to track debt)
            state.tokens -= needed;
            Duration::from_secs_f64(wait_secs)
        };

        // Sleep without holding the lock so other workers can evaluate their schedules
        sleep(wait_duration).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_unlimited() {
        let limiter = RateLimiter::unlimited();
        let start = Instant::now();
        limiter.acquire(1024 * 1024).await;
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn test_rate_limiter_throttles() {
        // Limit to 200 KB/s
        let limit_bps = 200 * 1024;
        let limiter = RateLimiter::new(limit_bps);

        let start = Instant::now();
        // Acquire 200 KB
        limiter.acquire(100 * 1024).await;
        limiter.acquire(100 * 1024).await;

        let elapsed = start.elapsed().as_secs_f64();
        // Since burst capacity is ~50KB, total ~200KB should take around 0.6 - 1.1s
        assert!(elapsed > 0.4, "Elapsed was only {:.3}s, expected throttling", elapsed);
    }
}
