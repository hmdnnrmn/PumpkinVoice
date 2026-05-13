use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub struct PacketRateLimiter {
    rate_limiters: Mutex<HashMap<Uuid, RateLimiter>>,
    max_packets_per_second: i32,
    threshold: u64,
    time_window: Duration,
}

impl PacketRateLimiter {
    pub fn new(max_packets_per_second: i32) -> Self {
        let time_window = Duration::from_secs(1);
        let threshold = if max_packets_per_second > 0 {
            (max_packets_per_second as f32 * time_window.as_secs_f32()) as u64
        } else {
            0
        };

        Self {
            rate_limiters: Mutex::new(HashMap::new()),
            max_packets_per_second,
            threshold,
            time_window,
        }
    }

    pub fn allow(&self, player: Uuid) -> bool {
        if self.max_packets_per_second <= 0 {
            return true;
        }

        let mut limiters = self.rate_limiters.lock().unwrap();
        let limiter = limiters
            .entry(player)
            .or_insert_with(|| RateLimiter::new(self.threshold, self.time_window));

        limiter.try_acquire()
    }

    pub fn on_player_logged_out(&self, player: Uuid) {
        let mut limiters = self.rate_limiters.lock().unwrap();
        limiters.remove(&player);
    }
}

struct RateLimiter {
    threshold: u64,
    time_per_token_ns: u64,
    last_leak: Instant,
    amount: u64,
}

impl RateLimiter {
    fn new(threshold: u64, window: Duration) -> Self {
        Self {
            threshold,
            time_per_token_ns: (window.as_nanos() as u64) / threshold.max(1),
            last_leak: Instant::now(),
            amount: 0,
        }
    }

    fn try_acquire(&mut self) -> bool {
        let now = Instant::now();
        let elapsed_ns = now.duration_since(self.last_leak).as_nanos() as u64;
        let leaked_tokens = elapsed_ns / self.time_per_token_ns;

        if leaked_tokens > 0 {
            self.amount = self.amount.saturating_sub(leaked_tokens);
            // Instead of multiplying and potentially overflowing, just sync to now if we leaked everything
            if self.amount == 0 {
                self.last_leak = now;
            } else {
                self.last_leak += Duration::from_nanos(leaked_tokens * self.time_per_token_ns);
            }
        }

        if self.amount >= self.threshold {
            return false;
        }

        self.amount += 1;
        true
    }
}
