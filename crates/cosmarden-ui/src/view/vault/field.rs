//! Field rows for the entry detail pane: the stable-key → localized-label
//! mapping and the masked row with its reveal/copy controls.
//!
//! Split out of `detail.rs` (which keeps the header and the per-entry-type
//! field assembly) so the pane's row rendering has one home and the notes area
//! (`super::notes`) can sit next to the rows it mirrors.

use crate::app::CosmardenApp;
use crate::fl;
use crate::message::Message;
use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, container, icon, secure_input, text};
use cosmic::Element;

/// Map a stable field *key* (used for edit-dispatch and reveal-state matching) to
/// its localized *display* label. Unknown keys — e.g. user-defined custom field
/// names — are shown verbatim. The key strings themselves must never be localized:
/// `EditFieldChanged` and `revealed_fields` match on them exactly.
pub(super) fn field_label(key: &str) -> String {
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

impl CosmardenApp {
    /// One read-only field row. A secret-class slot (`is_hidden`, or a label
    /// that names a password/key) renders masked with a reveal toggle; its copy
    /// button fetches the value on demand (`CopySecretField`) instead of
    /// copying whatever the redacted entry happens to hold.
    pub(super) fn view_field<'a>(
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
