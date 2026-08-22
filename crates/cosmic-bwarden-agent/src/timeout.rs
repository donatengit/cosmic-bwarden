use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Upper bound on one sleep so a live `UpdateLockTimeout` is noticed promptly.
const MAX_SLEEP: Duration = Duration::from_secs(30);

/// Seconds until `last + timeout`, capped at [`MAX_SLEEP`]. Zero means lock now.
pub(crate) fn remaining_sleep_secs(now: u64, last: u64, timeout: u64) -> u64 {
    if timeout == 0 {
        return MAX_SLEEP.as_secs();
    }
    let elapsed = now.saturating_sub(last);
    if elapsed >= timeout {
        0
    } else {
        (timeout - elapsed).min(MAX_SLEEP.as_secs())
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Cloneable handle for recording activity or changing the timeout live.
#[derive(Clone)]
pub struct TimerHandle {
    last_activity: Arc<AtomicU64>,
    /// 0 = disabled
    lock_timeout_secs: Arc<AtomicU64>,
}

impl TimerHandle {
    /// Record vault activity — resets the inactivity countdown.
    pub fn reset(&self) {
        self.last_activity.store(now_secs(), Ordering::Relaxed);
    }

    /// Change the timeout duration. seconds=0 disables autolock.
    /// Also resets the countdown so the new duration starts from now.
    pub fn set_duration(&self, seconds: u64) {
        self.lock_timeout_secs.store(seconds, Ordering::Relaxed);
        self.reset();
    }
}

pub struct AutolockTimer {
    handle: TimerHandle,
}

impl AutolockTimer {
    pub fn new(seconds: u64) -> Self {
        Self {
            handle: TimerHandle {
                last_activity: Arc::new(AtomicU64::new(now_secs())),
                lock_timeout_secs: Arc::new(AtomicU64::new(seconds)),
            },
        }
    }

    pub fn handle(&self) -> TimerHandle {
        self.handle.clone()
    }

    /// Runs the check loop. Call `tokio::spawn(timer.run(state))` from main.
    /// Returns only when the timer handle's sender is dropped (i.e. agent shuts down).
    pub async fn run(self, state: Arc<tokio::sync::Mutex<crate::state::State>>) {
        loop {
            let timeout = self.handle.lock_timeout_secs.load(Ordering::Relaxed);
            if timeout == 0 {
                tokio::time::sleep(MAX_SLEEP).await;
                continue;
            }
            let last = self.handle.last_activity.load(Ordering::Relaxed);
            let sleep_secs = remaining_sleep_secs(now_secs(), last, timeout);
            if sleep_secs > 0 {
                tokio::time::sleep(Duration::from_secs(sleep_secs)).await;
            }
            let timeout = self.handle.lock_timeout_secs.load(Ordering::Relaxed);
            if timeout == 0 {
                continue;
            }
            let last = self.handle.last_activity.load(Ordering::Relaxed);
            if now_secs().saturating_sub(last) >= timeout {
                let mut s = state.lock().await;
                if s.keys.is_some() {
                    log::info!(
                        "autolock: no activity for {} seconds, locking vault",
                        timeout
                    );
                    s.lock();
                }
                drop(s);
                tokio::time::sleep(MAX_SLEEP).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_is_zero_once_deadline_passed() {
        assert_eq!(remaining_sleep_secs(100, 0, 5), 0);
        assert_eq!(remaining_sleep_secs(5, 0, 5), 0);
    }

    #[test]
    fn remaining_is_capped_at_max_sleep() {
        assert_eq!(remaining_sleep_secs(0, 0, 5400), 30);
        assert_eq!(remaining_sleep_secs(10, 0, 40), 30);
    }

    #[test]
    fn remaining_matches_time_left_when_under_cap() {
        assert_eq!(remaining_sleep_secs(10, 0, 25), 15);
    }

    #[test]
    fn disabled_timeout_does_not_report_lock_now() {
        assert_eq!(remaining_sleep_secs(100, 0, 0), 30);
    }
}
