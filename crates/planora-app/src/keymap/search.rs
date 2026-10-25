//! Suche in der Tastenbelegung für die Einstellungen und die Übersicht: passt eine Aktion zum
//! Suchtext?

use super::{Action, Keymap};

/// Passt `a` zu `filter`? Gesucht wird im Anzeigenamen, im Bereich und im Text der Belegungen
/// (wie in der Tabelle, etwa „Strg+S“), ohne Rücksicht auf Groß- und Kleinschreibung. Mehrere
/// Wörter müssen alle vorkommen (in beliebigen Spalten); ein leerer Suchtext passt immer.
pub fn matches(a: Action, km: &Keymap, filter: &str) -> bool {
    let hay = format!("{}\n{}\n{}", a.label(), a.area().label(), km.text(a)).to_lowercase();
    filter
        .to_lowercase()
        .split_whitespace()
        .all(|word| hay.contains(word))
}
