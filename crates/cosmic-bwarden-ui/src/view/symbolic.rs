//! CosmicDE `-symbolic` names for chrome that used to be emoji.
//!
//! View code must call these helpers (not duplicate the strings) so the
//! mapping is unit-testable. Names come from `docs/icon_guidelines.md` §8.

/// Applet search-row: open this entry in the vault window (was 📂).
pub fn applet_open_in_vault_icon() -> &'static str {
    "preferences-workspaces-symbolic"
}

/// Applet search-row: open the login URI (was 🔗).
pub fn applet_open_link_icon() -> &'static str {
    "window-pop-out-symbolic"
}

/// Applet search-row: copy the secret (was 🔑).
pub fn applet_copy_secret_icon() -> &'static str {
    "network-vpn-symbolic"
}

/// Settings TPM diagnostic row: pass (was ✅) or fail (was ❌).
pub fn tpm_diagnostic_icon(passed: bool) -> &'static str {
    if passed {
        "object-select-symbolic"
    } else {
        "process-stop-symbolic"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applet_row_icons_match_the_documented_cosmic_names() {
        assert_eq!(
            applet_open_in_vault_icon(),
            "preferences-workspaces-symbolic"
        );
        assert_eq!(applet_open_link_icon(), "window-pop-out-symbolic");
        assert_eq!(applet_copy_secret_icon(), "network-vpn-symbolic");
    }

    #[test]
    fn tpm_diagnostic_icon_is_select_when_passed_and_stop_when_failed() {
        assert_eq!(tpm_diagnostic_icon(true), "object-select-symbolic");
        assert_eq!(tpm_diagnostic_icon(false), "process-stop-symbolic");
    }
}
