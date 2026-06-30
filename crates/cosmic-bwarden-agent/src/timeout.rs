use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Wakes every CHECK_INTERVAL to see if `lock_timeout` seconds have elapsed
/// since `last_activity`. Avoids timer churn: even frequent vault operations
/// only update an atomic, never cancel/reschedule a sleep.
const CHECK_INTERVAL: Duration = Duration::from_secs(300); // 5 minutes

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
            tokio::time::sleep(CHECK_INTERVAL).await;
            let timeout = self.handle.lock_timeout_secs.load(Ordering::Relaxed);
            if timeout == 0 {
                continue;
            }
            let last = self.handle.last_activity.load(Ordering::Relaxed);
            if now_secs().saturating_sub(last) >= timeout {
                let mut s = state.lock().await;
                if s.keys.is_some() {
                    log::info!("autolock: no activity for {} seconds, locking vault", timeout);
                    s.lock();
                }
            }
        }
    }
}
