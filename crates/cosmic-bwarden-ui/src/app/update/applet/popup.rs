//! Applet popup lifecycle: building the popup surface and putting values on
//! the clipboard with the auto-clear toast. Split out of `app/update/applet.rs`
//! along the same boundary as `helpers.rs`; both methods are called from the
//! message arms that stayed in the parent module.

use crate::app::state::CosmicBWardenApp;
use crate::app::tasks::check_protocol_version;
use crate::fl;
use crate::message::Message;
use cosmic::app::Task;
use cosmic::iced::window;
use cosmic::widget::text_input;
use cosmic::widget::Toast;
use cosmic_bwarden_core::agent_client::AgentClient;
use cosmic_bwarden_core::protocol::{Action as AgentAction, Response};
use zeroize::Zeroize;

use super::helpers::{clamped_anchor_rect, unlock_focus_id};

impl CosmicBWardenApp {
    /// Resets transient popup state and builds the task that opens the
    /// applet popup, focused on whichever unlock field is actually shown
    /// (PIN when TPM PIN unlock is active, master password otherwise).
    ///
    /// `(offset, bounds)` come from the icon click and position the popup
    /// next to the clicked applet icon. Only ever called from a click:
    /// cosmic-panel drops popups created while no panel surface is
    /// hovered/focused (see the note in `update_lifecycle`'s `PinRequested`),
    /// so opening from anywhere else can never map a popup.
    pub(crate) fn open_applet_popup_task(
        &mut self,
        anchor: (cosmic::iced::Vector, cosmic::iced::Rectangle),
    ) -> Task<Message> {
        // Reset only truly transient state; preserve search query and
        // favourites-filter so the popup re-opens with the last search intact.
        self.applet_unlock_password.zeroize();
        self.applet_unlock_password_revealed = false;
        self.applet_reprompt_id = None;
        self.applet_reprompt_password.zeroize();
        self.applet_reprompt_password_revealed = false;
        self.applet_error = None;
        self.applet_hovered_row_id = None;

        let mut tasks = Vec::new();
        tasks.push(check_protocol_version());
        tasks.push(text_input::focus(unlock_focus_id(self.unlock_mode)));
        tasks.push(Task::perform(
            async {
                let agent = AgentClient::new();
                match agent.send(AgentAction::GetConfig).await {
                    Ok(Response::Config {
                        config,
                        needs_login,
                        has_account,
                        is_locked,
                        sync_failed,
                        session_id,
                        lock_epoch,
                    }) => Ok((
                        config,
                        needs_login,
                        has_account,
                        is_locked,
                        sync_failed,
                        session_id,
                        lock_epoch,
                    )),
                    Ok(Response::Error { message }) => Err(message),
                    _ => Err("unexpected response".to_string()),
                }
            },
            |res| cosmic::Action::App(Message::ConfigReceived(res)),
        ));

        let popup_task = Task::done(cosmic::Action::Surface(
            cosmic::surface::action::app_popup::<CosmicBWardenApp>(
                |_| Default::default(),
                move |state: &mut CosmicBWardenApp| {
                    let new_id = window::Id::unique();
                    tracing::info!(?new_id, "creating applet popup surface");
                    state.applet_popup = Some(new_id);
                    state
                        .windows
                        .insert(new_id, crate::message::WindowState::Popup);
                    let mut popup_settings = state.core.applet.get_popup_settings(
                        state.core.main_window_id().unwrap_or(window::Id::RESERVED),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    let (offset, bounds) = anchor;
                    // Clamp the anchor rectangle to at least 1px on every side
                    // (as cosmic-applet-time does): a 0-width/0-height or negative
                    // anchor from a degenerate icon bounds would produce a
                    // zero-sized positioning rect and a misplaced/never-mapped
                    // popup. `bounds` is in logical floats; cast after clamping.
                    popup_settings.positioner.anchor_rect = clamped_anchor_rect(offset, bounds);
                    popup_settings.positioner.size_limits =
                        crate::view::applet::applet_popup_limits();
                    popup_settings
                },
                None,
            ),
        ));
        tasks.push(popup_task);
        Task::batch(tasks)
    }

    /// Copies a value to the clipboard and raises the "copied" toast. Shared by
    /// the popup's copy entries and by quick-generate, which copies straight to
    /// the clipboard instead of rendering the value.
    pub(super) fn applet_copy_to_clipboard(&mut self, value: String) -> Task<Message> {
        let clipboard_task = self.copy_to_clipboard_with_autoclear(value);
        let toast_task = self
            .applet_toasts
            .push(Toast::new(fl!("copied-to-clipboard")))
            .map(cosmic::Action::App);
        Task::batch(vec![clipboard_task, toast_task])
    }
}
