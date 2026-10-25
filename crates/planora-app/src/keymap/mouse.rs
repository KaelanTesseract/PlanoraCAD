//! Mausgesten der Ansichten über die Tastenbelegung: welche Aktion (Schwenken, Drehen) zu einer
//! laufenden Ziehgeste gehört und wie weit das Mausrad zoomt. Die Ansichten rechnen die Bewegung
//! selbst (Kamera, Blatt); hier wird nur entschieden, ob und welche Geste gilt.
//!
//! Die Belegung kommt mit [`set_current`] einmal je Bild in den egui-Kontext (die App legt dort
//! [`crate::workspace::Workspace::keymap`] ab); die Ansichten lesen sie mit [`current`] und
//! brauchen so keinen eigenen Parameter. Ohne Eintrag gilt die Voreinstellung.
//!
//! Solange die Einstellungen eine neue Belegung erfassen ([`super::settings_ui::is_capturing`]),
//! gilt keine Geste: Ziehen und Rad sollen dann nur erfasst werden, nicht die Ansicht bewegen.

use std::sync::Arc;

use egui::PointerButton;

use super::{Action, Binding, Gesture, Keymap, MouseButton};

fn current_id() -> egui::Id {
    egui::Id::new("planora-keymap-current")
}

fn wheel_modifiers_id() -> egui::Id {
    egui::Id::new("planora-keymap-wheel-modifiers")
}

/// Legt die Belegung für die Ansichten dieses (und der folgenden) Bilder ab.
pub fn set_current(ctx: &egui::Context, km: &Keymap) {
    let same = ctx.data(|d| {
        d.get_temp::<Arc<Keymap>>(current_id())
            .is_some_and(|k| *k == *km)
    });
    if !same {
        ctx.data_mut(|d| d.insert_temp(current_id(), Arc::new(km.clone())));
    }
}

/// Die mit [`set_current`] abgelegte Belegung; ohne Eintrag die Voreinstellung.
pub fn current(ctx: &egui::Context) -> Arc<Keymap> {
    ctx.data(|d| d.get_temp::<Arc<Keymap>>(current_id()))
        .unwrap_or_else(|| Arc::new(Keymap::defaults()))
}

fn capturing(ui: &egui::Ui) -> bool {
    super::settings_ui::is_capturing(ui.ctx())
}

fn pointer_button(b: MouseButton) -> PointerButton {
    match b {
        MouseButton::Left => PointerButton::Primary,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Right => PointerButton::Secondary,
    }
}

/// Gerade gehaltene Umschalttasten als (Strg, Umschalt, Alt); Strg kommt unter Windows als
/// `ctrl` und `command`, in Tests oft nur als eins von beiden.
fn held_modifiers(ui: &egui::Ui) -> (bool, bool, bool) {
    ui.input(|i| {
        let m = i.modifiers;
        (m.ctrl || m.command, m.shift, m.alt)
    })
}

/// Die Aktion aus `allowed`, deren Ziehgeste genau die Umschalttasten `mods` hat und deren Taste
/// `active` meldet. Weil die Umschalttasten genau passen müssen, unterscheiden sich Treffer nur in
/// der Taste (mehrere Tasten gehalten); dann gewinnt der erste in `allowed` und in der Belegung.
/// `skip` blendet einzelne Gesten aus.
fn pick(
    km: &Keymap,
    allowed: &[Action],
    mods: (bool, bool, bool),
    active: impl Fn(MouseButton) -> bool,
    skip: impl Fn(&Gesture) -> bool,
) -> Option<Action> {
    allowed.iter().copied().find(|&a| {
        km.bindings(a).iter().any(|b| match b {
            Binding::Mouse(
                g @ Gesture::Drag {
                    button,
                    ctrl,
                    shift,
                    alt,
                },
            ) => (*ctrl, *shift, *alt) == mods && !skip(g) && active(*button),
            _ => false,
        })
    })
}

/// Welche Aktion aus `allowed` gehört zu der gerade laufenden Ziehgeste auf `response`? Taste
/// und Umschalttasten (Strg, Umschalt, Alt) müssen genau passen; ein bloßer Klick ist keine
/// Ziehgeste (egui meldet ein Ziehen erst nach einigen Pixeln). Passen mehrere (mehrere Tasten
/// gehalten), gewinnt die erste in `allowed`.
pub fn drag_action(
    km: &Keymap,
    ui: &egui::Ui,
    response: &egui::Response,
    allowed: &[Action],
) -> Option<Action> {
    if !response.dragged() || capturing(ui) {
        return None;
    }
    pick(
        km,
        allowed,
        held_modifiers(ui),
        |b| response.dragged_by(pointer_button(b)),
        |_| false,
    )
}

/// Wie [`drag_action`] mit nur [`Action::ViewPan`], für Skizze und Zeichnung: dort zeichnet und
/// wählt die linke Taste ohne Umschalttasten, eine solche Geste schwenkt also nie.
pub fn pan_2d(km: &Keymap, ui: &egui::Ui, response: &egui::Response) -> bool {
    if !response.dragged() || capturing(ui) {
        return false;
    }
    let plain_left = |g: &Gesture| {
        *g == Gesture::Drag {
            button: MouseButton::Left,
            ctrl: false,
            shift: false,
            alt: false,
        }
    };
    pick(
        km,
        &[Action::ViewPan],
        held_modifiers(ui),
        |b| response.dragged_by(pointer_button(b)),
        plain_left,
    ) == Some(Action::ViewPan)
}

/// Wie [`drag_action`], aber nach den gerade gehaltenen Tasten statt nach egui's Ziehen; für
/// Flächen mit eigener Ziehschwelle (Navigationswürfel), die selbst entscheiden, dass gezogen
/// wird.
pub fn held_action(km: &Keymap, ui: &egui::Ui, allowed: &[Action]) -> Option<Action> {
    if capturing(ui) {
        return None;
    }
    let down = ui.input(|i| {
        [MouseButton::Left, MouseButton::Middle, MouseButton::Right]
            .map(|b| (b, i.pointer.button_down(pointer_button(b))))
    });
    pick(
        km,
        allowed,
        held_modifiers(ui),
        |b| down.iter().any(|&(x, d)| x == b && d),
        |_| false,
    )
}

/// Das Mausrad dieses Bilds, wenn eine Radgeste von [`Action::ViewZoom`] genau die
/// Umschalttasten des Drehens hat: `scroll` in Punkten wie `smooth_scroll_delta().y` (positiv =
/// Rad nach vorn) und `factor`, der Zoomfaktor, in den egui Strg+Rad umrechnet (1 ohne Strg).
/// egui glättet das Rad über mehrere Bilder und leitet es mit Umschalt in die Waagrechte, mit
/// Strg in `zoom_delta` um. Es zählen die Umschalttasten beim letzten Radereignis (wie bei egui),
/// damit das Nachlaufen nicht kippt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WheelInput {
    pub scroll: f32,
    pub factor: f32,
}

impl WheelInput {
    const NONE: WheelInput = WheelInput {
        scroll: 0.0,
        factor: 1.0,
    };
}

/// Rad dieses Bilds nach der Belegung, siehe [`WheelInput`].
pub fn wheel_input(km: &Keymap, ui: &egui::Ui) -> WheelInput {
    if capturing(ui) {
        return WheelInput::NONE;
    }
    let (event_mods, smooth, zoom, touch, event_zoom, held) = ui.input(|i| {
        let event_mods = i.events.iter().rev().find_map(|e| match e {
            egui::Event::MouseWheel { modifiers, .. } => Some(*modifiers),
            _ => None,
        });
        (
            event_mods,
            i.smooth_scroll_delta(),
            i.zoom_delta(),
            i.multi_touch().is_some(),
            event_zoom(i),
            i.modifiers,
        )
    });
    let ctx = ui.ctx();
    let m = match event_mods {
        Some(m) => {
            ctx.data_mut(|d| d.insert_temp(wheel_modifiers_id(), m));
            m
        }
        None => ctx
            .data(|d| d.get_temp::<egui::Modifiers>(wheel_modifiers_id()))
            .unwrap_or(held),
    };
    let gesture = Gesture::Wheel {
        ctrl: m.ctrl || m.command,
        shift: m.shift,
        alt: m.alt,
    };
    if !km
        .bindings(Action::ViewZoom)
        .contains(&Binding::Mouse(gesture))
    {
        return WheelInput::NONE;
    }
    let scroll = if m.shift {
        smooth.x + smooth.y
    } else {
        smooth.y
    };
    // Strg+Rad hat egui in einen Zoomfaktor umgerechnet (ohne Zwei-Finger-Geste).
    let wheel_zoom = zoom / event_zoom;
    let factor = if !touch && wheel_zoom.is_finite() && wheel_zoom > 0.0 {
        wheel_zoom
    } else {
        1.0
    };
    WheelInput { scroll, factor }
}

/// Zoomwert des Mausrads in Punkten (siehe [`WheelInput`]); der Strg+Rad-Faktor ist über egui's
/// `scroll_zoom_speed` in Punkte zurückgerechnet. 0, wenn keine Radgeste passt. Für die
/// 3D-Ansicht, die in Punkten rechnet; die Zeichnung nimmt [`wheel_factor`].
pub(crate) fn wheel(km: &Keymap, ui: &egui::Ui) -> f32 {
    let w = wheel_input(km, ui);
    let speed = ui.ctx().options(|o| o.input_options.scroll_zoom_speed);
    if speed > 0.0 && w.factor != 1.0 {
        w.scroll + w.factor.ln() / speed
    } else {
        w.scroll
    }
}

/// Zoomfaktor des Mausrads für Flächen mit eigener Radrate `rate` je Punkt: das Rad ohne Strg mit
/// `rate`, Strg+Rad mit egui's eigenem Faktor (wie `zoom_delta`). 1, wenn keine Radgeste passt.
pub fn wheel_factor(km: &Keymap, ui: &egui::Ui, rate: f32) -> f32 {
    let w = wheel_input(km, ui);
    w.factor * (w.scroll * rate).exp()
}

/// Zoomfaktor der Zwei-Finger-Geste in diesem Bild (Berührung oder Zoomereignis des Touchpads),
/// ohne das Mausrad; 1 ohne Geste.
pub fn pinch(ui: &egui::Ui) -> f32 {
    ui.input(|i| match i.multi_touch() {
        Some(t) => t.zoom_delta,
        None => event_zoom(i),
    })
}

fn event_zoom(i: &egui::InputState) -> f32 {
    i.events
        .iter()
        .map(|e| match e {
            egui::Event::Zoom(f) => *f,
            _ => 1.0,
        })
        .product()
}
