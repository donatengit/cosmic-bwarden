//! Shared session-secret wipe for lock, logout, and autolock.

use crate::app::state::CosmicBWardenApp;
use crate::message::View;
use cosmic::widget;
use zeroize::Zeroize as _;

impl CosmicBWardenApp {
    /// Drop decrypted vault state and form secrets. Unlock *mode* (PIN vs
    /// password) is left to the caller.
    pub(crate) fn wipe_session_secrets(&mut self) {
        self.selected_entry = None;
        self.editing_entry = None;
        self.selected_entry_id = None;
        self.entries.clear();
        self.all_entries.clear();
        self.revealed_fields.clear();
        self.error = None;
        self.pin_incorrect = false;

        self.login_password.zeroize();
        self.unlock_password.zeroize();
        self.unlock_pin.zeroize();
        self.unlock_pin_revealed = false;
        self.unlock_pin_apply_pending = false;
        self.main_window_pin.zeroize();
        self.reprompt_password.zeroize();
        self.reprompt_intent = None;
        self.pending_secret_field = None;
        if let Some(mut code) = self.totp_code.take() {
            code.zeroize();
        }
        self.applet_unlock_password.zeroize();
        self.applet_reprompt_password.zeroize();
        self.applet_pin.zeroize();
        self.login_pin.zeroize();
        self.login_pin_enabled = false;
        self.login_pin_revealed = false;
        self.tpm_setup_pin.zeroize();

        self.notes_content = widget::text_editor::Content::new();
        self.generator_history_revealed.clear();
        if let Some(mut leftover) = self.generator_result.take() {
            leftover.zeroize();
        }
        self.generator_result_revealed = false;
        for entry in &mut self.generator_history {
            entry.password.zeroize();
        }
        self.generator_history.clear();
        if let Some(mut pending) = self.clipboard_pending_clear.take() {
            pending.zeroize();
        }
    }

    pub(crate) fn wipe_for_lock(&mut self) {
        self.view = View::Unlock;
        self.wipe_session_secrets();
    }
}
