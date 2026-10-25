//! Verteiler: welche Aktionen die Tastenereignisse eines Bildes auslösen. Liest die Ereignisse,
//! verbraucht die passenden und meldet die Aktionen; ausführen und sperren (Tippen, Rückfrage,
//! offene Fenster, zweiter Durchgang) bleibt Sache des Aufrufers (`PlanoraApp::shortcuts`).

use super::{Action, Area, Binding, KeyChord, Keymap};
use crate::drawing_state::WorkArea;
use crate::state::AppState;

/// Bereich der Belegungen für den Tab: Skizze offen → Skizze, Zeichnung offen → Zeichnung,
/// sonst Modell.
pub fn current_area(state: &AppState) -> Area {
    if state.sketch_session.is_some() {
        Area::Sketch
    } else if matches!(state.drawing_ui.area, WorkArea::Drawing(_)) {
        Area::Drawing
    } else {
        Area::Model
    }
}

/// Die Aktionen, deren Taste in diesem Bild gedrückt wurde (Bereich `area` und „Überall“), siehe
/// [`pressed_if`].
pub fn pressed(ctx: &egui::Context, km: &Keymap, area: Area) -> Vec<Action> {
    pressed_if(ctx, km, area, |_| true)
}

/// Wie [`pressed`], nur für die Aktionen, für die `want` gilt; die Tasten der übrigen bleiben
/// unverbraucht. Umschalttasten zählen genau (Strg+Z passt nicht auf Strg+Umschalt+Z oder
/// Strg+Alt+Z, anders als bei `consume_shortcut`). Alle passenden Ereignisse werden verbraucht
/// (gehaltene Taste: Wiederholungen eines Bildes zusammen), jede Aktion kommt höchstens einmal.
/// Reihenfolge: mehr Umschalttasten zuerst, sonst die Tabelle. Hat ein Bedienelement den
/// Tastaturfokus, das Tasten liest (Knopf über Tab, Textfeld), bleibt die Leertaste ihm (sie
/// drückt den Knopf), die übrigen Kürzel wirken. Elemente, die nur Pfeile lesen und mit
/// [`crate::viewport::focus_is_not_typing`] gemeldet sind (Marker der Verlaufsleiste, Ansicht,
/// Statuszeile), halten die Leertaste nicht zurück. Verbraucht wird sie vor dem Aufbau der
/// Bedienelemente, egui macht aus ihr dann keinen Ersatzklick auf das fokussierte Element.
pub fn pressed_if(
    ctx: &egui::Context,
    km: &Keymap,
    area: Area,
    want: impl Fn(Action) -> bool,
) -> Vec<Action> {
    let focus = crate::viewport::keyboard_taken(ctx);
    let mut chords: Vec<(Action, KeyChord)> = Action::ALL
        .iter()
        .copied()
        .filter(|&a| want(a) && Keymap::areas_overlap(area, a.area()))
        .flat_map(|a| {
            km.bindings(a).iter().filter_map(move |b| match b {
                Binding::Key(c) => Some((a, *c)),
                Binding::Mouse(_) => None,
            })
        })
        .filter(|(_, c)| !(focus && c.key == egui::Key::Space))
        .collect();
    chords.sort_by_key(|(_, c)| std::cmp::Reverse(modifier_count(c)));
    let mut out = Vec::new();
    ctx.input_mut(|i| {
        for (a, c) in chords {
            let before = i.events.len();
            i.events.retain(|e| !is_press_of(e, &c));
            if i.events.len() != before && !out.contains(&a) {
                out.push(a);
            }
        }
    });
    out
}

fn modifier_count(c: &KeyChord) -> usize {
    usize::from(c.ctrl) + usize::from(c.shift) + usize::from(c.alt)
}

/// Gedrückt (auch Wiederholung) mit genau den Umschalttasten von `c`; Strg kommt unter Windows
/// als `ctrl` und `command`, in Tests oft nur als `command`.
fn is_press_of(e: &egui::Event, c: &KeyChord) -> bool {
    matches!(
        e,
        egui::Event::Key { key, pressed: true, modifiers, .. }
            if *key == c.key
                && (modifiers.ctrl || modifiers.command) == c.ctrl
                && modifiers.shift == c.shift
                && modifiers.alt == c.alt
    )
}
