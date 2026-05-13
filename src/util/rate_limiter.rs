use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use uuid::Uuid;

pub struct PacketRateLimiter {
    rate_limiters: Mutex<HashMap<Uuid, RateLimiter>>,
    max_packets_per_second: i32,
    time_window: Duration,
}

impl PacketRateLimiter {
    pub fn new(max_packets_per_second: i32) -> Self {
        Self {
            rate_limiters: Mutex::new(HashMap::new()),
            max_packets_per_second,
            time_window: Duration::from_secs(5),
        }
    }

    pub fn allow(&self, player: Uuid) -> bool {
        if self.max_packets_per_second <= 0 {
            return true;
        }

        let mut limiters = self.rate_limiters.lock().unwrap();
        let threshold = (self.max_packets_per_second as f32 * self.time_window.as_secs_f32()) as u64;
        
        let limiter = limiters.entry(player).or_insert_with(|| {
            RateLimiter::new(threshold, self.time_window)
        });

        limiter.try_acquire()
    }

    pub fn on_player_logged_out(&self, player: Uuid) {
        let mut limiters = self.rate_limiters.lock().unwrap();
        limiters.remove(&player);
    }
}

struct RateLimiter {
    threshold: u64,
    time_per_token: Duration,
    last_leak: Instant,
    amount: u64,
}

impl RateLimiter {
    fn new(threshold: u64, window: Duration) -> Self {
        Self {
            threshold,
            time_per_token: window / threshold as u32,
            last_leak: Instant::now(),
            amount: 0,
        }
    }

    fn try_acquire(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_leak);
        let leaked_tokens = (elapsed.as_nanos() / self.time_per_token.as_nanos()) as u64;

        if leaked_tokens > 0 {
            self.amount = self.amount.saturating_sub(leaked_tokens);
            self.last_leak += self.time_per_token * leaked_tokens as u32;
        }

        if self.amount >= self.threshold {
            return false;
        }

        self.amount += 1;
        true
    }
}
