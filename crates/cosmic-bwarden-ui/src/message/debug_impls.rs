//! Manual `Debug` that never prints secret payloads. Derived `Debug` would
//! leak clipboard copies, master-password / PIN form buffers, generator
//! output, and decrypted `Entry`s into iced debug tooling.

use super::Message;

impl Message {
    fn variant_name(&self) -> &'static str {
        match self {
            Self::ConfigReceived(_) => "ConfigReceived",
            Self::WindowClosed(_) => "WindowClosed",
            Self::RefreshStateInternal => "RefreshStateInternal",
            Self::AppletIconClicked(_, _) => "AppletIconClicked",
            Self::Exit => "Exit",
            Self::LockAndQuit => "LockAndQuit",
            Self::LogoutAndQuit => "LogoutAndQuit",
            Self::EmailChanged(_) => "EmailChanged",
            Self::PasswordChanged(_) => "PasswordChanged",
            Self::ServerChanged(_) => "ServerChanged",
            Self::RememberChanged(_) => "RememberChanged",
            Self::VerificationCodeChanged(_) => "VerificationCodeChanged",
            Self::LoginSubmitted => "LoginSubmitted",
            Self::UnlockPasswordChanged(_) => "UnlockPasswordChanged",
            Self::UnlockSubmitted => "UnlockSubmitted",
            Self::UnlockPinChanged(_) => "UnlockPinChanged",
            Self::UnlockPinRevealToggled => "UnlockPinRevealToggled",
            Self::SearchChanged(_) => "SearchChanged",
            Self::FilterTabActivated(_) => "FilterTabActivated",
            Self::PaneResized(_) => "PaneResized",
            Self::SelectEntry(_) => "SelectEntry",
            Self::EntryReceived(_) => "EntryReceived",
            Self::AddEntryRequested => "AddEntryRequested",
            Self::EditEntry => "EditEntry",
            Self::EditEntryLoaded(_) => "EditEntryLoaded",
            Self::CancelEdit => "CancelEdit",
            Self::SaveEdit => "SaveEdit",
            Self::EditFieldChanged(_, _) => "EditFieldChanged",
            Self::EditNameChanged(_) => "EditNameChanged",
            Self::EntriesReceived(_, _) => "EntriesReceived",
            Self::CopyToClipboard(_) => "CopyToClipboard",
            Self::CopySecretField(_, _) => "CopySecretField",
            Self::OnDemandSecretLoaded { .. } => "OnDemandSecretLoaded",
            Self::ClipboardClearElapsed(_) => "ClipboardClearElapsed",
            Self::ClipboardClearReadback(_, _) => "ClipboardClearReadback",
            Self::NotesAction(_) => "NotesAction",
            Self::DeleteEntry(_) => "DeleteEntry",
            Self::DeleteEntryResult(_) => "DeleteEntryResult",
            Self::SaveEditResult(_) => "SaveEditResult",
            Self::ConfirmDelete => "ConfirmDelete",
            Self::CancelDelete => "CancelDelete",
            Self::RepromptPasswordChanged(_) => "RepromptPasswordChanged",
            Self::SubmitReprompt => "SubmitReprompt",
            Self::CancelReprompt => "CancelReprompt",
            Self::NewEntryTypeChanged(_) => "NewEntryTypeChanged",
            Self::ToggleAdvanced => "ToggleAdvanced",
            Self::Surface(_) => "Surface",
            Self::OpenVaultRequested => "OpenVaultRequested",
            Self::Token(_) => "Token",
            Self::AppletUnlockPasswordChanged(_) => "AppletUnlockPasswordChanged",
            Self::AppletUnlockSubmitted => "AppletUnlockSubmitted",
            Self::AppletUnlockResult(_) => "AppletUnlockResult",
            Self::AppletSearchChanged(_) => "AppletSearchChanged",
            Self::AppletToggleFavouritesFilter => "AppletToggleFavouritesFilter",
            Self::AppletSearchResultsReceived(_, _) => "AppletSearchResultsReceived",
            Self::AppletCopyPrimary(_) => "AppletCopyPrimary",
            Self::AppletCopySecret(_) => "AppletCopySecret",
            Self::AppletSecretReceived(_) => "AppletSecretReceived",
            Self::AppletOpenInVault(_) => "AppletOpenInVault",
            Self::AppletOpenLink(_) => "AppletOpenLink",
            Self::AppletQuitMenuToggle => "AppletQuitMenuToggle",
            Self::AppletRepromptPasswordChanged(_) => "AppletRepromptPasswordChanged",
            Self::AppletRepromptSubmitted => "AppletRepromptSubmitted",
            Self::AppletRepromptCancelled => "AppletRepromptCancelled",
            Self::ProtocolVersionCheck(_) => "ProtocolVersionCheck",
            Self::AppletToggleUnlockPasswordReveal => "AppletToggleUnlockPasswordReveal",
            Self::AppletToggleRepromptPasswordReveal => "AppletToggleRepromptPasswordReveal",
            Self::CloseToast(_) => "CloseToast",
            Self::ToggleRevealField(_, _) => "ToggleRevealField",
            Self::ToggleMasterPasswordReveal => "ToggleMasterPasswordReveal",
            Self::SettingsViewClicked => "SettingsViewClicked",
            Self::AuthResult(_) => "AuthResult",
            Self::LockResult => "LockResult",
            Self::LogoutResult => "LogoutResult",
            Self::LockClicked => "LockClicked",
            Self::LogoutClicked => "LogoutClicked",
            Self::SyncClicked => "SyncClicked",
            Self::SyncResult(_) => "SyncResult",
            Self::EventReceived(_) => "EventReceived",
            Self::TogglePin(_) => "TogglePin",
            Self::ToggleSearchPinned => "ToggleSearchPinned",
            Self::ToggleEditPasswordReveal => "ToggleEditPasswordReveal",
            Self::ToggleRepromptPasswordReveal => "ToggleRepromptPasswordReveal",
            Self::SettingsEditClicked => "SettingsEditClicked",
            Self::SettingsSaveClicked => "SettingsSaveClicked",
            Self::SettingsCancelClicked => "SettingsCancelClicked",
            Self::SettingsServerChanged(_) => "SettingsServerChanged",
            Self::SettingsLockTimeoutChanged(_) => "SettingsLockTimeoutChanged",
            Self::LoginPinEnabledToggled(_) => "LoginPinEnabledToggled",
            Self::LoginPinChanged(_) => "LoginPinChanged",
            Self::LoginPinRevealToggled => "LoginPinRevealToggled",
            Self::AppletPinChanged(_) => "AppletPinChanged",
            Self::AppletPinSubmitted => "AppletPinSubmitted",
            Self::AppletPinResult(_) => "AppletPinResult",
            Self::AppletTogglePinReveal => "AppletTogglePinReveal",
            Self::AppletUseMasterPasswordInstead => "AppletUseMasterPasswordInstead",
            Self::MainWindowPinChanged(_) => "MainWindowPinChanged",
            Self::MainWindowPinSubmitted => "MainWindowPinSubmitted",
            Self::MainWindowPinResult(_) => "MainWindowPinResult",
            Self::TpmStatusReceived(_) => "TpmStatusReceived",
            Self::TpmDaStatusReceived(_) => "TpmDaStatusReceived",
            Self::TpmDiagnosticsReceived(_) => "TpmDiagnosticsReceived",
            Self::TpmSetupFormToggle => "TpmSetupFormToggle",
            Self::TpmDisableFormToggle => "TpmDisableFormToggle",
            Self::TpmSetupPinChanged(_) => "TpmSetupPinChanged",
            Self::TpmSetupPinRevealToggled => "TpmSetupPinRevealToggled",
            Self::TpmSetupSubmitted => "TpmSetupSubmitted",
            Self::TpmSetupResult(_) => "TpmSetupResult",
            Self::TpmDisableSubmitted => "TpmDisableSubmitted",
            Self::TpmDisableResult(_) => "TpmDisableResult",
            Self::TpmServerCredentialsToggled(_) => "TpmServerCredentialsToggled",
            Self::TpmServerCredentialsResult(_) => "TpmServerCredentialsResult",
            Self::GeneratorViewClicked => "GeneratorViewClicked",
            Self::GeneratorTabActivated(_) => "GeneratorTabActivated",
            Self::GeneratorUppercaseToggled(_) => "GeneratorUppercaseToggled",
            Self::GeneratorLowercaseToggled(_) => "GeneratorLowercaseToggled",
            Self::GeneratorNumbersToggled(_) => "GeneratorNumbersToggled",
            Self::GeneratorSpecialToggled(_) => "GeneratorSpecialToggled",
            Self::GeneratorLengthChanged(_) => "GeneratorLengthChanged",
            Self::GeneratorResetClicked => "GeneratorResetClicked",
            Self::GeneratorGenerateClicked => "GeneratorGenerateClicked",
            Self::GeneratorGenerated(_) => "GeneratorGenerated",
            Self::GeneratorRevealToggled => "GeneratorRevealToggled",
            Self::GeneratorSettingsReceived(_) => "GeneratorSettingsReceived",
            Self::GeneratorHistoryReceived(_) => "GeneratorHistoryReceived",
            Self::GeneratorHistoryRevealToggled(_) => "GeneratorHistoryRevealToggled",
            Self::GeneratorHistoryDeleteRequested(_) => "GeneratorHistoryDeleteRequested",
            Self::GeneratorHistoryDeleteConfirmed => "GeneratorHistoryDeleteConfirmed",
            Self::GeneratorHistoryDeleteCancelled => "GeneratorHistoryDeleteCancelled",
            Self::GeneratorHistoryDeleted(_) => "GeneratorHistoryDeleted",
            Self::AppletGeneratePasswordRequested => "AppletGeneratePasswordRequested",
            Self::AppletGeneratePasswordReceived(_) => "AppletGeneratePasswordReceived",
        }
    }
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SelectEntry(id) => write!(f, "SelectEntry({id:?})"),
            Self::DeleteEntry(id) => write!(f, "DeleteEntry({id:?})"),
            Self::ClipboardClearElapsed(gen) => write!(f, "ClipboardClearElapsed({gen})"),
            Self::SearchChanged(_) => f.write_str("SearchChanged"),
            other => f.write_str(other.variant_name()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Message;

    #[test]
    fn message_debug_never_prints_secrets() {
        let secret = "hunter2-secret";
        let cases = [
            format!("{:?}", Message::PasswordChanged(secret.to_string())),
            format!("{:?}", Message::UnlockPasswordChanged(secret.to_string())),
            format!("{:?}", Message::UnlockPinChanged(secret.to_string())),
            format!("{:?}", Message::CopyToClipboard(secret.to_string())),
            format!(
                "{:?}",
                Message::ClipboardClearReadback(1, Some(secret.to_string()))
            ),
            format!("{:?}", Message::RepromptPasswordChanged(secret.to_string())),
            format!(
                "{:?}",
                Message::EditFieldChanged("password".into(), secret.to_string())
            ),
            format!("{:?}", Message::LoginPinChanged(secret.to_string())),
            format!("{:?}", Message::AppletPinChanged(secret.to_string())),
            format!("{:?}", Message::MainWindowPinChanged(secret.to_string())),
            format!("{:?}", Message::TpmSetupPinChanged(secret.to_string())),
            format!("{:?}", Message::GeneratorGenerated(Ok(secret.to_string()))),
            format!(
                "{:?}",
                Message::AppletGeneratePasswordReceived(Ok(secret.to_string()))
            ),
            format!(
                "{:?}",
                Message::AppletSecretReceived(Ok(secret.to_string()))
            ),
        ];
        for s in cases {
            assert!(!s.contains(secret), "Message Debug leaked secret: {s}");
        }
        assert_eq!(
            format!("{:?}", Message::PasswordChanged(secret.to_string())),
            "PasswordChanged"
        );
    }
}
