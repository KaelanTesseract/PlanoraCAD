//! Belegungen als Text: zum Anzeigen (Menüs, Einstellungen, Übersicht; in der Sprache der
//! Oberfläche, [`binding_text`]) und zum Speichern in der Konfiguration (immer die deutschen
//! Namen, [`stored_text`]). [`parse_binding`] liest genau das, was [`stored_text`] schreibt
//! (dazu die englischen Tastennamen von egui und Leerzeichen um die Pluszeichen). Auf Deutsch
//! sind beide Texte gleich.

use super::{Binding, Gesture, KeyChord, MouseButton};
use egui::Key;

const CTRL: &str = "Strg";
const SHIFT: &str = "Umschalt";
const ALT: &str = "Alt";
const WHEEL: &str = "Mausrad";

/// Gespeicherte (deutsche) Namen der Tasten, deren egui-Name englisch ist, und die Id ihres
/// Anzeigenamens in den Sprachdateien; alle übrigen behalten den egui-Namen („E“, „F5“, „1“).
/// Kein Name enthält „+“ (das trennt die Teile, siehe [`parse_binding`]). Der gespeicherte
/// Name gehört in die Konfiguration und ändert sich nie; angezeigt wird der Name der Sprache.
const GERMAN_KEYS: [(Key, &str, &str); 38] = [
    (Key::Delete, "Entf", "key-delete"),
    (Key::Space, "Leertaste", "key-space"),
    (Key::Escape, "Esc", "key-escape"),
    (Key::Enter, "Eingabe", "key-enter"),
    (Key::Backspace, "Rücktaste", "key-backspace"),
    (Key::Insert, "Einfg", "key-insert"),
    (Key::Home, "Pos1", "key-home"),
    (Key::End, "Ende", "key-end"),
    (Key::PageUp, "Bild auf", "key-page-up"),
    (Key::PageDown, "Bild ab", "key-page-down"),
    (Key::ArrowLeft, "Pfeil links", "key-arrow-left"),
    (Key::ArrowRight, "Pfeil rechts", "key-arrow-right"),
    (Key::ArrowUp, "Pfeil oben", "key-arrow-up"),
    (Key::ArrowDown, "Pfeil unten", "key-arrow-down"),
    (Key::Tab, "Tab", "key-tab"),
    (Key::Copy, "Kopieren", "key-copy"),
    (Key::Cut, "Ausschneiden", "key-cut"),
    (Key::Paste, "Einfügen", "key-paste"),
    (Key::Colon, "Doppelpunkt", "key-colon"),
    (Key::Comma, "Komma", "key-comma"),
    (Key::Minus, "Minus", "key-minus"),
    (Key::Period, "Punkt", "key-period"),
    (Key::Plus, "Plus", "key-plus"),
    (Key::Equals, "Gleich", "key-equals"),
    (Key::Semicolon, "Semikolon", "key-semicolon"),
    (Key::Backslash, "Backslash", "key-backslash"),
    (Key::Slash, "Schrägstrich", "key-slash"),
    (Key::Pipe, "Senkrechter Strich", "key-pipe"),
    (Key::Questionmark, "Fragezeichen", "key-questionmark"),
    (
        Key::Exclamationmark,
        "Ausrufezeichen",
        "key-exclamationmark",
    ),
    (Key::OpenBracket, "Eckige Klammer auf", "key-open-bracket"),
    (Key::CloseBracket, "Eckige Klammer zu", "key-close-bracket"),
    (
        Key::OpenCurlyBracket,
        "Geschweifte Klammer auf",
        "key-open-brace",
    ),
    (
        Key::CloseCurlyBracket,
        "Geschweifte Klammer zu",
        "key-close-brace",
    ),
    (Key::Backtick, "Gravis", "key-backtick"),
    (Key::Quote, "Apostroph", "key-quote"),
    (Key::BrowserBack, "Browser zurück", "key-browser-back"),
    (Key::IntlBackslash, "Taste <>", "key-intl-backslash"),
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

/// Name der Taste in der Sprache der Oberfläche.
pub(crate) fn key_name(key: Key) -> String {
    GERMAN_KEYS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map_or_else(|| key.name().to_string(), |(_, _, id)| planora_i18n::t(id))
}

/// Gespeicherter (deutscher) Name der Taste.
fn stored_key_name(key: Key) -> &'static str {
    GERMAN_KEYS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map_or_else(|| key.name(), |(_, n, _)| n)
}

fn key_from_name(name: &str) -> Option<Key> {
    GERMAN_KEYS
        .iter()
        .find(|(_, n, _)| *n == name)
        .map(|(k, _, _)| *k)
        .or_else(|| Key::from_name(name))
        .filter(|k| !is_modifier_key(*k))
}

/// Gespeicherter (deutscher) Name der Maustaste.
fn stored_button_name(button: MouseButton) -> &'static str {
    match button {
        MouseButton::Left => "Linke Taste ziehen",
        MouseButton::Middle => "Mittlere Taste ziehen",
        MouseButton::Right => "Rechte Taste ziehen",
    }
}

/// Die Namen der gehaltenen Umschalttasten: gespeichert (`stored`, deutsch) oder in der Sprache
/// der Oberfläche.
fn modifiers(ctrl: bool, shift: bool, alt: bool, stored: bool) -> Vec<String> {
    let names = if stored {
        [CTRL.to_string(), SHIFT.to_string(), ALT.to_string()]
    } else {
        [
            planora_i18n::tr!("key-ctrl"),
            planora_i18n::tr!("key-shift"),
            planora_i18n::tr!("key-alt"),
        ]
    };
    [ctrl, shift, alt]
        .into_iter()
        .zip(names)
        .filter_map(|(on, name)| on.then_some(name))
        .collect()
}

fn chord(c: &KeyChord, stored: bool) -> String {
    let mut parts = modifiers(c.ctrl, c.shift, c.alt, stored);
    parts.push(if stored {
        stored_key_name(c.key).to_string()
    } else {
        key_name(c.key)
    });
    parts.join("+")
}

fn gesture(g: &Gesture, stored: bool) -> String {
    let (mut parts, what) = match *g {
        Gesture::Drag {
            button,
            ctrl,
            shift,
            alt,
        } => (
            modifiers(ctrl, shift, alt, stored),
            if stored {
                stored_button_name(button).to_string()
            } else {
                match button {
                    MouseButton::Left => planora_i18n::tr!("mouse-drag-left"),
                    MouseButton::Middle => planora_i18n::tr!("mouse-drag-middle"),
                    MouseButton::Right => planora_i18n::tr!("mouse-drag-right"),
                }
            },
        ),
        Gesture::Wheel { ctrl, shift, alt } => (
            modifiers(ctrl, shift, alt, stored),
            if stored {
                WHEEL.to_string()
            } else {
                planora_i18n::tr!("key-wheel")
            },
        ),
    };
    parts.push(what);
    parts.join(" + ")
}

/// „Strg+Umschalt+E“, „Entf“, „Leertaste“, „E“ (englisch „Ctrl+Shift+E“, „Delete“, „Space“).
pub fn chord_text(c: &KeyChord) -> String {
    chord(c, false)
}

/// „Mittlere Taste ziehen“, „Umschalt + Mittlere Taste ziehen“, „Mausrad“.
pub fn gesture_text(g: &Gesture) -> String {
    gesture(g, false)
}

/// Die Belegung in der Sprache der Oberfläche (Menü, Einstellungen, Übersicht).
pub fn binding_text(b: &Binding) -> String {
    match b {
        Binding::Key(c) => chord_text(c),
        Binding::Mouse(g) => gesture_text(g),
    }
}

/// Die Belegung für die Konfiguration: immer die deutschen Namen, unabhängig von der Sprache.
pub fn stored_text(b: &Binding) -> String {
    match b {
        Binding::Key(c) => chord(c, true),
        Binding::Mouse(g) => gesture(g, true),
    }
}

/// Umkehrung von [`stored_text`]: Umschalttasten (jede höchstens einmal) und zuletzt die
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
        .find(|b| stored_button_name(*b) == *last);
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
