//! Profilwahl der Dialoge „Skizze extrudieren“, „Skizze drehen“ und „Bearbeiten“ (Extrusion,
//! Rotation), wie in Fusion: Klick in die 3D-Ansicht schaltet ein Profil um, die Liste mit
//! Häkchen ist der zweite Weg. Beide wirken auf dieselbe Wahl (Indizes in `profiles()` der
//! berechneten Skizze).
//!
//! Ablauf je Bild: Die 3D-Ansicht (vor den Dialogen gezeichnet) trägt Hover und Klick in
//! [`AppState::profile_pick`] ein; der Dialog übernimmt den Klick ([`take_click`]), zeichnet die
//! Liste ([`list`]) und legt die Wahl für die Ansicht ab ([`publish`]). Ohne offenen Dialog mit
//! Profilwahl löscht die App die Wahl ([`AppState::profile_pick`] = `None`).

use crate::state::{AppState, ProfilePick};
use planora_feature::FeatureId;
use planora_i18n::tr;
use planora_sketch::ExtractedProfiles;

/// Schaltet Profil `i` in der Wahl `sel` um (aufsteigend sortiert).
pub fn toggle(sel: &mut Vec<usize>, i: usize) {
    match sel.iter().position(|&k| k == i) {
        Some(k) => {
            sel.remove(k);
        }
        None => {
            sel.push(i);
            sel.sort_unstable();
        }
    }
}

/// Übernimmt einen Klick der 3D-Ansicht auf ein Profil der Skizze `sketch` in die Wahl `sel`;
/// `true`, wenn sich die Wahl geändert hat. Ein Klick auf eine andere Skizze verfällt.
pub fn take_click(state: &mut AppState, sketch: FeatureId, sel: &mut Vec<usize>) -> bool {
    let Some(pick) = state.profile_pick.as_mut() else {
        return false;
    };
    let clicked = pick.clicked.take();
    match clicked {
        Some(i) if pick.sketch == sketch => {
            toggle(sel, i);
            true
        }
        _ => false,
    }
}

/// Legt die Wahl `sel` der Skizze `sketch` für die 3D-Ansicht ab; das Profil unter dem
/// Mauszeiger bleibt, solange die Skizze dieselbe ist.
pub fn publish(state: &mut AppState, sketch: FeatureId, sel: &[usize]) {
    let hover = state
        .profile_pick
        .as_ref()
        .filter(|p| p.sketch == sketch)
        .and_then(|p| p.hover);
    state.profile_pick = Some(ProfilePick {
        sketch,
        selected: sel.to_vec(),
        hover,
        clicked: None,
    });
}

/// Bezeichnung von Profil `i` in der Liste („Profil 2: 12,00 mm²“, mit Zahl der Löcher).
pub fn label(extracted: &ExtractedProfiles, i: usize) -> String {
    let p = &extracted.profiles[i];
    let area =
        p.outer.signed_area().abs() - p.holes.iter().map(|h| h.signed_area().abs()).sum::<f64>();
    match p.holes.len() {
        0 => tr!(
            "create-profile-label",
            n = i + 1,
            area = planora_i18n::fmt_area_num(area),
            unit = planora_i18n::current_unit().area_symbol()
        ),
        holes => tr!(
            "create-profile-label-holes",
            n = i + 1,
            area = planora_i18n::fmt_area_num(area),
            unit = planora_i18n::current_unit().area_symbol(),
            holes = holes
        ),
    }
}

/// Liste der Profile mit Häkchen (Klick schaltet um), darunter der Hinweis, wenn nichts gewählt
/// ist, und die nicht verwendbaren Flächen mit Grund. `true`, wenn sich die Wahl geändert hat.
pub fn list(ui: &mut egui::Ui, extracted: &ExtractedProfiles, sel: &mut Vec<usize>) -> bool {
    ui.label(tr!("create-profile"));
    let mut changed = false;
    if extracted.profiles.is_empty() {
        ui.weak(tr!("create-no-profile"));
    }
    for i in 0..extracted.profiles.len() {
        let mut on = sel.contains(&i);
        if ui.checkbox(&mut on, label(extracted, i)).changed() {
            toggle(sel, i);
            changed = true;
        }
    }
    if sel.is_empty() && !extracted.profiles.is_empty() {
        ui.weak(tr!("create-pick-profile"));
    }
    for r in &extracted.rejected {
        ui.weak(tr!("create-rejected", reason = r.reason.to_string()));
    }
    changed
}
