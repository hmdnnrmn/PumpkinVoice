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
            max_packets_per_second as u64
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

        let allowed = limiter.try_acquire();
        let log_limit = !allowed && limiter.should_log_limit();
        let amount = limiter.amount;
        let threshold = limiter.threshold;
        drop(limiters);
        if log_limit {
            tracing::warn!(
                "{}",
                crate::i18n::translate_str_with(
                    crate::i18n::default_locale(),
                    "log.rate_limit.player",
                    &[
                        player.to_string(),
                        amount.to_string(),
                        threshold.to_string(),
                    ],
                )
            );
        }
        allowed
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
    last_warning: Option<Instant>,
}

impl RateLimiter {
    fn new(threshold: u64, window: Duration) -> Self {
        Self {
            threshold,
            time_per_token_ns: ((window.as_nanos() as u64) / threshold.max(1)).max(1),
            last_leak: Instant::now(),
            amount: 0,
            last_warning: None,
        }
    }

    fn try_acquire(&mut self) -> bool {
        let now = Instant::now();
        let elapsed_ns = now.duration_since(self.last_leak).as_nanos() as u64;
        let leaked_tokens = elapsed_ns / self.time_per_token_ns;

        if leaked_tokens > 0 {
            self.amount = self.amount.saturating_sub(leaked_tokens);
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

    fn should_log_limit(&mut self) -> bool {
        let now = Instant::now();
        if self
            .last_warning
            .is_some_and(|last| now.duration_since(last) < Duration::from_secs(5))
        {
            return false;
        }
        self.last_warning = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{PacketRateLimiter, RateLimiter};
    use std::time::Duration;
    use uuid::Uuid;

    #[test]
    fn limits_are_per_player_and_logout_clears_the_budget() {
        let limiter = PacketRateLimiter::new(1);
        let player = Uuid::new_v4();
        assert!(limiter.allow(player));
        assert!(!limiter.allow(player));
        assert!(limiter.allow(Uuid::new_v4()));
        limiter.on_player_logged_out(player);
        assert!(limiter.allow(player));
    }

    #[test]
    fn disabled_and_extreme_limits_do_not_panic() {
        for limit in [-1, 0, i32::MAX] {
            let limiter = PacketRateLimiter::new(limit);
            let player = Uuid::new_v4();
            for _ in 0..100 {
                assert!(limiter.allow(player));
            }
        }
    }

    #[test]
    fn warnings_are_throttled_and_tokens_recover() {
        let mut limiter = RateLimiter::new(1, Duration::from_secs(1));
        assert!(limiter.try_acquire());
        assert!(!limiter.try_acquire());
        assert!(limiter.should_log_limit());
        assert!(!limiter.should_log_limit());
        limiter.last_leak -= Duration::from_secs(1);
        assert!(limiter.try_acquire());
    }
}
