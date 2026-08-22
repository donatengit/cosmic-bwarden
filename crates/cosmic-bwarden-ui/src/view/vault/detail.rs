use crate::app::CosmicBWardenApp;
use crate::fl;
use crate::message::Message;
use crate::view::style::muted_text;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{
    button, container, divider, icon, secure_input, settings as cosmic_settings, text,
};
use cosmic::Element;
use cosmic_bwarden_core::db::{Entry, EntryData};
use cosmic_bwarden_core::protocol::EntryType;

/// Map a stable field *key* (used for edit-dispatch and reveal-state matching) to
/// its localized *display* label. Unknown keys — e.g. user-defined custom field
/// names — are shown verbatim. The key strings themselves must never be localized:
/// `EditFieldChanged` and `revealed_fields` match on them exactly.
fn field_label(key: &str) -> String {
    match key {
        "Username" => fl!("field-username"),
        "Password" => fl!("field-password"),
        "TOTP" => fl!("field-totp"),
        "TOTP Seed" => fl!("field-totp-seed"),
        "Private Key" => fl!("field-private-key"),
        "Public Key" => fl!("field-public-key"),
        "Card Number" => fl!("field-card-number"),
        "Cardholder" => fl!("field-cardholder"),
        "Brand" => fl!("field-brand"),
        "Email" => fl!("field-email"),
        "SSN" => fl!("field-ssn"),
        // New-item types (Bitwarden v2026.7.0): bank account / driver's
        // license / passport. Keys stay stable literals (edit dispatch and
        // reveal-state matching); only the display is localized.
        "Bank Name" => fl!("field-bank-name"),
        "Name on Account" => fl!("field-name-on-account"),
        "Account Type" => fl!("field-account-type"),
        "Account Number" => fl!("field-account-number"),
        "Routing Number" => fl!("field-routing-number"),
        "Branch Number" => fl!("field-branch-number"),
        "PIN" => fl!("field-pin"),
        "SWIFT Code" => fl!("field-swift-code"),
        "IBAN" => fl!("field-iban"),
        "Bank Contact Phone" => fl!("field-bank-contact-phone"),
        "First Name" => fl!("field-first-name"),
        "Middle Name" => fl!("field-middle-name"),
        "Last Name" => fl!("field-last-name"),
        "Date of Birth" => fl!("field-date-of-birth"),
        "License Number" => fl!("field-license-number"),
        "Issuing Country" => fl!("field-issuing-country"),
        "Issuing State" => fl!("field-issuing-state"),
        "Issue Date" => fl!("field-issue-date"),
        "Expiration Date" => fl!("field-expiration-date"),
        "Issuing Authority" => fl!("field-issuing-authority"),
        "License Class" => fl!("field-license-class"),
        "Surname" => fl!("field-surname"),
        "Given Name" => fl!("field-given-name"),
        "Sex" => fl!("field-sex"),
        "Birth Place" => fl!("field-birth-place"),
        "Nationality" => fl!("field-nationality"),
        "Passport Number" => fl!("field-passport-number"),
        "Passport Type" => fl!("field-passport-type"),
        "National Identification Number" => fl!("field-national-identification-number"),
        other => other.to_string(),
    }
}

impl CosmicBWardenApp {
    pub fn view_entry_details<'a>(&'a self, entry: &'a Entry) -> Element<'a, Message> {
        let is_editing = self.editing_entry.is_some();

        let header = cosmic::widget::row::with_capacity(3)
            .spacing(10)
            .align_y(Alignment::Center)
            .push(if !is_editing {
                let is_pinned = self
                    .entries
                    .iter()
                    .find(|e| e.id == entry.id)
                    .map(|e| e.is_pinned)
                    .unwrap_or(false);
                let icon_name = if is_pinned {
                    "starred-symbolic"
                } else {
                    "non-starred-symbolic"
                };
                Element::from(
                    button::icon(icon::from_name(icon_name))
                        .on_press(Message::TogglePin(entry.id.clone())),
                )
            } else {
                Element::from(cosmic::widget::Space::new().width(Length::Fixed(40.0)))
            })
            .push(if is_editing {
                Element::from(
                    cosmic::widget::text_input::text_input(
                        fl!("name"),
                        &self.editing_entry.as_ref().unwrap().name,
                    )
                    .on_input(Message::EditNameChanged)
                    .width(Length::Fill),
                )
            } else {
                text::title2(&entry.name).width(Length::Fill).into()
            })
            .push(if is_editing {
                Element::from(
                    cosmic::widget::row::with_capacity(2)
                        .spacing(5)
                        .push(button::suggested(fl!("save")).on_press(Message::SaveEdit))
                        .push(button::standard(fl!("cancel")).on_press(Message::CancelEdit)),
                )
            } else {
                Element::from(button::suggested(fl!("edit")).on_press(Message::EditEntry))
            });

        let mut fields_col = cosmic::widget::column::with_capacity(5).spacing(10);

        if is_editing {
            let editing = self.editing_entry.as_ref().unwrap();
            let is_new = cosmic_bwarden_core::protocol::entry_save::is_new(editing);

            if is_new {
                let entry_type_selector = cosmic::widget::row::with_capacity(3)
                    .spacing(10)
                    .push(
                        button::standard(fl!("entry-type-login"))
                            .on_press(Message::NewEntryTypeChanged(EntryType::Login)),
                    )
                    .push(
                        button::standard(fl!("entry-type-note"))
                            .on_press(Message::NewEntryTypeChanged(EntryType::SecureNote)),
                    )
                    .push(
                        button::standard(fl!("entry-type-ssh-key"))
                            .on_press(Message::NewEntryTypeChanged(EntryType::SshKey)),
                    );
                fields_col = fields_col.push(text::body(fl!("entry-type")));
                fields_col = fields_col.push(entry_type_selector);
                fields_col = fields_col.push(divider::horizontal::default());
            }

            match &editing.data {
                EntryData::Login {
                    username,
                    password,
                    totp,
                    ..
                } => {
                    fields_col = fields_col.push(cosmic_settings::item(
                        field_label("Username"),
                        cosmic::widget::text_input::text_input(
                            field_label("Username"),
                            username.as_deref().unwrap_or(""),
                        )
                        .on_input(|v| Message::EditFieldChanged("Username".to_string(), v)),
                    ));

                    let pw_input = secure_input(
                        field_label("Password"),
                        password.as_ref().map(|s| s.expose()).unwrap_or(""),
                        Some(Message::ToggleEditPasswordReveal),
                        !self.edit_password_revealed,
                    )
                    .on_input(|v| Message::EditFieldChanged("Password".to_string(), v));

                    fields_col =
                        fields_col.push(cosmic_settings::item(field_label("Password"), pw_input));

                    let totp_input = cosmic::widget::text_input::text_input(
                        field_label("TOTP Seed"),
                        totp.as_ref().map(|s| s.expose()).unwrap_or(""),
                    )
                    .on_input(|v| Message::EditFieldChanged("TOTP".to_string(), v));
                    fields_col = fields_col
                        .push(cosmic_settings::item(field_label("TOTP Seed"), totp_input));
                }
                EntryData::SshKey {
                    private_key,
                    public_key,
                    ..
                } => {
                    let pk_input = secure_input(
                        field_label("Private Key"),
                        private_key.as_ref().map(|s| s.expose()).unwrap_or(""),
                        Some(Message::ToggleEditPasswordReveal),
                        !self.edit_password_revealed,
                    )
                    .on_input(|v| Message::EditFieldChanged("Private Key".to_string(), v));

                    fields_col = fields_col
                        .push(cosmic_settings::item(field_label("Private Key"), pk_input));

                    fields_col = fields_col.push(cosmic_settings::item(
                        field_label("Public Key"),
                        cosmic::widget::text_input::text_input(
                            field_label("Public Key"),
                            public_key.as_deref().unwrap_or(""),
                        )
                        .on_input(|v| Message::EditFieldChanged("Public Key".to_string(), v)),
                    ));
                }
                _ => {}
            }
        } else {
            match &entry.data {
                EntryData::Login {
                    username,
                    password,
                    ..
                } => {
                    if let Some(u) = username {
                        fields_col =
                            fields_col.push(self.view_field("Username", u, &entry.id, false));
                    }
                    fields_col = fields_col.push(self.view_field(
                        "Password",
                        password.as_ref().map(|s| s.expose()).unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    fields_col = fields_col.push(self.view_field(
                        "TOTP",
                        self.totp_code.as_deref().unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                }
                EntryData::SshKey {
                    private_key,
                    public_key,
                    ..
                } => {
                    fields_col = fields_col.push(self.view_field(
                        "Private Key",
                        private_key.as_ref().map(|s| s.expose()).unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    if let Some(pubk) = public_key {
                        fields_col =
                            fields_col.push(self.view_field("Public Key", pubk, &entry.id, false));
                    }
                }
                EntryData::Card {
                    number,
                    cardholder_name,
                    brand,
                    ..
                } => {
                    fields_col = fields_col.push(self.view_field(
                        "Card Number",
                        number.as_ref().map(|s| s.expose()).unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    if let Some(c) = cardholder_name {
                        fields_col =
                            fields_col.push(self.view_field("Cardholder", c, &entry.id, false));
                    }
                    if let Some(b) = brand {
                        fields_col = fields_col.push(self.view_field("Brand", b, &entry.id, false));
                    }
                }
                EntryData::Identity {
                    username,
                    email,
                    ssn,
                    license_number,
                    passport_number,
                    ..
                } => {
                    if let Some(u) = username {
                        fields_col =
                            fields_col.push(self.view_field("Username", u, &entry.id, false));
                    }
                    if let Some(e) = email {
                        fields_col = fields_col.push(self.view_field("Email", e, &entry.id, false));
                    }
                    fields_col = fields_col.push(self.view_field(
                        "SSN",
                        ssn.as_deref().unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    fields_col = fields_col.push(self.view_field(
                        "License Number",
                        license_number.as_deref().unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    fields_col = fields_col.push(self.view_field(
                        "Passport Number",
                        passport_number.as_deref().unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                }
                EntryData::BankAccount {
                    bank_name,
                    name_on_account,
                    account_type,
                    account_number,
                    routing_number,
                    branch_number,
                    pin,
                    swift_code,
                    iban,
                    bank_contact_phone,
                } => {
                    if let Some(v) = bank_name {
                        fields_col =
                            fields_col.push(self.view_field("Bank Name", v, &entry.id, false));
                    }
                    if let Some(v) = name_on_account {
                        fields_col = fields_col.push(self.view_field(
                            "Name on Account",
                            v,
                            &entry.id,
                            false,
                        ));
                    }
                    if let Some(v) = account_type {
                        fields_col =
                            fields_col.push(self.view_field("Account Type", v, &entry.id, false));
                    }
                    fields_col = fields_col.push(self.view_field(
                        "Account Number",
                        account_number.as_ref().map(|s| s.expose()).unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    fields_col = fields_col.push(self.view_field(
                        "Routing Number",
                        routing_number.as_ref().map(|s| s.expose()).unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    fields_col = fields_col.push(self.view_field(
                        "Branch Number",
                        branch_number.as_deref().unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    fields_col = fields_col.push(self.view_field(
                        "PIN",
                        pin.as_ref().map(|s| s.expose()).unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    fields_col = fields_col.push(self.view_field(
                        "SWIFT Code",
                        swift_code.as_deref().unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    fields_col = fields_col.push(self.view_field(
                        "IBAN",
                        iban.as_deref().unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    if let Some(v) = bank_contact_phone {
                        fields_col = fields_col.push(self.view_field(
                            "Bank Contact Phone",
                            v,
                            &entry.id,
                            false,
                        ));
                    }
                }
                EntryData::DriversLicense {
                    first_name,
                    middle_name,
                    last_name,
                    date_of_birth,
                    license_number,
                    issuing_country,
                    issuing_state,
                    issue_date,
                    expiration_date,
                    issuing_authority,
                    license_class,
                } => {
                    if let Some(v) = first_name {
                        fields_col =
                            fields_col.push(self.view_field("First Name", v, &entry.id, false));
                    }
                    if let Some(v) = middle_name {
                        fields_col =
                            fields_col.push(self.view_field("Middle Name", v, &entry.id, false));
                    }
                    if let Some(v) = last_name {
                        fields_col =
                            fields_col.push(self.view_field("Last Name", v, &entry.id, false));
                    }
                    if let Some(v) = date_of_birth {
                        fields_col =
                            fields_col.push(self.view_field("Date of Birth", v, &entry.id, false));
                    }
                    fields_col = fields_col.push(self.view_field(
                        "License Number",
                        license_number.as_ref().map(|s| s.expose()).unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    if let Some(v) = issuing_country {
                        fields_col = fields_col.push(self.view_field(
                            "Issuing Country",
                            v,
                            &entry.id,
                            false,
                        ));
                    }
                    if let Some(v) = issuing_state {
                        fields_col =
                            fields_col.push(self.view_field("Issuing State", v, &entry.id, false));
                    }
                    if let Some(v) = issue_date {
                        fields_col =
                            fields_col.push(self.view_field("Issue Date", v, &entry.id, false));
                    }
                    if let Some(v) = expiration_date {
                        fields_col = fields_col.push(self.view_field(
                            "Expiration Date",
                            v,
                            &entry.id,
                            false,
                        ));
                    }
                    if let Some(v) = issuing_authority {
                        fields_col = fields_col.push(self.view_field(
                            "Issuing Authority",
                            v,
                            &entry.id,
                            false,
                        ));
                    }
                    if let Some(v) = license_class {
                        fields_col =
                            fields_col.push(self.view_field("License Class", v, &entry.id, false));
                    }
                }
                EntryData::Passport {
                    surname,
                    given_name,
                    date_of_birth,
                    sex,
                    birth_place,
                    nationality,
                    issuing_country,
                    passport_number,
                    passport_type,
                    national_identification_number,
                    issuing_authority,
                    issue_date,
                    expiration_date,
                } => {
                    if let Some(v) = surname {
                        fields_col =
                            fields_col.push(self.view_field("Surname", v, &entry.id, false));
                    }
                    if let Some(v) = given_name {
                        fields_col =
                            fields_col.push(self.view_field("Given Name", v, &entry.id, false));
                    }
                    fields_col = fields_col.push(self.view_field(
                        "Date of Birth",
                        date_of_birth.as_deref().unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    if let Some(v) = sex {
                        fields_col = fields_col.push(self.view_field("Sex", v, &entry.id, false));
                    }
                    if let Some(v) = birth_place {
                        fields_col =
                            fields_col.push(self.view_field("Birth Place", v, &entry.id, false));
                    }
                    if let Some(v) = nationality {
                        fields_col =
                            fields_col.push(self.view_field("Nationality", v, &entry.id, false));
                    }
                    if let Some(v) = issuing_country {
                        fields_col = fields_col.push(self.view_field(
                            "Issuing Country",
                            v,
                            &entry.id,
                            false,
                        ));
                    }
                    fields_col = fields_col.push(self.view_field(
                        "Passport Number",
                        passport_number.as_ref().map(|s| s.expose()).unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    if let Some(v) = passport_type {
                        fields_col =
                            fields_col.push(self.view_field("Passport Type", v, &entry.id, false));
                    }
                    fields_col = fields_col.push(self.view_field(
                        "National Identification Number",
                        national_identification_number.as_deref().unwrap_or(""),
                        &entry.id,
                        true,
                    ));
                    if let Some(v) = issuing_authority {
                        fields_col = fields_col.push(self.view_field(
                            "Issuing Authority",
                            v,
                            &entry.id,
                            false,
                        ));
                    }
                    if let Some(v) = issue_date {
                        fields_col =
                            fields_col.push(self.view_field("Issue Date", v, &entry.id, false));
                    }
                    if let Some(v) = expiration_date {
                        fields_col = fields_col.push(self.view_field(
                            "Expiration Date",
                            v,
                            &entry.id,
                            false,
                        ));
                    }
                }
                _ => {}
            }

            for field in &entry.fields {
                if let Some(name) = &field.name {
                    let is_hidden = field.ty == Some(cosmic_bwarden_core::api::FieldType::Hidden);
                    if is_hidden || field.value.is_some() {
                        fields_col = fields_col.push(self.view_field(
                            name,
                            field.value.as_ref().map(|v| v.expose()).unwrap_or(""),
                            &entry.id,
                            is_hidden,
                        ));
                    }
                }
            }
        }

        let is_login_or_ssh = matches!(
            if is_editing {
                &self.editing_entry.as_ref().unwrap().data
            } else {
                &entry.data
            },
            EntryData::Login { .. } | EntryData::SshKey { .. }
        );

        let mut col = cosmic::widget::column::with_capacity(3).spacing(20);
        if is_login_or_ssh {
            let card = cosmic::widget::column::with_capacity(2)
                .spacing(20)
                .push(header)
                .push(fields_col);
            col = col.push(
                container(card)
                    .padding(15)
                    .class(cosmic::theme::Container::Primary),
            );
        } else {
            col = col.push(header);
            col = col.push(fields_col);
        }
        col = col.push(text::body(fl!("notes")));

        let notes_editor = cosmic::widget::text_editor::TextEditor::new(&self.notes_content)
            .on_action(Message::NotesAction)
            .height(Length::Fill);
        let notes_container = container(notes_editor)
            .height(Length::Fill)
            .padding(5)
            .class(cosmic::theme::Container::Background);

        let mut outer = cosmic::widget::column::with_capacity(4)
            .spacing(10)
            .height(Length::Fill)
            .padding(20);
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
                button::destructive(fl!("delete-entry"))
                    .on_press(Message::DeleteEntry(entry.id.clone()))
                    .width(Length::Fill)
                    .into()
            };
            outer = outer.push(delete_area);
        }

        outer = outer.push(
            button::custom(
                text::caption(fl!("entry-id", id = entry.id.clone())).class(muted_text()),
            )
            .on_press(Message::CopyToClipboard(entry.id.clone()))
            .class(cosmic::theme::Button::Text)
            .padding([2, 0]),
        );

        outer.into()
    }

    pub fn view_field<'a>(
        &'a self,
        label: &'a str,
        value: &'a str,
        entry_id: &str,
        is_hidden: bool,
    ) -> Element<'a, Message> {
        let is_password = is_hidden
            || label.to_lowercase().contains("password")
            || label.to_lowercase().contains("key")
            || label == "TOTP";
        let is_revealed = self
            .revealed_fields
            .contains(&(entry_id.to_string(), label.to_string()));

        let mut row = cosmic::widget::row::with_capacity(3)
            .spacing(10)
            .align_y(Alignment::Center);
        row = row.push(text::body(field_label(label)).width(Length::Fixed(100.0)));

        if is_password {
            let pw_input = secure_input(
                "",
                value,
                Some(Message::ToggleRevealField(
                    entry_id.to_string(),
                    label.to_string(),
                )),
                !is_revealed,
            )
            .width(Length::Fill);
            row = row.push(pw_input);
        } else {
            row = row.push(text::body(value).width(Length::Fill));
        }

        row = row.push(
            button::icon(icon::from_name("edit-copy-symbolic")).on_press(if is_password {
                Message::CopySecretField(entry_id.to_string(), label.to_string())
            } else {
                Message::CopyToClipboard(value.to_string())
            }),
        );
        container(row).padding(5).into()
    }
}
