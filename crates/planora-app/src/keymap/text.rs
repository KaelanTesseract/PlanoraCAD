//! Belegungen als Text: zum Anzeigen (Menüs, Einstellungen, Übersicht) und zum Speichern in der
//! Konfiguration. [`parse_binding`] liest genau das, was [`binding_text`] schreibt (dazu die
//! englischen Tastennamen von egui und Leerzeichen um die Pluszeichen).

use super::{Binding, Gesture, KeyChord, MouseButton};
use egui::Key;

const CTRL: &str = "Strg";
const SHIFT: &str = "Umschalt";
const ALT: &str = "Alt";
const WHEEL: &str = "Mausrad";

/// Deutsche Namen der Tasten, deren egui-Name englisch ist; alle übrigen behalten ihn („E“,
/// „F5“, „1“). Kein Name enthält „+“ (das trennt die Teile, siehe [`parse_binding`]).
const GERMAN_KEYS: [(Key, &str); 38] = [
    (Key::Delete, "Entf"),
    (Key::Space, "Leertaste"),
    (Key::Escape, "Esc"),
    (Key::Enter, "Eingabe"),
    (Key::Backspace, "Rücktaste"),
    (Key::Insert, "Einfg"),
    (Key::Home, "Pos1"),
    (Key::End, "Ende"),
    (Key::PageUp, "Bild auf"),
    (Key::PageDown, "Bild ab"),
    (Key::ArrowLeft, "Pfeil links"),
    (Key::ArrowRight, "Pfeil rechts"),
    (Key::ArrowUp, "Pfeil oben"),
    (Key::ArrowDown, "Pfeil unten"),
    (Key::Tab, "Tab"),
    (Key::Copy, "Kopieren"),
    (Key::Cut, "Ausschneiden"),
    (Key::Paste, "Einfügen"),
    (Key::Colon, "Doppelpunkt"),
    (Key::Comma, "Komma"),
    (Key::Minus, "Minus"),
    (Key::Period, "Punkt"),
    (Key::Plus, "Plus"),
    (Key::Equals, "Gleich"),
    (Key::Semicolon, "Semikolon"),
    (Key::Backslash, "Backslash"),
    (Key::Slash, "Schrägstrich"),
    (Key::Pipe, "Senkrechter Strich"),
    (Key::Questionmark, "Fragezeichen"),
    (Key::Exclamationmark, "Ausrufezeichen"),
    (Key::OpenBracket, "Eckige Klammer auf"),
    (Key::CloseBracket, "Eckige Klammer zu"),
    (Key::OpenCurlyBracket, "Geschweifte Klammer auf"),
    (Key::CloseCurlyBracket, "Geschweifte Klammer zu"),
    (Key::Backtick, "Gravis"),
    (Key::Quote, "Apostroph"),
    (Key::BrowserBack, "Browser zurück"),
    (Key::IntlBackslash, "Taste <>"),
];

/// Umschalttasten als eigene Taste (links, rechts, Windows-Taste): nie belegbar, sie gehören
/// immer zu einer anderen Taste.
pub fn is_modifier_key(key: Key) -> bool {
    matches!(
        key,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::SuperLeft
            | Key::SuperRight
    )
}

pub(crate) fn key_name(key: Key) -> &'static str {
    GERMAN_KEYS
        .iter()
        .find(|(k, _)| *k == key)
        .map_or_else(|| key.name(), |(_, n)| n)
}

fn key_from_name(name: &str) -> Option<Key> {
    GERMAN_KEYS
        .iter()
        .find(|(_, n)| *n == name)
        .map(|(k, _)| *k)
        .or_else(|| Key::from_name(name))
        .filter(|k| !is_modifier_key(*k))
}

fn button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "Linke Taste ziehen",
        MouseButton::Middle => "Mittlere Taste ziehen",
        MouseButton::Right => "Rechte Taste ziehen",
    }
}

fn modifiers(ctrl: bool, shift: bool, alt: bool) -> Vec<&'static str> {
    [(ctrl, CTRL), (shift, SHIFT), (alt, ALT)]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect()
}

/// „Strg+Umschalt+E“, „Entf“, „Leertaste“, „E“.
pub fn chord_text(c: &KeyChord) -> String {
    let mut parts = modifiers(c.ctrl, c.shift, c.alt);
    parts.push(key_name(c.key));
    parts.join("+")
}

/// „Mittlere Taste ziehen“, „Umschalt + Mittlere Taste ziehen“, „Mausrad“.
pub fn gesture_text(g: &Gesture) -> String {
    let (mut parts, what) = match *g {
        Gesture::Drag {
            button,
            ctrl,
            shift,
            alt,
        } => (modifiers(ctrl, shift, alt), button_name(button)),
        Gesture::Wheel { ctrl, shift, alt } => (modifiers(ctrl, shift, alt), WHEEL),
    };
    parts.push(what);
    parts.join(" + ")
}

pub fn binding_text(b: &Binding) -> String {
    match b {
        Binding::Key(c) => chord_text(c),
        Binding::Mouse(g) => gesture_text(g),
    }
}

/// Umkehrung von [`binding_text`]: Umschalttasten (jede höchstens einmal) und zuletzt die
/// Taste oder Geste, mit „+“ getrennt. Alles andere ergibt `None`, auch eine Umschalttaste
/// allein als Taste ([`is_modifier_key`]).
pub fn parse_binding(text: &str) -> Option<Binding> {
    let parts: Vec<&str> = text.split('+').map(str::trim).collect();
    let (last, mods) = parts.split_last()?;
    let (mut ctrl, mut shift, mut alt) = (false, false, false);
    for m in mods {
        let flag = match *m {
            CTRL => &mut ctrl,
            SHIFT => &mut shift,
            ALT => &mut alt,
            _ => return None,
        };
        if *flag {
            return None;
        }
        *flag = true;
    }
    let button = [MouseButton::Left, MouseButton::Middle, MouseButton::Right]
        .into_iter()
        .find(|b| button_name(*b) == *last);
    if let Some(button) = button {
        return Some(Binding::Mouse(Gesture::Drag {
            button,
            ctrl,
            shift,
            alt,
        }));
    }
    if *last == WHEEL {
        return Some(Binding::Mouse(Gesture::Wheel { ctrl, shift, alt }));
    }
    let key = key_from_name(last)?;
    Some(Binding::Key(KeyChord {
        key,
        ctrl,
        shift,
        alt,
    }))
}
