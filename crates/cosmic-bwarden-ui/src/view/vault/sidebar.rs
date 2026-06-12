use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, container, divider, icon, list_column, text};
use cosmic_bwarden_core::protocol::EntryType;
use crate::app::CosmicBWardenApp;
use crate::message::{Message, View};
use crate::fl;

const FILTER_LABELS: [&str; 4] = ["All", "Logins", "Notes", "SSH Keys"];

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
        use cosmic::widget::{column, text_input};

        let mut sidebar = column::with_capacity(6).spacing(10).height(Length::Fill);

        // Search Bar
        let search_input = text_input::text_input(fl!("search"), &self.search_query)
            .on_input(Message::SearchChanged)
            .on_submit(Message::SearchSubmitted);

        let star_icon = if self.search_only_pinned { "starred-symbolic" } else { "non-starred-symbolic" };
        let star_btn = button::icon(icon::from_name(star_icon))
            .on_press(Message::ToggleSearchPinned);

        sidebar = sidebar.push(cosmic::widget::row::with_capacity(2).spacing(5).align_y(Alignment::Center)
            .push(search_input)
            .push(star_btn));

        // Filter dropdown
        let filter_dropdown = cosmic::widget::dropdown(
            &FILTER_LABELS[..],
            Some(filter_to_idx(&self.filter_type)),
            |idx| Message::FilterTypeChanged(idx_to_filter(idx)),
        ).width(Length::Fill);
        sidebar = sidebar.push(filter_dropdown);
        sidebar = sidebar.push(divider::horizontal::default());

        // Entry List
        let mut list = list_column();
        if self.entries.is_empty() {
            list = list.add(container(text::body("No entries found")).padding(10));
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
        top_row = top_row.push(button::suggested("Add").on_press(Message::AddEntryRequested).width(Length::Fill));
        top_row = top_row.push(button::standard("Sync").on_press(Message::SyncClicked));
        sidebar = sidebar.push(top_row);

        let mut bottom_row = cosmic::widget::row::with_capacity(3).spacing(10).align_y(Alignment::Center);
        let settings_btn = if self.view == View::Settings { button::suggested(fl!("settings")) } else { button::standard(fl!("settings")) };
        bottom_row = bottom_row.push(settings_btn.on_press(Message::SettingsViewClicked).width(Length::Fill));
        bottom_row = bottom_row.push(button::standard(fl!("lock")).on_press(Message::LockClicked));
        bottom_row = bottom_row.push(button::standard(fl!("logout")).on_press(Message::LogoutClicked));
        sidebar = sidebar.push(bottom_row);

        sidebar.into()
    }
}
