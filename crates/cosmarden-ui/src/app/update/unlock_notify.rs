//! Desktop notification when something (SSH agent, browser) needs the vault
//! unlocked.
//!
//! Payload construction is pure so tests can assert the copy and symbolic
//! icon without a notification daemon. The session-bus `Notify` call is a
//! thin wrapper: failure is `warn!` and the unlock-view priming still
//! happens.

use super::lifecycle::{check_tpm_da_task, fetch_config_task};
use crate::app::state::CosmardenApp;
use crate::fl;
use crate::message::{Message, UnlockMode, View};
use cosmic::app::Task;
use std::collections::HashMap;
use zbus::proxy;

/// Freedesktop icon name for the lightweight symbolic mark installed to
/// hicolor (`com.enikeev.cosmarden-symbolic.svg`).
pub const APP_ICON: &str = "com.enikeev.cosmarden-symbolic";

/// What `org.freedesktop.Notifications.Notify` is sent. No secrets: every
/// field is fixed app copy or the public icon name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotifyPayload {
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
}

impl NotifyPayload {
    pub fn unlock_requested() -> Self {
        Self {
            app_name: fl!("app-title"),
            app_icon: APP_ICON.to_string(),
            summary: fl!("unlock-requested-summary"),
            body: fl!("unlock-requested-body"),
        }
    }

    /// Arguments `send` passes to `Notify`. Empty actions/hints live at the
    /// call site; this is the rest of the wire shape (icon, copy, timeout).
    pub fn notify_args(&self) -> NotifyArgs<'_> {
        NotifyArgs {
            app_name: &self.app_name,
            replaces_id: 0,
            app_icon: &self.app_icon,
            summary: &self.summary,
            body: &self.body,
            expire_timeout: -1,
        }
    }
}

/// Freedesktop `Notify` fields derived from [`NotifyPayload`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotifyArgs<'a> {
    pub app_name: &'a str,
    pub replaces_id: u32,
    pub app_icon: &'a str,
    pub summary: &'a str,
    pub body: &'a str,
    pub expire_timeout: i32,
}

/// Same decision `EventReceived(UnlockRequested|PinRequested)` uses: a
/// ready account gets the localized payload; otherwise nothing is sent.
pub fn payload_if_ready(ready: bool) -> Option<NotifyPayload> {
    ready.then(NotifyPayload::unlock_requested)
}

#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, zbus::zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;
}

async fn send(payload: &NotifyPayload) -> Result<u32, String> {
    let args = payload.notify_args();
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| e.to_string())?;
    let proxy = NotificationsProxy::new(&conn)
        .await
        .map_err(|e| e.to_string())?;
    proxy
        .notify(
            args.app_name,
            args.replaces_id,
            args.app_icon,
            args.summary,
            args.body,
            &[],
            HashMap::new(),
            args.expire_timeout,
        )
        .await
        .map_err(|e| e.to_string())
}

fn send_task(payload: NotifyPayload) -> Task<Message> {
    Task::perform(
        async move {
            if let Err(e) = send(&payload).await {
                tracing::warn!("unlock-requested desktop notification failed: {e}");
            }
        },
        |()| cosmic::action::none(),
    )
}

/// Prime the unlock form and, when an account is ready, queue a desktop
/// Notify. Never opens the applet popup.
pub fn on_unlock_event(app: &mut CosmardenApp, pin: bool) -> Task<Message> {
    let payload = payload_if_ready(app.unlock_prompt_ready());
    app.last_unlock_notify = payload.clone();
    let Some(payload) = payload else {
        return fetch_config_task();
    };

    if pin {
        app.unlock_mode = UnlockMode::Pin;
        app.password_preferred = false;
        app.pin_incorrect = false;
    } else {
        app.unlock_mode = UnlockMode::Password;
    }
    app.view = View::Unlock;
    app.selected_entry = None;
    app.editing_entry = None;
    app.selected_entry_id = None;

    let notify = send_task(payload);
    if pin {
        Task::batch([notify, check_tpm_da_task()])
    } else {
        notify
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_when_ready_is_localized_symbolic_icon() {
        let payload = payload_if_ready(true).expect("ready account notifies");
        assert_eq!(payload.app_icon, APP_ICON);
        assert_eq!(payload.app_name, fl!("app-title"));
        assert_eq!(payload.summary, fl!("unlock-requested-summary"));
        assert_eq!(payload.body, fl!("unlock-requested-body"));
        let blob = format!(
            "{} {} {} {}",
            payload.app_name, payload.app_icon, payload.summary, payload.body
        );
        for forbidden in ["password", "PIN", "ssh-ed25519", "BEGIN"] {
            assert!(
                !blob.contains(forbidden),
                "{forbidden:?} must not appear in notify copy: {blob}"
            );
        }
    }

    #[test]
    fn payload_when_not_ready_is_none() {
        assert!(payload_if_ready(false).is_none());
    }

    #[test]
    fn notify_args_are_the_wire_shape_send_uses() {
        let payload = NotifyPayload::unlock_requested();
        let args = payload.notify_args();
        assert_eq!(args.app_name, payload.app_name);
        assert_eq!(args.app_icon, APP_ICON);
        assert_eq!(args.summary, fl!("unlock-requested-summary"));
        assert_eq!(args.body, fl!("unlock-requested-body"));
        assert_eq!(args.replaces_id, 0);
        assert_eq!(args.expire_timeout, -1);
    }
}
