use crate::app::CosmicBWardenApp;
use crate::fl;
use crate::message::{Message, View};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, container, divider, icon, list_column, segmented_button, text};
use cosmic::Element;
use cosmic_bwarden_core::protocol::EntryType;

/// Model for the type-filter segmented control. Item order is matched to
/// `filter_to_idx` / `idx_to_filter` (All, Logins, Notes, SSH Keys).
pub(crate) fn filter_model() -> segmented_button::SingleSelectModel {
    segmented_button::Model::builder()
        .insert(|b| b.text(fl!("filter-all")).activate())
        .insert(|b| b.text(fl!("filter-logins")))
        .insert(|b| b.text(fl!("filter-notes")))
        .insert(|b| b.text(fl!("filter-ssh-keys")))
        .build()
}

/// Inverse of `idx_to_filter`; production code only maps tab → filter, so
/// this direction is exercised by tests alone.
#[cfg(test)]
pub(crate) fn filter_to_idx(filter: &Option<EntryType>) -> usize {
    match filter {
        None => 0,
        Some(EntryType::Login) => 1,
        Some(EntryType::SecureNote) => 2,
        Some(EntryType::SshKey) => 3,
        Some(_) => 0,
    }
}

pub(crate) fn idx_to_filter(idx: usize) -> Option<EntryType> {
    match idx {
        1 => Some(EntryType::Login),
        2 => Some(EntryType::SecureNote),
        3 => Some(EntryType::SshKey),
        _ => None,
    }
}

impl CosmicBWardenApp {
    pub fn view_sidebar(&self) -> Element<'_, Message> {
        use cosmic::widget::{column, search_input};

        let mut sidebar = column::with_capacity(6).spacing(10).height(Length::Fill);

        // Search Bar
        let search_bar = search_input(fl!("search"), &self.search_query)
            .on_input(Message::SearchChanged)
            .width(Length::Fill);

        let star_icon = if self.search_only_pinned {
            "starred-symbolic"
        } else {
            "non-starred-symbolic"
        };
        let star_btn =
            button::icon(icon::from_name(star_icon)).on_press(Message::ToggleSearchPinned);

        sidebar = sidebar.push(
            cosmic::widget::row::with_capacity(2)
                .spacing(5)
                .align_y(Alignment::Center)
                .push(search_bar)
                .push(star_btn),
        );

        // Type filter as a joint segmented control. The Custom style
        // delegates to the stock Control appearance — identical look, but the
        // widget only draws (and reserves width for) the active-item
        // checkmark when the style is literally `Control`, so this drops it.
        let filter_control = cosmic::widget::segmented_control::horizontal(&self.filter_model)
            .on_activate(Message::FilterTabActivated)
            .style(cosmic::theme::SegmentedButton::Custom(Box::new(|theme| {
                segmented_button::StyleSheet::horizontal(
                    theme,
                    &cosmic::theme::SegmentedButton::Control,
                )
            })))
            .width(Length::Fill);
        sidebar = sidebar.push(filter_control);
        sidebar = sidebar.push(divider::horizontal::default());

        // Entry List
        let mut list = list_column();
        if self.entries.is_empty() {
            list = list.add(container(text::body(fl!("no-entries-found"))).padding(10));
        } else {
            for entry in &self.entries {
                let id = entry.id.clone();
                let is_selected = self.selected_entry_id.as_deref() == Some(&id);
                let mut btn = button::text(&entry.name)
                    .on_press(Message::SelectEntry(id))
                    .width(Length::Fill);
                if is_selected {
                    btn = btn.class(cosmic::theme::Button::Suggested);
                }
                list = list.add(btn);
            }
        }
        sidebar = sidebar.push(cosmic::widget::scrollable(list).height(Length::Fill));

        // Bottom Actions
        sidebar = sidebar.push(divider::horizontal::default());

        // Row 1: Add and Password Generator (never accented — a secondary
        // tool, not a primary navigation destination like Settings).
        sidebar = sidebar.push(
            cosmic::widget::row::with_capacity(2)
                .spacing(10)
                .align_y(Alignment::Center)
                .push(
                    button::suggested(fl!("add"))
                        .on_press(Message::AddEntryRequested)
                        .width(Length::Fill),
                )
                .push(
                    button::standard(fl!("password-generator"))
                        .on_press(Message::GeneratorViewClicked)
                        .width(Length::Fill),
                ),
        );

        let session_expired = self.sync_failed
            && self
                .error
                .as_deref()
                .map(|e| e.contains("session token"))
                .unwrap_or(false);

        // Sync is icon-only like Lock/Logout; its state shows through the
        // icon + destructive class, the tooltip names the state.
        let sync_area: cosmic::Element<Message> = if self.syncing {
            // Show a small spinner while the sync request is in flight so the
            // user gets immediate feedback that something is happening.
            container(cosmic::widget::indeterminate_circular().size(20.0))
                .center_x(Length::Fixed(32.0))
                .center_y(Length::Fixed(32.0))
                .into()
        } else if session_expired {
            button::icon(icon::from_name("dialog-password-symbolic"))
                .class(cosmic::theme::Button::Destructive)
                .tooltip(fl!("session-expired"))
                .on_press(Message::LogoutClicked)
                .into()
        } else if self.sync_failed {
            button::icon(icon::from_name("network-error-symbolic"))
                .class(cosmic::theme::Button::Destructive)
                .tooltip(fl!("not-synced"))
                .on_press(Message::SyncClicked)
                .into()
        } else {
            button::icon(icon::from_name("emblem-synchronizing-symbolic"))
                .tooltip(fl!("sync"))
                .on_press(Message::SyncClicked)
                .into()
        };

        // Row 2: Sync, Lock, Logout (all icons, compact) and Settings.
        let lock_btn = button::icon(icon::from_name("system-lock-screen-symbolic"))
            .tooltip(fl!("lock"))
            .on_press(Message::LockClicked);
        let logout_btn = button::icon(icon::from_name("system-log-out-symbolic"))
            .tooltip(fl!("logout"))
            .on_press(Message::LogoutClicked);
        let settings_btn = if self.view == View::Settings {
            button::suggested(fl!("settings"))
        } else {
            button::standard(fl!("settings"))
        };
        let actions_row = cosmic::widget::row::with_capacity(4)
            .spacing(5)
            .align_y(Alignment::Center)
            .push(sync_area)
            .push(lock_btn)
            .push(logout_btn)
            .push(settings_btn.on_press(Message::SettingsViewClicked));
        sidebar = sidebar.push(actions_row);

        // Show the actual error text so the user knows WHY the button is red.
        if self.sync_failed {
            if let Some(ref msg) = self.error {
                sidebar = sidebar.push(text::caption(msg.as_str()));
            }
        }

        sidebar.into()
    }
}
