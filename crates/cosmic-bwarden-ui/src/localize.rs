// SPDX-License-Identifier: MPL-2.0

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::{DefaultLocalizer, LanguageLoader, Localizer};
use rust_embed::RustEmbed;
use std::sync::LazyLock;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();

    loader
        .load_fallback_language(&Localizations)
        .expect("Error while loading fallback language");

    // Don't wrap interpolated arguments in Unicode bidi-isolation marks
    // (U+2068/U+2069). Our substituted values are short numbers/durations, and
    // the invisible marks otherwise break plain substring checks and render oddly
    // in some terminals.
    loader.set_use_isolating(false);

    loader
});

#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {{
        i18n_embed_fl::fl!($crate::localize::LANGUAGE_LOADER, $message_id)
    }};
    // Forward named Fluent arguments (`name = value`, comma-separated) verbatim
    // as token trees — `expr` fragments can't match `name = value` syntax.
    ($message_id:literal, $($args:tt)*) => {{
        i18n_embed_fl::fl!($crate::localize::LANGUAGE_LOADER, $message_id, $($args)*)
    }};
}

// Get the `Localizer` to be used for localizing this library.
pub fn localizer() -> Box<dyn Localizer> {
    Box::from(DefaultLocalizer::new(&*LANGUAGE_LOADER, &Localizations))
}

pub fn localize() {
    let localizer = localizer();
    let requested_languages = i18n_embed::DesktopLanguageRequester::requested_languages();
    if let Err(error) = localizer.select(&requested_languages) {
        eprintln!("Error while loading language for cosmic-bwarden {}", error);
    }
}
