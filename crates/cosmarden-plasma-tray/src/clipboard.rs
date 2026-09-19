//! Clipboard writes with the same auto-clear policy as the UI.
//!
//! This is the crate's only plaintext boundary: `Secret::expose()` is called
//! here and nowhere else. `Secret`'s `Display` prints asterisks, so a value that
//! is meant to be copied has to be exposed deliberately. The `Secret` itself is
//! then moved into the auto-clear task, which is what needs the plaintext to
//! decide whether the selection is still ours — it is zeroized when that task
//! ends, and the read-back used for the comparison is zeroized too.
//!
//! Nothing here is exercised by a unit test. A test must never write to the
//! developer's own selection (`README`/`AGENTS.md`: tests never touch real user
//! state); the parts worth asserting are pure and live in `state`/`menu`, and
//! [`should_clear`] is the one policy function.

use cosmarden_core::db::Secret;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Whether the clipboard still holds our value.
///
/// The timer clears only our own copy: anything the user copied in the
/// meantime — another password, a URL, a file — must survive it.
pub fn should_clear(current: &str, ours: &str) -> bool {
    current == ours
}

/// One clipboard for the process lifetime.
///
/// On Wayland the contents are served by the owning process, so the handle has
/// to outlive the copy: dropping it would leave the selection empty the moment
/// the write returns.
fn clipboard() -> Option<&'static Mutex<arboard::Clipboard>> {
    static CLIPBOARD: OnceLock<Option<Mutex<arboard::Clipboard>>> = OnceLock::new();
    CLIPBOARD
        .get_or_init(|| match arboard::Clipboard::new() {
            Ok(clipboard) => Some(Mutex::new(clipboard)),
            Err(e) => {
                tracing::error!("no clipboard available: {e}");
                None
            }
        })
        .as_ref()
}

/// Put `secret` on the clipboard and schedule the auto-clear.
///
/// `what` is the log line's subject (an entry label), never the value.
pub fn copy_secret(secret: Secret, what: &str) {
    let Some(handle) = clipboard() else {
        tracing::error!("{what} was not copied: this session has no clipboard");
        return;
    };

    let write = handle.lock().map_err(|_| ()).and_then(|mut clipboard| {
        // The one legitimate `expose()`: the clipboard is plaintext by nature.
        clipboard.set_text(secret.expose()).map_err(|_| ())
    });
    match write {
        Ok(()) => tracing::info!(
            "{what} copied; the clipboard is cleared in {}s unless it changes",
            cosmarden_core::CLIPBOARD_CLEAR_SECS
        ),
        Err(()) => {
            tracing::error!("writing {what} to the clipboard failed");
            return;
        }
    }

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(cosmarden_core::CLIPBOARD_CLEAR_SECS)).await;
        clear_if_ours(&secret, handle);
    });
}

/// Clear the selection when it is still ours (see [`should_clear`]).
fn clear_if_ours(ours: &Secret, handle: &'static Mutex<arboard::Clipboard>) {
    let Ok(mut clipboard) = handle.lock() else {
        return;
    };
    match clipboard.get_text() {
        Ok(mut current) => {
            let ours = ours.expose();
            let clear = should_clear(&current, ours);
            // The read-back is plaintext too; wipe our copy of it.
            use zeroize::Zeroize as _;
            current.zeroize();
            if clear {
                if let Err(e) = clipboard.set_text(String::new()) {
                    tracing::warn!("clearing the copied secret failed: {e}");
                } else {
                    tracing::debug!("copied secret cleared from the clipboard");
                }
            }
        }
        // Reading can fail when another owner took the selection; then there is
        // nothing of ours to clear.
        Err(e) => tracing::debug!("clipboard read failed, leaving it alone: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_own_value_is_cleared() {
        assert!(should_clear("hunter2", "hunter2"));
        assert!(
            !should_clear("something the user copied", "hunter2"),
            "a value the user replaced ours with must survive the timer"
        );
        assert!(!should_clear("", "hunter2"));
    }
}
