//! UI strings through Fluent. Only English is written; translations are left for contributors.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

const EN_FTL: &str = include_str!("../i18n/en.ftl");

pub struct Strings {
    bundle: FluentBundle<FluentResource>,
}

impl Strings {
    pub fn english() -> Self {
        let lang: LanguageIdentifier = "en-US".parse().expect("valid language tag");
        let resource = FluentResource::try_new(EN_FTL.to_owned())
            .unwrap_or_else(|(_, errors)| panic!("en.ftl does not parse: {errors:?}"));
        let mut bundle = FluentBundle::new(vec![lang]);
        // Fluent wraps arguments in Unicode isolation marks by default; egui draws them as boxes.
        bundle.set_use_isolating(false);
        bundle
            .add_resource(resource)
            .unwrap_or_else(|errors| panic!("en.ftl has duplicate messages: {errors:?}"));
        Self { bundle }
    }

    /// Looks up a message without arguments. A missing key renders as the key itself, so a typo shows
    /// up on screen instead of crashing.
    pub fn get(&self, key: &str) -> String {
        self.format(key, &[])
    }

    /// Looks up a message and fills in its arguments. Numbers are passed preformatted, so the UI
    /// controls their precision.
    pub fn format(&self, key: &str, args: &[(&str, &str)]) -> String {
        let Some(pattern) = self.bundle.get_message(key).and_then(|m| m.value()) else {
            return key.to_owned();
        };
        let mut fluent_args = FluentArgs::new();
        for &(name, value) in args {
            fluent_args.set(name, value);
        }
        let mut errors = Vec::new();
        self.bundle
            .format_pattern(pattern, Some(&fluent_args), &mut errors)
            .into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_strings_load() {
        assert_eq!(Strings::english().get("app-title"), "AIC Modeler");
    }

    #[test]
    fn arguments_are_filled_in_without_isolation_marks() {
        let strings = Strings::english();
        assert_eq!(
            strings.format("link-label", &[("flow", "12"), ("belts", "0.4")]),
            "12 /min · 0.4 belts"
        );
    }

    #[test]
    fn missing_key_renders_as_key() {
        assert_eq!(Strings::english().get("no-such-key"), "no-such-key");
    }
}
