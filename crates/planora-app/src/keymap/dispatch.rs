//! Verteiler: welche Aktionen die Tastenereignisse eines Bildes auslösen. Liest die Ereignisse,
//! verbraucht die passenden und meldet die Aktionen; ausführen und sperren (Tippen, Rückfrage,
//! offene Fenster, zweiter Durchgang) bleibt Sache des Aufrufers (`PlanoraApp::shortcuts`).

use super::{Action, Area, Binding, KeyChord, Keymap, ModKey};
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
    ctx.input_mut(|i| pressed_in(&mut i.events, focus, km, area, want))
}

/// Wie [`pressed_if`] über eine Liste von Ereignissen (etwa die eines abgedockten Fensters,
/// [`crate::detach::ChildKeys`]); `focus`: ein Bedienelement, das Tasten liest, hat dort den
/// Tastaturfokus (die Leertaste bleibt ihm).
pub fn pressed_in(
    events: &mut Vec<egui::Event>,
    focus: bool,
    km: &Keymap,
    area: Area,
    want: impl Fn(Action) -> bool,
) -> Vec<Action> {
    let mut chords: Vec<(Action, KeyChord)> = Action::ALL
        .iter()
        .copied()
        .filter(|&a| want(a) && Keymap::areas_overlap(area, a.area()))
        .flat_map(|a| {
            km.bindings(a).iter().filter_map(move |b| match b {
                Binding::Key(c) => Some((a, *c)),
                Binding::Mouse(_) | Binding::Modifier(_) => None,
            })
        })
        .filter(|(_, c)| !(focus && c.key == egui::Key::Space))
        .collect();
    chords.sort_by_key(|(_, c)| std::cmp::Reverse(modifier_count(c)));
    let mut out = Vec::new();
    for (a, c) in chords {
        let before = events.len();
        events.retain(|e| !is_press_of(e, &c));
        if events.len() != before && !out.contains(&a) {
            out.push(a);
        }
    }
    out
}

/// Erkennt „Umschalttaste allein gedrückt und losgelassen“ über die Bilder. Ein Tipp gilt nur,
/// wenn genau eine Umschalttaste gedrückt wurde, während kein anderes Ereignis kam: keine Taste
/// (außer Umschalttasten selbst), keine Maustaste (auch nicht schon vorher gedrückt: Alt plus
/// Ziehen dreht die Kamera) und kein Rad, und keine zweite Umschalttaste dazukam. Ein Wechsel des
/// Fensterfokus (Alt+Tab) bricht ihn ab und sperrt, bis alle Umschalttasten los sind: Beim
/// Zurückkommen mit noch gehaltenem Alt gilt das Loslassen nicht als Tipp. Gemeldet wird er beim
/// Loslassen. Je Bild genau einmal [`ModTap::step`] aufrufen.
#[derive(Clone, Copy, Debug, Default)]
pub struct ModTap {
    prev: [bool; 3],
    armed: Option<ModKey>,
    /// Fokus gewechselt: bis zum Loslassen aller Umschalttasten kein neuer Tipp.
    blocked: bool,
}

impl ModTap {
    /// Wertet ein Bild aus; `Some`, wenn in diesem Bild eine Umschalttaste nach einem reinen Tipp
    /// losgelassen wurde.
    pub fn step(&mut self, i: &egui::InputState) -> Option<ModKey> {
        let m = i.modifiers;
        let now = [m.ctrl || m.command, m.shift, m.alt];
        let focus_changed = !i.focused
            || i.events
                .iter()
                .any(|e| matches!(e, egui::Event::WindowFocused(_)));
        let other = focus_changed
            || i.pointer.any_down()
            || i.events.iter().any(|e| match e {
                egui::Event::Key { key, pressed, .. } => {
                    *pressed && !super::text::is_modifier_key(*key)
                }
                egui::Event::PointerButton { pressed, .. } => *pressed,
                egui::Event::MouseWheel { .. } | egui::Event::Text(_) => true,
                _ => false,
            });
        if other {
            self.armed = None;
        }
        if focus_changed {
            self.blocked = true;
        }
        let count = now.iter().filter(|b| **b).count();
        let mut fired = None;
        if count == 0 {
            fired = self.armed.take();
            self.blocked = focus_changed;
        } else if count == 1 {
            let held = if now[0] {
                ModKey::Ctrl
            } else if now[1] {
                ModKey::Shift
            } else {
                ModKey::Alt
            };
            if self.prev == [false; 3] && !other && !self.blocked {
                self.armed = Some(held);
            } else if self.armed != Some(held) {
                self.armed = None;
            }
        } else {
            self.armed = None;
        }
        self.prev = now;
        fired
    }
}

fn mod_tap_id() -> egui::Id {
    egui::Id::new("planora-keymap-mod-tap")
}

/// Der Tipp einer Umschalttaste in diesem Bild (siehe [`ModTap`]); der Verfolger liegt im
/// Kontext. Genau einmal je Bild aufrufen, auch wenn die Tasten gesperrt sind (der Verfolger muss
/// jedes Bild sehen).
pub fn tapped(ctx: &egui::Context) -> Option<ModKey> {
    let mut tap = ctx
        .data(|d| d.get_temp::<ModTap>(mod_tap_id()))
        .unwrap_or_default();
    let out = ctx.input(|i| tap.step(i));
    ctx.data_mut(|d| d.insert_temp(mod_tap_id(), tap));
    out
}

/// Die Aktionen im Bereich `area` (und „Überall“), die den Tipp `m` belegt haben, in der
/// Reihenfolge der Tabelle.
pub fn tap_actions(km: &Keymap, area: Area, m: ModKey) -> Vec<Action> {
    Action::ALL
        .iter()
        .copied()
        .filter(|&a| Keymap::areas_overlap(area, a.area()))
        .filter(|&a| km.bindings(a).contains(&Binding::Modifier(m)))
        .collect()
}

/// Wird eine Taste von `a` gerade gehalten (z. B. Pfeil hoch für „Z sperren“)? Die Umschalttasten
/// müssen genau stimmen, wie bei [`pressed`]; eine Belegung „Umschalttaste allein“ gilt, solange
/// genau diese gehalten wird. Verbraucht nichts. Wirkt für Tasten (nicht für Mausgesten) von
/// Aktionen im Bereich `area` oder „Überall“.
pub fn held(ctx: &egui::Context, km: &Keymap, area: Area, a: Action) -> bool {
    if !Keymap::areas_overlap(area, a.area()) {
        return false;
    }
    ctx.input(|i| {
        let m = i.modifiers;
        km.bindings(a).iter().any(|b| match b {
            Binding::Key(c) => {
                i.key_down(c.key)
                    && (m.ctrl || m.command) == c.ctrl
                    && m.shift == c.shift
                    && m.alt == c.alt
            }
            // Eine Umschalttaste allein gehalten: genau sie und keine weitere.
            Binding::Modifier(k) => {
                let now = [m.ctrl || m.command, m.shift, m.alt];
                let want = match k {
                    ModKey::Ctrl => [true, false, false],
                    ModKey::Shift => [false, true, false],
                    ModKey::Alt => [false, false, true],
                };
                now == want && i.focused
            }
            Binding::Mouse(_) => false,
        })
    })
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
