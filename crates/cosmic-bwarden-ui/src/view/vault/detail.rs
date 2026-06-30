use cosmic::Element;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, container, text, icon, settings as cosmic_settings, divider, secure_input};
use cosmic_bwarden_core::db::{Entry, EntryData};
use cosmic_bwarden_core::protocol::EntryType;
use crate::app::CosmicBWardenApp;
use crate::message::Message;
use crate::view::style::muted_text;

impl CosmicBWardenApp {
    pub fn view_entry_details<'a>(&'a self, entry: &'a Entry) -> Element<'a, Message> {
        let is_editing = self.editing_entry.is_some();

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

        let mut fields_col = cosmic::widget::column::with_capacity(5).spacing(10);

        if is_editing {
            let editing = self.editing_entry.as_ref().unwrap();
            let is_new = editing.id.starts_with("new-");

            if is_new {
                let entry_type_selector = cosmic::widget::row::with_capacity(3).spacing(10)
                    .push(button::standard("Login").on_press(Message::NewEntryTypeChanged(EntryType::Login)))
                    .push(button::standard("Note").on_press(Message::NewEntryTypeChanged(EntryType::SecureNote)))
                    .push(button::standard("SSH Key").on_press(Message::NewEntryTypeChanged(EntryType::SshKey)));
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

        let is_login_or_ssh = matches!(
            if is_editing { &self.editing_entry.as_ref().unwrap().data } else { &entry.data },
            EntryData::Login { .. } | EntryData::SshKey { .. }
        );

        let mut col = cosmic::widget::column::with_capacity(3).spacing(20);
        if is_login_or_ssh {
            let card = cosmic::widget::column::with_capacity(2).spacing(20)
                .push(header)
                .push(fields_col);
            col = col.push(container(card).padding(15).class(cosmic::theme::Container::Primary));
        } else {
            col = col.push(header);
            col = col.push(fields_col);
        }
        col = col.push(divider::horizontal::default());
        col = col.push(text::body("Notes"));

        let notes_editor = cosmic::widget::text_editor(&self.notes_content)
            .on_action(Message::NotesAction)
            .height(Length::Fill);
        let notes_container = container(notes_editor).height(Length::Fill).padding(5).class(cosmic::theme::Container::Background);

        let mut outer = cosmic::widget::column::with_capacity(4).spacing(10).height(Length::Fill).padding(20);
        outer = outer.push(col);
        outer = outer.push(notes_container);

        if is_editing {
            outer = outer.push(divider::horizontal::default());
            let delete_area: Element<Message> = if self.deleting {
                container(cosmic::widget::indeterminate_circular().size(24.0))
                    .center_x(Length::Fill)
                    .padding([6, 0])
                    .into()
            } else {
                button::destructive("Delete Entry")
                    .on_press(Message::DeleteEntry(entry.id.clone()))
                    .width(Length::Fill)
                    .into()
            };
            outer = outer.push(delete_area);
        }

        outer = outer.push(
            button::custom(text::caption(format!("ID: {}", entry.id)).class(muted_text()))
                .on_press(Message::CopyToClipboard(entry.id.clone()))
                .class(cosmic::theme::Button::Text)
                .padding([2, 0]),
        );

        outer.into()
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
}
