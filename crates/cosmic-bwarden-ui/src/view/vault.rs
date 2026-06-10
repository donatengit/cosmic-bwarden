use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, container, text, icon, settings as cosmic_settings, divider, secure_input, list_column};
use cosmic_bwarden_core::db::{Entry, EntryData};
use crate::app::CosmicBWardenApp;
use crate::message::{Message};
use crate::fl;

impl CosmicBWardenApp {
    pub fn view_vault(&self) -> Element<'_, Message> {
        let sidebar = self.view_sidebar();
        let details = if let Some(entry) = &self.selected_entry {
            self.view_entry_details(entry)
        } else {
            container(text::body(fl!("select-entry"))).center_x(Length::Fill).center_y(Length::Fill).into()
        };

        cosmic::widget::row::with_capacity(2).spacing(0)
            .push(container(sidebar)
                .class(cosmic::theme::Container::Background)
                .width(Length::Fixed(300.0))
                .height(Length::Fill)
                .padding(10))
            .push(divider::vertical::default())
            .push(container(details).width(Length::Fill).height(Length::Fill))
            .into()
    }

    pub fn view_entry_details<'a>(&'a self, entry: &'a Entry) -> Element<'a, Message> {
        let is_editing = self.editing_entry.is_some();
        let mut col = cosmic::widget::column::with_capacity(10).spacing(20).padding(20);

        let header = cosmic::widget::row::with_capacity(3).spacing(10).align_y(Alignment::Center)
            .push(if !is_editing {
                let is_pinned = self.entries.iter().find(|e| e.id == entry.id).map(|e| e.is_pinned).unwrap_or(false);
                let icon_name = if is_pinned { "starred-symbolic" } else { "non-starred-symbolic" };
                Element::from(button::icon(icon::from_name(icon_name))
                    .on_press(Message::TogglePin(entry.id.clone())))
            } else {
                Element::from(cosmic::widget::Space::new().width(Length::Fixed(40.0)))
            })
            .push(if is_editing {
                Element::from(cosmic::widget::text_input::text_input("Name", &self.editing_entry.as_ref().unwrap().name)
                    .on_input(Message::EditNameChanged)
                    .width(Length::Fill))
            } else {
                text::title2(&entry.name).width(Length::Fill).into()
            })
            .push(if is_editing {
                Element::from(cosmic::widget::row::with_capacity(2).spacing(5)
                    .push(button::suggested("Save").on_press(Message::SaveEdit))
                    .push(button::standard("Cancel").on_press(Message::CancelEdit)))
            } else {
                Element::from(button::suggested("Edit").on_press(Message::EditEntry))
            });
        
        col = col.push(header);

        let mut fields_col = cosmic::widget::column::with_capacity(5).spacing(10);
        
        if is_editing {
            let editing = self.editing_entry.as_ref().unwrap();
            let is_new = editing.id.starts_with("new-");

            if is_new {
                let entry_type_selector = cosmic::widget::row::with_capacity(3).spacing(10)
                    .push(button::standard("Login").on_press(Message::NewEntryTypeChanged(cosmic_bwarden_core::protocol::EntryType::Login)))
                    .push(button::standard("Note").on_press(Message::NewEntryTypeChanged(cosmic_bwarden_core::protocol::EntryType::SecureNote)))
                    .push(button::standard("SSH Key").on_press(Message::NewEntryTypeChanged(cosmic_bwarden_core::protocol::EntryType::SshKey)));
                fields_col = fields_col.push(text::body("Entry Type"));
                fields_col = fields_col.push(entry_type_selector);
                fields_col = fields_col.push(divider::horizontal::default());
            }
            
            match &editing.data {
                EntryData::Login { username, password, totp, .. } => {
                    fields_col = fields_col.push(cosmic_settings::item("Username",
                        cosmic::widget::text_input::text_input("Username", username.as_deref().unwrap_or(""))
                            .on_input(|v| Message::EditFieldChanged("Username".to_string(), v))));

                    let pw_input = secure_input("Password", password.as_ref().map(|s| s.expose()).unwrap_or(""), Some(Message::ToggleEditPasswordReveal), !self.edit_password_revealed)
                            .on_input(|v| Message::EditFieldChanged("Password".to_string(), v));

                    fields_col = fields_col.push(cosmic_settings::item("Password", pw_input));

                    let totp_input = cosmic::widget::text_input::text_input("TOTP Seed", totp.as_ref().map(|s| s.expose()).unwrap_or(""))
                            .on_input(|v| Message::EditFieldChanged("TOTP".to_string(), v));
                    fields_col = fields_col.push(cosmic_settings::item("TOTP Seed", totp_input));
                }
                EntryData::SshKey { private_key, public_key, .. } => {
                    let pk_input = secure_input("Private Key", private_key.as_ref().map(|s| s.expose()).unwrap_or(""), Some(Message::ToggleEditPasswordReveal), !self.edit_password_revealed)
                            .on_input(|v| Message::EditFieldChanged("Private Key".to_string(), v));

                    fields_col = fields_col.push(cosmic_settings::item("Private Key", pk_input));

                    fields_col = fields_col.push(cosmic_settings::item("Public Key",
                        cosmic::widget::text_input::text_input("Public Key", public_key.as_deref().unwrap_or(""))
                            .on_input(|v| Message::EditFieldChanged("Public Key".to_string(), v))));
                }
                _ => {}
            }
        } else {
            match &entry.data {
                EntryData::Login { username, password, totp, .. } => {
                    if let Some(u) = username {
                        fields_col = fields_col.push(self.view_field("Username", u, &entry.id, false));
                    }
                    if let Some(p) = password {
                        fields_col = fields_col.push(self.view_field("Password", p.expose(), &entry.id, true));
                    }
                    if let Some(t) = totp {
                        fields_col = fields_col.push(self.view_field("TOTP", t.expose(), &entry.id, true));
                    }
                }
                EntryData::SshKey { private_key, public_key, .. } => {
                    if let Some(pk) = private_key {
                        fields_col = fields_col.push(self.view_field("Private Key", pk.expose(), &entry.id, true));
                    }
                    if let Some(pubk) = public_key {
                        fields_col = fields_col.push(self.view_field("Public Key", pubk, &entry.id, false));
                    }
                }
                EntryData::Card { number, cardholder_name, brand, .. } => {
                    if let Some(n) = number {
                        fields_col = fields_col.push(self.view_field("Card Number", n.expose(), &entry.id, true));
                    }
                    if let Some(c) = cardholder_name {
                        fields_col = fields_col.push(self.view_field("Cardholder", c, &entry.id, false));
                    }
                    if let Some(b) = brand {
                        fields_col = fields_col.push(self.view_field("Brand", b, &entry.id, false));
                    }
                }
                EntryData::Identity { username, email, .. } => {
                    if let Some(u) = username {
                        fields_col = fields_col.push(self.view_field("Username", u, &entry.id, false));
                    }
                    if let Some(e) = email {
                        fields_col = fields_col.push(self.view_field("Email", e, &entry.id, false));
                    }
                }
                _ => {}
            }

            for field in &entry.fields {
                if let (Some(name), Some(value)) = (&field.name, &field.value) {
                    let is_hidden = field.ty == Some(cosmic_bwarden_core::api::FieldType::Hidden);
                    fields_col = fields_col.push(self.view_field(name, value.expose(), &entry.id, is_hidden));
                }
            }
        }

        col = col.push(fields_col);
        col = col.push(divider::horizontal::default());
        col = col.push(text::body("Notes"));

        let notes_editor = cosmic::widget::text_editor(&self.notes_content)
            .on_action(Message::NotesAction);
        col = col.push(container(notes_editor).height(Length::Fixed(200.0)).padding(5).class(cosmic::theme::Container::Background));

        if is_editing {
            col = col.push(divider::horizontal::default());
            col = col.push(button::destructive("Delete Entry")
                .on_press(Message::DeleteEntry(entry.id.clone()))
                .width(Length::Fill));
        }

        cosmic::widget::scrollable(col).height(Length::Fill).into()
    }

    pub fn view_field<'a>(&'a self, label: &'a str, value: &'a str, entry_id: &str, is_hidden: bool) -> Element<'a, Message> {
        let is_password = is_hidden || label.to_lowercase().contains("password") || label.to_lowercase().contains("key") || label == "TOTP";
        let is_revealed = self.revealed_fields.contains(&(entry_id.to_string(), label.to_string()));

        let mut row = cosmic::widget::row::with_capacity(3).spacing(10).align_y(Alignment::Center);
        row = row.push(text::body(label).width(Length::Fixed(100.0)));

        if is_password {
            let pw_input = secure_input("", value, Some(Message::ToggleRevealField(entry_id.to_string(), label.to_string())), !is_revealed)
                .width(Length::Fill);
            row = row.push(pw_input);
        } else {
            row = row.push(text::body(value).width(Length::Fill));
        }

        row = row.push(button::icon(icon::from_name("edit-copy-symbolic"))
            .on_press(Message::CopyToClipboard(value.to_string())));
        container(row).padding(5).into()
    }

    pub fn view_sidebar(&self) -> Element<'_, Message> {
        use cosmic::widget::{column, text_input};

        let mut sidebar = column::with_capacity(5).spacing(10).height(Length::Fill);

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

        // Filter Type
        let filter_options = vec!["All", "Logins", "Notes", "SSH Keys"];
        let selected_filter_idx = match self.filter_type.as_deref() {
            None | Some("All") => 0,
            Some("Logins") => 1,
            Some("Notes") => 2,
            Some("SSH Keys") => 3,
            _ => 0,
        };

        let mut filter_row = cosmic::widget::row::with_capacity(filter_options.len()).spacing(5);
        for (i, opt) in filter_options.iter().enumerate() {
            let btn = if i == selected_filter_idx {
                button::suggested(*opt)
            } else {
                button::standard(*opt)
            };
            filter_row = filter_row.push(btn.on_press(Message::FilterTypeChanged(if *opt == "All" { None } else { Some(opt.to_string()) })));
        }
        sidebar = sidebar.push(filter_row);
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
        let mut bottom_row = cosmic::widget::row::with_capacity(2).spacing(10).align_y(Alignment::Center);
        bottom_row = bottom_row.push(button::suggested("Add").on_press(Message::AddEntryRequested).width(Length::Fill));
        bottom_row = bottom_row.push(button::standard("Sync").on_press(Message::SyncClicked));
        sidebar = sidebar.push(bottom_row);

        sidebar.into()
    }
}
