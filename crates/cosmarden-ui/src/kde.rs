//! KDE/Plasma extras for the vault window. Compiled only with `--features kde`.
//!
//! The COSMIC applet and the vault window both run on Plasma; libcosmic's
//! theme and config lookups fail there from the first frame on because Plasma
//! has no COSMIC session D-Bus. Those crates log at `ERROR`, so this module
//! demotes them to `=off` — `just build-kde` produces the binary with this
//! filter, `just build` the default one.
//!
//! The Plasma tray used to live here as a second binary (`cosmarden-tray`,
//! StatusNotifierItem + compact popup). It is retired: the tray is now
//! `cosmarden-plasma-tray`, a separate crate that links no libcosmic. See
//! `docs/kde_applet_parity.md`.

use tracing_subscriber::EnvFilter;

/// Tracing directives that demote libcosmic's missing-session theme
/// errors. They must be `=off`: those crates log at `ERROR`, so `=warn`
/// would still print them.
pub const THEME_NOISE_DIRECTIVES: &[&str] = &[
    "cosmic::theme=off",
    "cosmic::app::cosmic=off",
    "cosmic_config=off",
];

pub fn apply_theme_noise_filter(mut filter: EnvFilter) -> EnvFilter {
    for directive in THEME_NOISE_DIRECTIVES {
        filter = filter.add_directive(directive.parse().expect("static directive"));
    }
    filter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_noise_directives_parse_as_env_filter_layers() {
        let mut filter = EnvFilter::new("warn");
        for directive in THEME_NOISE_DIRECTIVES {
            filter = filter.add_directive(
                directive
                    .parse()
                    .unwrap_or_else(|e| panic!("{directive:?} must parse: {e}")),
            );
        }
        let _ = apply_theme_noise_filter(EnvFilter::new("warn"));
        assert_eq!(THEME_NOISE_DIRECTIVES.len(), 3);
        for directive in THEME_NOISE_DIRECTIVES {
            assert!(
                directive.ends_with("=off"),
                "{directive} must be =off: libcosmic logs these at ERROR"
            );
        }
    }
}
