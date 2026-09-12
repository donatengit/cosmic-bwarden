//! Pure PIN-unlock authorization flags. Always compiled so the checks can be
//! unit-tested without `--features tpm`.

/// Flags, not leftover blob files, authorize PIN unlock.
/// Called from `handle_unlock_with_pin` when the crate is built with `tpm`.
#[cfg_attr(not(feature = "tpm"), allow(dead_code))]
pub(crate) fn pin_unlock_blocked_reason(tpm_enabled: bool) -> Option<&'static str> {
    if tpm_enabled {
        None
    } else {
        Some("TPM PIN unlock is not enabled")
    }
}

#[cfg(test)]
mod tests {
    use super::pin_unlock_blocked_reason;

    #[test]
    fn pin_unlock_blocked_when_tpm_disabled() {
        assert_eq!(
            pin_unlock_blocked_reason(false),
            Some("TPM PIN unlock is not enabled")
        );
    }

    #[test]
    fn pin_unlock_allowed_when_tpm_enabled() {
        assert_eq!(pin_unlock_blocked_reason(true), None);
    }
}
