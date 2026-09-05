pub mod add;
pub mod browser_save;
pub mod merge;
pub mod ops;
pub mod query;
pub mod sync;
mod totp;

use crate::state::State;
use cosmic_bwarden_core::protocol::{Action, Response};
use std::sync::Arc;
use tokio::sync::Mutex;

pub async fn handle_request(action: Action, state: &Arc<Mutex<State>>) -> Response {
    match action {
        Action::Sync => sync::handle_sync(state).await,
        Action::GetEntries {
            query,
            entry_type,
            only_pinned,
        } => query::handle_get_entries(query, entry_type, only_pinned, state).await,
        Action::GetSidebarEntries {
            query,
            entry_type,
            only_pinned,
            domain,
        } => query::handle_get_sidebar_entries(query, entry_type, only_pinned, domain, state).await,
        Action::GetEntry { id, password } => query::handle_get_entry(id, password, state).await,
        Action::GetEntryMeta { id } => query::handle_get_entry_meta(id, state).await,
        Action::GetPassword { id, password } => {
            match query::handle_get_entry(id, password, state).await {
                Response::Entry { entry } => {
                    let password = match entry.data {
                        cosmic_bwarden_core::db::EntryData::Login {
                            password: Some(p), ..
                        } => p,
                        cosmic_bwarden_core::db::EntryData::SshKey {
                            private_key: Some(pk),
                            ..
                        } => pk,
                        cosmic_bwarden_core::db::EntryData::Card {
                            number: Some(n), ..
                        } => n,
                        cosmic_bwarden_core::db::EntryData::BankAccount {
                            account_number: Some(n),
                            ..
                        } => n,
                        cosmic_bwarden_core::db::EntryData::DriversLicense {
                            license_number: Some(n),
                            ..
                        } => n,
                        cosmic_bwarden_core::db::EntryData::Passport {
                            passport_number: Some(n),
                            ..
                        } => n,
                        cosmic_bwarden_core::db::EntryData::SecureNote => match &entry.notes {
                            Some(n) => n.clone(),
                            None => {
                                return Response::Error {
                                    message: "entry has no notes".to_string(),
                                }
                            }
                        },
                        _ => {
                            return Response::Error {
                                message: "entry has no password".to_string(),
                            }
                        }
                    };
                    Response::Password { password }
                }
                r => r,
            }
        }
        Action::GetTotp { id, password } => ops::handle_get_totp(id, password, state).await,
        Action::DeleteEntry { id } => ops::handle_delete_entry(id, state).await,
        Action::UpdateEntry { entry } => ops::handle_update_entry(entry, state).await,
        Action::CopyToClipboard { .. } => Response::Error {
            message: "CopyToClipboard not implemented in agent; handle in client".to_string(),
        },
        Action::PinEntry { id } => ops::handle_pin_entry(id, state).await,
        Action::UnpinEntry { id } => ops::handle_unpin_entry(id, state).await,
        Action::AddEntry {
            name,
            entry_type,
            username,
            password,
            notes,
            fields,
            totp,
            uris,
        } => {
            add::handle_add_entry(
                name, entry_type, username, password, notes, fields, totp, uris, state,
            )
            .await
        }
        Action::AddSecureNote {
            name,
            notes,
            fields,
        } => {
            add::handle_add_entry(
                name,
                cosmic_bwarden_core::protocol::EntryType::SecureNote,
                None,
                None,
                Some(notes),
                fields,
                None,
                Vec::new(),
                state,
            )
            .await
        }
        Action::AddSshKey {
            name,
            private_key,
            public_key,
            notes,
            fields,
        } => add::handle_add_ssh_key(name, private_key, public_key, notes, fields, state).await,
        Action::AddCard {
            name,
            cardholder_name,
            brand,
            number,
            exp_month,
            exp_year,
            code,
            notes,
            fields,
        } => {
            add::handle_add_card(
                name,
                cardholder_name,
                brand,
                number,
                exp_month,
                exp_year,
                code,
                notes,
                fields,
                state,
            )
            .await
        }
        Action::AddIdentity {
            name,
            first_name,
            last_name,
            address1,
            city,
            state: state_code,
            postal_code,
            country,
            email,
            phone,
            notes,
            fields,
        } => {
            add::handle_add_identity(
                name,
                first_name,
                last_name,
                address1,
                city,
                state_code,
                postal_code,
                country,
                email,
                phone,
                notes,
                fields,
                state,
            )
            .await
        }
        Action::AddBankAccount {
            name,
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
            notes,
            fields,
        } => {
            add::handle_add_bank_account(
                name,
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
                notes,
                fields,
                state,
            )
            .await
        }
        Action::AddDriversLicense {
            name,
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
            notes,
            fields,
        } => {
            add::handle_add_drivers_license(
                name,
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
                notes,
                fields,
                state,
            )
            .await
        }
        Action::AddPassport {
            name,
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
            notes,
            fields,
        } => {
            add::handle_add_passport(
                name,
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
                notes,
                fields,
                state,
            )
            .await
        }
        Action::CheckLoginMatch {
            domain,
            username,
            password,
        } => browser_save::handle_check_login_match(domain, username, password, state).await,
        Action::UpdateLoginPassword { id, password } => {
            browser_save::handle_update_login_password(id, password, state).await
        }
        _ => Response::Error {
            message: "not implemented in vault handler".to_string(),
        },
    }
}
