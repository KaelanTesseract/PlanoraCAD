//! Verweise auf Parameter im Inhalt eines Skizzentextes (Skizzen 11b, Spec 4): `{Name}` wird
//! durch den Wert des Parameters ersetzt. Der Text hält den Rohinhalt; der ersetzte steht nur zur
//! Laufzeit in [`TextData::shown`](planora_sketch::text::TextData::shown), und die Kurven
//! entstehen daraus. Ersetzt und neu gesetzt wird in [`Document::reshape_texts`]
//! (`crate::model::Document`), weil nur die App eine Schriftquelle hat; die Neuberechnung prüft
//! die Verweise und warnt bei unbekannten Namen.
//!
//! **Annahmen:** Der Wert steht ohne Einheitenzeichen da (der Nutzer tippt „{Breite} mm“); `{`
//! lässt sich nicht maskieren, ein `{…}` ohne gültigen Namen bleibt einfach stehen.

use crate::expr::{ParamUnit, check_name, same_name};
use crate::params::ParamScope;
use planora_sketch::model::Sketch;
use planora_sketch::text::TextId;

/// Ein Verweis in `content`: Byte-Bereich samt Klammern und der Name.
struct Ref<'a> {
    start: usize,
    end: usize,
    name: &'a str,
}

/// Alle Verweise `{Name}` in `content` in Reihenfolge. Ohne Leerzeichen in der Klammer, mit einem
/// Namen, den [`check_name`] annimmt; `{}` oder `{1a}` sind keiner. Bei `{{Name}}` zählt die
/// innere Klammer.
fn refs(content: &str) -> Vec<Ref<'_>> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = content[from..].find('{') {
        let open = from + rel;
        let after = open + 1;
        let Some(close_rel) = content[after..].find(['{', '}']) else {
            break;
        };
        let close = after + close_rel;
        if content[close..].starts_with('}') {
            let name = &content[after..close];
            if check_name(name).is_ok() {
                out.push(Ref {
                    start: open,
                    end: close + 1,
                    name,
                });
            }
            from = close + 1;
        } else {
            from = close;
        }
    }
    out
}

/// Enthält `content` einen Verweis `{Name}` mit einem Namen, den [`check_name`] annimmt? Ohne
/// Leerzeichen in der Klammer; `{}` oder `{1a}` sind kein Verweis.
pub fn has_param_ref(content: &str) -> bool {
    !refs(content).is_empty()
}

/// Die Namen aller Verweise `{Name}` in `content`, in der Reihenfolge des Auftretens (doppelte
/// bleiben).
pub fn param_refs(content: &str) -> Vec<String> {
    refs(content).into_iter().map(|r| r.name.into()).collect()
}

/// Der Wert eines Parameters, wie er im Text steht: Länge in der Anzeigeeinheit mit deren
/// Nachkommastellen (wie im Parameterfeld), Winkel (Grad) und Zahl mit zwei; ohne
/// Einheitenzeichen.
fn value_text(v: f64, unit: ParamUnit) -> String {
    match unit {
        ParamUnit::Length => planora_i18n::fmt_len_num(v),
        ParamUnit::Angle | ParamUnit::Number => planora_i18n::fmt_num(v, 2),
    }
}

/// Ersetzt jeden bekannten `{Name}` (Groß/Klein egal) durch den Wert aus `scope`. Unbekanntes
/// bleibt wörtlich stehen und kommt, wie geschrieben und in Reihenfolge, in die zweite Liste.
pub fn substitute(content: &str, scope: &ParamScope) -> (String, Vec<String>) {
    let mut out = String::with_capacity(content.len());
    let mut unknown = Vec::new();
    let mut last = 0;
    for r in refs(content) {
        out.push_str(&content[last..r.start]);
        match scope.lookup(r.name) {
            Some((v, unit)) => out.push_str(&value_text(v, unit)),
            None => {
                out.push_str(&content[r.start..r.end]);
                unknown.push(r.name.to_string());
            }
        }
        last = r.end;
    }
    out.push_str(&content[last..]);
    (out, unknown)
}

/// Schreibt jeden Verweis auf `old` (ganzer Name, Groß/Klein wie [`same_name`]) auf `new` um; der
/// übrige Text bleibt zeichengleich.
pub fn rename_refs(content: &str, old: &str, new: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut last = 0;
    for r in refs(content) {
        if same_name(r.name, old) {
            out.push_str(&content[last..r.start]);
            out.push('{');
            out.push_str(new);
            out.push('}');
            last = r.end;
        }
    }
    out.push_str(&content[last..]);
    out
}

/// Setzt den gezeigten Inhalt jedes Textes in `sk`: `Some(ersetzt)` bei Verweisen, sonst `None`.
/// `true`, wenn sich einer geändert hat. Die Kurven setzt danach [`Sketch::reshape_texts`] neu.
pub fn apply_shown(sk: &mut Sketch, scope: &ParamScope) -> bool {
    let wanted: Vec<(TextId, Option<String>)> = sk
        .texts()
        .map(|(t, d)| {
            let shown = has_param_ref(&d.content).then(|| substitute(&d.content, scope).0);
            (t, shown)
        })
        .collect();
    let mut changed = false;
    for (t, shown) in wanted {
        changed |= sk.set_text_shown(t, shown);
    }
    changed
}

/// Nennt ein Text der Skizze den Parameter `name` in `{…}`?
pub(crate) fn sketch_mentions(sk: &Sketch, name: &str) -> bool {
    sk.texts()
        .any(|(_, d)| refs(&d.content).iter().any(|r| same_name(r.name, name)))
}

/// Schreibt in allen Texten der Skizze `{old}` auf `{new}` um ([`rename_refs`]).
pub(crate) fn rename_in_sketch(sk: &mut Sketch, old: &str, new: &str) {
    let renamed: Vec<(TextId, String)> = sk
        .texts()
        .map(|(t, d)| (t, rename_refs(&d.content, old, new)))
        .collect();
    for (t, content) in renamed {
        sk.set_text_content(t, content);
    }
}

/// Die unbekannten Namen in den Texten der Skizze, je Name (Groß/Klein egal) einmal, in der
/// Reihenfolge der Texte; leer ohne.
pub(crate) fn unknown_names(sk: &Sketch, scope: &ParamScope) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (_, d) in sk.texts() {
        for r in refs(&d.content) {
            if scope.lookup(r.name).is_none() && !out.iter().any(|n| same_name(n, r.name)) {
                out.push(r.name.to_string());
            }
        }
    }
    out
}
