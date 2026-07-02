use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, container, divider, icon, list_column, text};
use cosmic_bwarden_core::protocol::EntryType;
use crate::app::CosmicBWardenApp;
use crate::message::{Message, View};
use crate::fl;

/// Localized filter dropdown labels, order-matched to `filter_to_idx` /
/// `idx_to_filter` (All, Logins, Notes, SSH Keys).
fn filter_labels() -> Vec<String> {
    vec![
        fl!("filter-all"),
        fl!("filter-logins"),
        fl!("filter-notes"),
        fl!("filter-ssh-keys"),
    ]
}

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

        let star_icon = if self.search_only_pinned { "starred-symbolic" } else { "non-starred-symbolic" };
        let star_btn = button::icon(icon::from_name(star_icon))
            .on_press(Message::ToggleSearchPinned);

        sidebar = sidebar.push(cosmic::widget::row::with_capacity(2).spacing(5).align_y(Alignment::Center)
            .push(search_bar)
            .push(star_btn));

        // Filter dropdown
        let filter_dropdown = cosmic::widget::dropdown(
            filter_labels(),
            Some(filter_to_idx(&self.filter_type)),
            |idx| Message::FilterTypeChanged(idx_to_filter(idx)),
        ).width(Length::Fill);
        sidebar = sidebar.push(filter_dropdown);
        sidebar = sidebar.push(divider::horizontal::default());

        // Entry List
        let mut list = list_column();
        if self.entries.is_empty() {
            list = list.add(container(text::body(fl!("no-entries-found"))).padding(10));
        } else {
            for entry in &self.entries {
                let id = entry.id.clone();
                let is_selected = self.selected_entry_id.as_deref() == Some(&id);
                let mut btn = button::text(&entry.name).on_press(Message::SelectEntry(id)).width(Length::Fill);
                if is_selected {
                    btn = btn.class(cosmic::theme::Button::Suggested);
                }
                list = list.add(btn);
            }
        }
        sidebar = sidebar.push(cosmic::widget::scrollable(list).height(Length::Fill));

        // Bottom Actions
        sidebar = sidebar.push(divider::horizontal::default());

        let mut top_row = cosmic::widget::row::with_capacity(2).spacing(10).align_y(Alignment::Center);
        top_row = top_row.push(button::suggested(fl!("add")).on_press(Message::AddEntryRequested).width(Length::Fill));

        let session_expired = self.sync_failed
            && self.error.as_deref().map(|e| e.contains("session token")).unwrap_or(false);

        let sync_area: cosmic::Element<Message> = if self.syncing {
            // Show a small spinner while the sync request is in flight so the
            // user gets immediate feedback that something is happening.
            container(cosmic::widget::indeterminate_circular().size(20.0))
                .center_x(Length::Fixed(64.0))
                .center_y(Length::Fixed(32.0))
                .into()
        } else if session_expired {
            button::destructive(fl!("session-expired")).on_press(Message::LogoutClicked).into()
        } else if self.sync_failed {
            button::destructive(fl!("not-synced")).on_press(Message::SyncClicked).into()
        } else {
            button::standard(fl!("sync")).on_press(Message::SyncClicked).into()
        };
        top_row = top_row.push(sync_area);
        sidebar = sidebar.push(top_row);

        // Show the actual error text so the user knows WHY the button is red.
        if self.sync_failed {
            if let Some(ref msg) = self.error {
                sidebar = sidebar.push(text::caption(msg.as_str()));
            }
        }

        let mut bottom_row = cosmic::widget::row::with_capacity(3).spacing(10).align_y(Alignment::Center);
        let settings_btn = if self.view == View::Settings { button::suggested(fl!("settings")) } else { button::standard(fl!("settings")) };
        bottom_row = bottom_row.push(settings_btn.on_press(Message::SettingsViewClicked).width(Length::Fill));
        bottom_row = bottom_row.push(button::standard(fl!("lock")).on_press(Message::LockClicked));
        bottom_row = bottom_row.push(button::standard(fl!("logout")).on_press(Message::LogoutClicked));
        sidebar = sidebar.push(bottom_row);

        sidebar.into()
    }
}
