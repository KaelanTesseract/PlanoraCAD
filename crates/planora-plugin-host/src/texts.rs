//! Die Texte eines Plug-ins: Fluent-Dateien aus `languages/`, abgefragt über `i18n.text`.
//!
//! Gesucht wird erst in der gewünschten Sprache (auch nur mit dem Sprachteil, `de-AT` → `de`),
//! dann in `en`, zuletzt gibt es den Schlüssel selbst zurück.

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use std::collections::BTreeMap;
use unic_langid::LanguageIdentifier;

/// Die Texte aller Sprachen eines Plug-ins.
pub struct PluginTexts {
    bundles: BTreeMap<String, FluentBundle<FluentResource>>,
}

impl PluginTexts {
    /// Liest die Sprachdateien (Schlüssel: Sprach-Id, Wert: Inhalt der `.ftl`-Datei). Dateien mit
    /// unbrauchbarem Namen werden übergangen, Fehler in einer Datei kosten nur die betroffenen Einträge.
    pub fn new(languages: &BTreeMap<String, String>) -> PluginTexts {
        let mut bundles = BTreeMap::new();
        for (id, source) in languages {
            let Ok(langid) = id.parse::<LanguageIdentifier>() else {
                continue;
            };
            let resource = match FluentResource::try_new(source.clone()) {
                Ok(r) => r,
                Err((r, _)) => r,
            };
            let mut bundle = FluentBundle::new_concurrent(vec![langid]);
            // Ohne Isolationszeichen um eingesetzte Werte, damit der Text sauber bleibt.
            bundle.set_use_isolating(false);
            // Doppelte Schlüssel in einer Datei: der erste gilt, der Rest wird übergangen.
            let _ = bundle.add_resource(resource);
            bundles.insert(id.clone(), bundle);
        }
        PluginTexts { bundles }
    }

    fn format(&self, lang: &str, key: &str, args: &FluentArgs) -> Option<String> {
        let bundle = self.bundles.get(lang)?;
        let pattern = bundle.get_message(key)?.value()?;
        let mut errors = Vec::new();
        Some(
            bundle
                .format_pattern(pattern, Some(args), &mut errors)
                .into_owned(),
        )
    }

    /// Der Text zu `key` in der Sprache `lang` (Sprache → `en` → Schlüssel selbst).
    pub fn text(&self, lang: &str, key: &str, args: &[(String, String)]) -> String {
        let mut fluent_args = FluentArgs::new();
        for (name, value) in args {
            fluent_args.set(name.as_str(), value.as_str());
        }
        let language_part = lang.split(['-', '_']).next().unwrap_or(lang);
        [lang, language_part, "en"]
            .into_iter()
            .find_map(|l| self.format(l, key, &fluent_args))
            .unwrap_or_else(|| key.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts() -> PluginTexts {
        let mut l = BTreeMap::new();
        l.insert(
            "de".to_string(),
            "gruss = Hallo, { $name }!\nnur-de = Nur deutsch\n".to_string(),
        );
        l.insert(
            "en".to_string(),
            "gruss = Hello, { $name }!\nnur-en = English only\n".to_string(),
        );
        PluginTexts::new(&l)
    }

    fn name(n: &str) -> Vec<(String, String)> {
        vec![("name".to_string(), n.to_string())]
    }

    #[test]
    fn language_then_english_then_key() {
        let t = texts();
        assert_eq!(t.text("de", "gruss", &name("Ann")), "Hallo, Ann!");
        assert_eq!(t.text("de-AT", "gruss", &name("Ann")), "Hallo, Ann!");
        assert_eq!(t.text("fr", "gruss", &name("Ann")), "Hello, Ann!");
        assert_eq!(t.text("de", "nur-en", &[]), "English only");
        assert_eq!(t.text("de", "fehlt", &[]), "fehlt");
    }
}
