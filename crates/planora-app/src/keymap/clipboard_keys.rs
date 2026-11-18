//! Strg+C, Strg+X und Strg+V als Tasten (Skizzen 11b, Task 3).
//!
//! egui-winit macht aus diesen drei Kürzeln keine Tasten, sondern `Event::Copy`, `Event::Cut` und
//! `Event::Paste(text)`; `Paste` kommt nur, wenn die Systemzwischenablage Text hat. Das Loslassen
//! von V kommt als `Event::Key` trotzdem. Damit die Belegung der Tastenkürzel (und ihre
//! Umbelegung in den Einstellungen) für die Zwischenablage der Skizze gilt, werden die drei
//! Ereignisse hier vor dem Verteiler zu gedrückten Tasten Strg+C, Strg+X und Strg+V.
//!
//! - Hat ein Textfeld die Tastatur (`typing`), bleibt alles, wie es ist: Das Feld liest `Copy`,
//!   `Cut` und `Paste` selbst.
//! - Strg+V mit leerer Systemzwischenablage: Es kommt kein `Paste`, nur das Loslassen von V mit
//!   Strg. Ein solches Loslassen ohne `Paste` seit dem letzten Loslassen wird zusätzlich ein
//!   Druck. Der Riegel [`PasteLatch`] merkt, ob zum Loslassen ein `Paste` kam (dann gibt es den
//!   Druck schon), damit Strg+V genau einmal wirkt.
//! - Die Umwandlung ist idempotent: Das Loslassen von V mit Strg wird dabei entfernt (der
//!   Zustand der Tasten steht schon in der Eingabe), ein zweiter Aufruf im selben Bild findet
//!   nichts mehr.

use egui::{Event, Key, Modifiers};

/// Merkt je Fenster, ob seit dem letzten Loslassen von V ein `Event::Paste` kam.
#[derive(Clone, Copy, Debug, Default)]
pub struct PasteLatch {
    pub paste_seen: bool,
}

/// Strg gehalten (unter Windows und Linux `ctrl` und `command`, in Tests oft nur `command`)?
fn has_ctrl(m: &Modifiers) -> bool {
    m.ctrl || m.command
}

/// Die gedrückte Taste `key` mit den Umschalttasten `modifiers`.
fn press(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Wie [`clipboard_events_as_keys_with`] mit gehaltenem Strg allein.
pub fn clipboard_events_as_keys(events: &mut Vec<Event>, typing: bool, latch: &mut PasteLatch) {
    clipboard_events_as_keys_with(events, typing, latch, Modifiers::COMMAND);
}

/// Macht aus `Copy`, `Cut` und `Paste` gedrückte Tasten C, X, V mit den gehaltenen Umschalttasten
/// `held` (siehe das Modul). Nur mit Strg: Umschalt+Entf, Umschalt+Einfg und Ähnliches bleiben,
/// was sie sind; Strg+Umschalt+C wird die Taste Strg+Umschalt+C (nicht Strg+C).
/// `typing`: Ein Textfeld hat die Tastatur; dann bleibt alles, wie es ist. Nur der Riegel läuft
/// mit: Ein `Paste` im Feld gehört zum Loslassen von V, das nach einem Fokuswechsel kommen kann.
pub fn clipboard_events_as_keys_with(
    events: &mut Vec<Event>,
    typing: bool,
    latch: &mut PasteLatch,
    held: Modifiers,
) {
    if typing {
        for e in events.iter() {
            match e {
                Event::Paste(_) => latch.paste_seen = true,
                Event::Key {
                    key: Key::V,
                    pressed: false,
                    ..
                } => latch.paste_seen = false,
                _ => {}
            }
        }
        return;
    }
    let ctrl = has_ctrl(&held);
    let mut out = Vec::with_capacity(events.len());
    for e in events.drain(..) {
        match e {
            Event::Copy if ctrl => out.push(press(Key::C, held)),
            Event::Cut if ctrl => out.push(press(Key::X, held)),
            Event::Paste(_) if ctrl => {
                latch.paste_seen = true;
                out.push(press(Key::V, held));
            }
            Event::Key {
                key: Key::V,
                pressed: false,
                modifiers,
                ..
            } => {
                // Jedes Loslassen von V beendet das Einfügen; mit Strg und ohne `Paste` war die
                // Systemzwischenablage leer: Das Loslassen ist der Druck (mit denselben
                // Umschalttasten, Strg+Umschalt+V bleibt Strg+Umschalt+V).
                if has_ctrl(&modifiers) && !latch.paste_seen {
                    out.push(press(Key::V, modifiers));
                }
                latch.paste_seen = false;
                if !has_ctrl(&modifiers) {
                    out.push(e);
                }
            }
            other => out.push(other),
        }
    }
    *events = out;
}

fn latch_id(viewport: egui::ViewportId) -> egui::Id {
    egui::Id::new(("planora-paste-latch", viewport))
}

fn sketch_open_id() -> egui::Id {
    egui::Id::new("planora-clip-sketch-open")
}

/// Merkt für die abgedockten Fenster, ob im aktiven Tab eine Skizze offen ist (die App setzt es
/// je Bild). Nur dann werden `Copy`/`Cut`/`Paste` dort zu Tasten.
pub fn set_sketch_open(ctx: &egui::Context, open: bool) {
    ctx.data_mut(|d| d.insert_temp(sketch_open_id(), open));
}

/// Ist im aktiven Tab eine Skizze offen (siehe [`set_sketch_open`])?
pub fn sketch_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(sketch_open_id()))
        .unwrap_or(false)
}

/// Wie [`clipboard_events_as_keys_with`] für die Eingabe des Fensters, in dem gerade gezeichnet
/// wird; der Riegel liegt je Fenster im Kontext (temporär). Ohne offene Skizze (`sketch_open`)
/// bleiben alle Ereignisse unverändert (Strg+C in markiertem Text einer Beschriftung gehört egui).
pub fn convert_input(ctx: &egui::Context, typing: bool, sketch_open: bool) {
    let id = latch_id(ctx.viewport_id());
    let mut latch = ctx
        .data(|d| d.get_temp::<PasteLatch>(id))
        .unwrap_or_default();
    if sketch_open {
        ctx.input_mut(|i| {
            let held = i.modifiers;
            clipboard_events_as_keys_with(&mut i.events, typing, &mut latch, held);
        });
    } else {
        latch.paste_seen = false;
    }
    ctx.data_mut(|d| d.insert_temp(id, latch));
}
