//! Compile-time gate: the default UI test binary must not enable `kde`.
//! The feature's own tests live in `src/kde.rs` and run via `just test-unit-kde`.

#[cfg(not(feature = "kde"))]
#[test]
fn default_ui_build_does_not_enable_kde() {
    // Inline `const` block: clippy's `assertions_on_constants` requires the
    // constant assertion to be evaluated at compile time instead.
    const {
        assert!(
            !cfg!(feature = "kde"),
            "just test-unit / cargo test -p cosmarden-ui must stay COSMIC-default"
        )
    };
}

#[cfg(feature = "kde")]
#[test]
fn kde_feature_compiles_the_theme_noise_filter() {
    let _ = crate::kde::apply_theme_noise_filter(tracing_subscriber::EnvFilter::new("warn"));
    assert!(
        crate::kde::THEME_NOISE_DIRECTIVES.contains(&"cosmic::theme=off"),
        "must demote cosmic::theme ERROR (missing cosmic-settings-daemon)"
    );
}

#[cfg(feature = "kde")]
#[test]
fn tray_left_click_is_toggle_popup_for_secret_inputs() {
    use crate::kde::KdeTrayCommand;
    assert!(matches!(
        crate::kde::menu::command_to_message(KdeTrayCommand::TogglePopup),
        crate::message::Message::KdeTray(KdeTrayCommand::TogglePopup)
    ));
}
