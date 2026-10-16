use std::time::{Duration, Instant};
use tracing::warn;

pub struct CircuitBreaker {
    max_per_second: u32,
    window_start: Instant,
    event_count: u32,
    max_payload_bytes: usize,
}

impl CircuitBreaker {
    pub fn new(max_per_second: u32, max_payload_mb: usize) -> Self {
        Self {
            max_per_second: max_per_second.max(1),
            window_start: Instant::now(),
            event_count: 0,
            max_payload_bytes: max_payload_mb * 1024 * 1024,
        }
    }

    /// Returns true if the event should be processed, false if rate limit exceeded
    pub fn allow_event(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.window_start) >= Duration::from_secs(1) {
            self.window_start = now;
            self.event_count = 1;
            true
        } else if self.event_count < self.max_per_second {
            self.event_count += 1;
            true
        } else {
            warn!(
                "Clipboard rate limit reached (>{}/sec). Throttling incoming events.",
                self.max_per_second
            );
            false
        }
    }

    /// Returns true if the payload size is within acceptable bounds
    pub fn allow_payload_size(&self, bytes: usize) -> bool {
        if bytes > self.max_payload_bytes {
            warn!(
                "Payload size {} bytes exceeds configured limit {} bytes. Dropping payload.",
                bytes, self.max_payload_bytes
            );
            false
        } else {
            true
        }
    }
}
