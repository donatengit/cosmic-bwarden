// Manual `Debug` impls that never print secret payloads. Many `Action`/`Response`
// variants carry master passwords, PINs, TOTP secrets, card data, and generated
// passwords; the derived `Debug` would leak them into logs (`handler.rs`,
// `main.rs`, `browser_host.rs`) at info/debug. Kept in a sibling module to the
// enum definitions purely to keep `protocol.rs` within the project's file-size
// guidelines — no behavioral difference from being inline.

use super::{Action, Response};
use crate::db::Entry;

// Manual `Debug` that never prints field values. Many `Action` variants carry
// master passwords, PINs, TOTP secrets, and card data; the derived `Debug` would
// leak them into logs (`handler.rs`, `main.rs`, `browser_host.rs`) at info/debug.
impl std::fmt::Debug for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            // Non-secret scalar fields are safe and useful for debugging.
            Self::GetEntry { id, .. } => write!(f, "GetEntry {{ id: {id:?} }}"),
            Self::GetEntryMeta { id } => write!(f, "GetEntryMeta {{ id: {id:?} }}"),
            Self::GetPassword { id, .. } => write!(f, "GetPassword {{ id: {id:?} }}"),
            Self::GetTotp { id, .. } => write!(f, "GetTotp {{ id: {id:?} }}"),
            Self::DeleteEntry { id } => write!(f, "DeleteEntry {{ id: {id:?} }}"),
            Self::PinEntry { id } => write!(f, "PinEntry {{ id: {id:?} }}"),
            Self::UnpinEntry { id } => write!(f, "UnpinEntry {{ id: {id:?} }}"),
            Self::SetPendingEntry { id } => write!(f, "SetPendingEntry {{ id: {id:?} }}"),
            Self::UpdateLockTimeout { seconds } => {
                write!(f, "UpdateLockTimeout {{ seconds: {seconds} }}")
            }
            Self::GeneratePassword { settings } => {
                write!(f, "GeneratePassword {{ settings: {settings:?} }}")
            }
            Self::DeleteGeneratedPassword { created_at } => {
                write!(f, "DeleteGeneratedPassword {{ created_at: {created_at} }}")
            }
            // Everything else: variant name only.
            other => f.write_str(other.variant_name()),
        }
    }
}

// Manual `Debug` that never prints secret payloads. `Password`, `Totp`, `Entry`,
// `Entries`, `GeneratedPassword`, and `PasswordHistory` carry decrypted secrets;
// the derived `Debug` would leak them into logs. Non-secret variants (errors,
// versions, flags) print their useful fields.
impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ack => f.write_str("Ack"),
            Self::Error { message } => write!(f, "Error {{ message: {message:?} }}"),
            Self::TwoFactorRequired { providers, .. } => {
                write!(
                    f,
                    "TwoFactorRequired {{ providers: {providers:?}, token: <redacted> }}"
                )
            }
            Self::NewDeviceVerificationRequired => f.write_str("NewDeviceVerificationRequired"),
            Self::Config {
                needs_login,
                has_account,
                is_locked,
                sync_failed,
                lock_epoch,
                ..
            } => write!(
                f,
                "Config {{ needs_login: {needs_login}, has_account: {has_account}, \
                 is_locked: {is_locked}, sync_failed: {sync_failed}, lock_epoch: {lock_epoch} }}"
            ),
            Self::Entries { entries } => {
                write!(f, "Entries {{ count: {}, <redacted> }}", entries.len())
            }
            Self::SidebarEntries { entries } => {
                write!(f, "SidebarEntries {{ count: {} }}", entries.len())
            }
            Self::Entry { .. } => f.write_str("Entry { <redacted> }"),
            Self::EntryMeta {
                entry,
                filled_secrets,
                ..
            } => write!(
                f,
                "EntryMeta {{ secrets_in_entry: {}, filled_secrets: {filled_secrets:?} }}",
                meta_secret_count(entry)
            ),
            Self::Password { .. } => f.write_str("Password { <redacted> }"),
            Self::Totp { .. } => f.write_str("Totp { <redacted> }"),
            Self::Version {
                version,
                protocol_version,
            } => write!(
                f,
                "Version {{ version: {version:?}, protocol_version: {protocol_version:?} }}"
            ),
            Self::Event { event } => write!(f, "Event {{ event: {event:?} }}"),
            Self::TpmStatus {
                available,
                configured,
            } => write!(
                f,
                "TpmStatus {{ available: {available}, configured: {configured} }}"
            ),
            Self::TpmDiagnostics { checks } => {
                write!(f, "TpmDiagnostics {{ checks: {} }}", checks.len())
            }
            Self::TpmDaStatus { status } => write!(f, "TpmDaStatus {{ {status:?} }}"),
            Self::LoginMatch {
                entry_id,
                password_matches,
                ..
            } => write!(
                f,
                "LoginMatch {{ entry_id: {entry_id:?}, password_matches: {password_matches} }}"
            ),
            Self::GeneratedPassword { .. } => f.write_str("GeneratedPassword { <redacted> }"),
            Self::GeneratorSettings { settings } => {
                write!(f, "GeneratorSettings {{ settings: {settings:?} }}")
            }
            Self::PasswordHistory { entries } => {
                write!(
                    f,
                    "PasswordHistory {{ count: {}, <redacted> }}",
                    entries.len()
                )
            }
        }
    }
}

/// Count how many secret slots in `entry` are still populated. Used only for
/// the `EntryMeta` Debug arm to convey, without printing any value, whether the
/// caller re-sent secret-bearing data (which would contradict the redaction
/// contract) versus the normal fully-redacted meta payload.
fn meta_secret_count(entry: &Entry) -> usize {
    use crate::db::EntryData;
    let mut n = usize::from(entry.notes.is_some());
    n += entry
        .fields
        .iter()
        .filter(|f| {
            // Only hidden (user-designated secret) custom fields count as secret
            // slots; visible Text values already travel in the redacted entry.
            f.ty == Some(crate::api::FieldType::Hidden) && f.value.is_some()
        })
        .count();
    match &entry.data {
        EntryData::Login { password, totp, .. } => {
            n += usize::from(password.is_some()) + usize::from(totp.is_some())
        }
        EntryData::Card { number, code, .. } => {
            n += usize::from(number.is_some()) + usize::from(code.is_some())
        }
        EntryData::SshKey { private_key, .. } => n += usize::from(private_key.is_some()),
        EntryData::BankAccount {
            account_number,
            routing_number,
            branch_number,
            pin,
            swift_code,
            iban,
            ..
        } => {
            n += usize::from(account_number.is_some())
                + usize::from(routing_number.is_some())
                + usize::from(branch_number.is_some())
                + usize::from(pin.is_some())
                + usize::from(swift_code.is_some())
                + usize::from(iban.is_some())
        }
        EntryData::DriversLicense { license_number, .. } => {
            n += usize::from(license_number.is_some())
        }
        EntryData::Passport {
            passport_number,
            national_identification_number,
            date_of_birth,
            ..
        } => {
            n += usize::from(passport_number.is_some())
                + usize::from(national_identification_number.is_some())
                + usize::from(date_of_birth.is_some())
        }
        EntryData::Identity {
            ssn,
            license_number,
            passport_number,
            ..
        } => {
            n += usize::from(ssn.is_some())
                + usize::from(license_number.is_some())
                + usize::from(passport_number.is_some())
        }
        EntryData::SecureNote => {}
    }
    n
}
