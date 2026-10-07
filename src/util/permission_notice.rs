use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

// Only authenticated voice players reach this cache. Bound it independently of
// the number of historical sessions and fail closed rather than flood the HUD.
const MAX_NOTICES: usize = 4096;
const CLEANUP_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum VoicePermission {
    Speak,
    Listen,
}

impl VoicePermission {
    pub fn permission(self) -> &'static str {
        match self {
            Self::Speak => "pumpkin_voice:speak",
            Self::Listen => "pumpkin_voice:listen",
        }
    }

    pub fn message_key(self) -> &'static str {
        match self {
            Self::Speak => "voice.no_speak_permission",
            Self::Listen => "voice.no_listen_permission",
        }
    }

    fn cooldown(self) -> Duration {
        match self {
            // Match Simple Voice Chat's CooldownTimer defaults and listen timer.
            Self::Speak => Duration::from_secs(10),
            Self::Listen => Duration::from_secs(30),
        }
    }
}

pub struct PermissionNoticeCooldown {
    notices: HashMap<(Uuid, VoicePermission), Instant>,
    last_cleanup: Instant,
}

impl Default for PermissionNoticeCooldown {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

impl PermissionNoticeCooldown {
    fn new(now: Instant) -> Self {
        Self {
            notices: HashMap::new(),
            last_cleanup: now,
        }
    }

    /// Pure bookkeeping: callers must release their lock before calling the
    /// Pumpkin host to display the notice, as host calls can re-enter the plugin.
    pub fn should_notify(&mut self, player: Uuid, kind: VoicePermission, now: Instant) -> bool {
        if now.saturating_duration_since(self.last_cleanup) >= CLEANUP_INTERVAL {
            self.notices.retain(|(_, permission), last| {
                now.saturating_duration_since(*last) <= permission.cooldown()
            });
            self.last_cleanup = now;
        }

        let key = (player, kind);
        if let Some(last) = self.notices.get_mut(&key) {
            if now.saturating_duration_since(*last) <= kind.cooldown() {
                return false;
            }
            *last = now;
        } else {
            if self.notices.len() >= MAX_NOTICES {
                return false;
            }
            self.notices.insert(key, now);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_notices_follow_the_upstream_ten_second_cooldown() {
        let now = Instant::now();
        let player = Uuid::from_u128(1);
        let mut notices = PermissionNoticeCooldown::new(now);
        assert!(notices.should_notify(player, VoicePermission::Speak, now));
        assert!(!notices.should_notify(player, VoicePermission::Speak, now));
        assert!(!notices.should_notify(
            player,
            VoicePermission::Speak,
            now + Duration::from_secs(10)
        ));
        assert!(notices.should_notify(
            player,
            VoicePermission::Speak,
            now + Duration::from_millis(10_001)
        ));
        assert!(!notices.should_notify(
            player,
            VoicePermission::Speak,
            now + Duration::from_secs(11)
        ));
    }

    #[test]
    fn listening_has_an_independent_thirty_second_cooldown_per_player() {
        let now = Instant::now();
        let player = Uuid::from_u128(1);
        let other = Uuid::from_u128(2);
        let mut notices = PermissionNoticeCooldown::new(now);
        assert!(notices.should_notify(player, VoicePermission::Speak, now));
        assert!(notices.should_notify(player, VoicePermission::Listen, now));
        assert!(notices.should_notify(other, VoicePermission::Listen, now));
        assert!(!notices.should_notify(
            player,
            VoicePermission::Listen,
            now + Duration::from_secs(30)
        ));
        assert!(notices.should_notify(
            player,
            VoicePermission::Listen,
            now + Duration::from_millis(30_001)
        ));
    }

    #[test]
    fn cache_is_bounded_and_reclaims_expired_sessions() {
        let now = Instant::now();
        let mut notices = PermissionNoticeCooldown::new(now);
        for player in 0..MAX_NOTICES {
            assert!(notices.should_notify(
                Uuid::from_u128(player as u128),
                VoicePermission::Listen,
                now
            ));
        }
        let other = Uuid::from_u128(MAX_NOTICES as u128);
        assert!(!notices.should_notify(other, VoicePermission::Speak, now));
        assert_eq!(notices.notices.len(), MAX_NOTICES);
        assert!(notices.should_notify(
            other,
            VoicePermission::Speak,
            now + Duration::from_secs(31)
        ));
        assert_eq!(notices.notices.len(), 1);
    }

    #[test]
    fn earlier_timestamps_do_not_bypass_cooldowns_or_panic() {
        let now = Instant::now();
        let mut notices = PermissionNoticeCooldown::new(now);
        let player = Uuid::from_u128(1);
        assert!(notices.should_notify(
            player,
            VoicePermission::Listen,
            now + Duration::from_secs(1)
        ));
        assert!(!notices.should_notify(player, VoicePermission::Listen, now));
    }
}
