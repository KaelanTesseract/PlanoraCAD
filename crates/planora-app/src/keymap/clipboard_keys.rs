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

/// Die gedrückte Taste `key` mit Strg.
fn ctrl_press(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::COMMAND,
    }
}

/// Macht aus `Copy`, `Cut` und `Paste` gedrückte Tasten Strg+C, Strg+X, Strg+V (siehe das Modul).
/// `typing`: Ein Textfeld hat die Tastatur; dann bleibt alles, wie es ist (der Riegel wird nur
/// zurückgesetzt, denn ein Loslassen im Feld sieht dieser Weg nicht).
pub fn clipboard_events_as_keys(events: &mut Vec<Event>, typing: bool, latch: &mut PasteLatch) {
    if typing {
        latch.paste_seen = false;
        return;
    }
    let mut out = Vec::with_capacity(events.len());
    for e in events.drain(..) {
        match e {
            Event::Copy => out.push(ctrl_press(Key::C)),
            Event::Cut => out.push(ctrl_press(Key::X)),
            Event::Paste(_) => {
                latch.paste_seen = true;
                out.push(ctrl_press(Key::V));
            }
            Event::Key {
                key: Key::V,
                pressed: false,
                modifiers,
                ..
            } => {
                // Jedes Loslassen von V beendet das Einfügen; mit Strg und ohne `Paste` war die
                // Systemzwischenablage leer: Das Loslassen ist der Druck.
                if has_ctrl(&modifiers) && !latch.paste_seen {
                    out.push(ctrl_press(Key::V));
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

/// Wie [`clipboard_events_as_keys`] für die Ereignisse `events` des Fensters `viewport`; der
/// Riegel liegt je Fenster im Kontext (temporär).
pub fn convert_for(
    ctx: &egui::Context,
    viewport: egui::ViewportId,
    events: &mut Vec<Event>,
    typing: bool,
) {
    let id = latch_id(viewport);
    let mut latch = ctx
        .data(|d| d.get_temp::<PasteLatch>(id))
        .unwrap_or_default();
    clipboard_events_as_keys(events, typing, &mut latch);
    ctx.data_mut(|d| d.insert_temp(id, latch));
}

/// Wie [`convert_for`] für die Eingabe des Fensters, in dem gerade gezeichnet wird.
pub fn convert_input(ctx: &egui::Context, typing: bool) {
    let viewport = ctx.viewport_id();
    let id = latch_id(viewport);
    let mut latch = ctx
        .data(|d| d.get_temp::<PasteLatch>(id))
        .unwrap_or_default();
    ctx.input_mut(|i| clipboard_events_as_keys(&mut i.events, typing, &mut latch));
    ctx.data_mut(|d| d.insert_temp(id, latch));
}
