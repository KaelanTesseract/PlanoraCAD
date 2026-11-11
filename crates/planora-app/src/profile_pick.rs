//! Profilwahl der Dialoge „Skizze extrudieren“, „Skizze drehen“ und „Bearbeiten“ (Extrusion,
//! Rotation), wie in Fusion: Klick in die 3D-Ansicht schaltet ein Profil um, die Liste mit
//! Häkchen ist der zweite Weg. Beide wirken auf dieselbe Wahl (Indizes in `profiles()` der
//! berechneten Skizze).
//!
//! Ganzer Text als Profil (Skizzen 8, Spec D2): Die Profile eines Textes bilden eine Gruppe
//! ([`groups`]). Ein einfacher Klick auf eines davon schaltet alle gemeinsam um
//! ([`toggle_group`]), Strg+Klick nur das eine; an anderen Profilen wirkt Strg+Klick wie ein
//! einfacher Klick, Umschalt hat keine eigene Bedeutung. In der Liste steht je Text eine Zeile,
//! deren Häkchen den Zwischenzustand zeigt, wenn nur ein Teil gewählt ist.
//!
//! Ablauf je Bild: Die 3D-Ansicht (vor den Dialogen gezeichnet) trägt Hover und Klick (mit den
//! gedrückten Tasten) in [`AppState::profile_pick`] ein; der Dialog übernimmt den Klick
//! ([`take_click`]), zeichnet die Liste ([`list`]) und legt die Wahl für die Ansicht ab
//! ([`publish`]). Ohne offenen Dialog mit Profilwahl löscht die App die Wahl
//! ([`AppState::profile_pick`] = `None`).

use crate::state::{AppState, ProfilePick};
use planora_feature::FeatureId;
use planora_i18n::tr;
use planora_sketch::model::{CurveId, Sketch};
use planora_sketch::text::TextId;
use planora_sketch::{ExtractedProfiles, ProfileRef};
use std::collections::HashMap;

/// Eine Zeile der Profilwahl: ein einzelnes Profil oder alle Profile eines Textes (Spec D2).
/// `members` sind aufsteigend, die Gruppen stehen in der Reihenfolge ihres ersten Profils.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileGroup {
    Single(usize),
    Text { text: TextId, members: Vec<usize> },
}

/// Häkchen einer Gruppe: keins, ein Teil oder alle ihrer Profile gewählt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckState {
    None,
    Partial,
    All,
}

/// Stand des Häkchens der Profile `members` in der Wahl `sel` (leere Gruppe: keins).
pub fn check_state(members: &[usize], sel: &[usize]) -> CheckState {
    let on = members.iter().filter(|i| sel.contains(i)).count();
    if on == 0 {
        CheckState::None
    } else if on == members.len() {
        CheckState::All
    } else {
        CheckState::Partial
    }
}

/// Schaltet die Profile `group` gemeinsam um: Sind alle gewählt, werden alle abgewählt, sonst
/// alle gewählt. `sel` bleibt aufsteigend sortiert und ohne Doppel.
pub fn toggle_group(sel: &mut Vec<usize>, group: &[usize]) {
    if check_state(group, sel) == CheckState::All {
        sel.retain(|i| !group.contains(i));
    } else {
        for &i in group {
            if !sel.contains(&i) {
                sel.push(i);
            }
        }
        sel.sort_unstable();
    }
}

/// Die Gruppen der Profile `extracted` der Skizze `sketch`: Ein Profil, dessen Kanten (Rand und
/// Löcher) alle aus Kurven desselben lebenden Textes stammen, gehört zur Gruppe dieses Textes
/// (wie [`Sketch::text_of_profile`]); jedes andere ist eine Gruppe für sich. Die Zuordnung der
/// Kurven zu Texten entsteht einmal für alle Profile, nicht je Profil.
pub fn groups(extracted: &ExtractedProfiles, sketch: &Sketch) -> Vec<ProfileGroup> {
    let owner: HashMap<CurveId, TextId> = sketch
        .texts()
        .flat_map(|(t, d)| d.curves.iter().map(move |&c| (c, t)))
        .collect();
    let mut out = Vec::new();
    // Stelle der Gruppe je Text in `out`.
    let mut slot: HashMap<TextId, usize> = HashMap::new();
    for i in 0..extracted.profiles.len() {
        let text = extracted.refs.get(i).and_then(|r| text_of(r, &owner));
        match text {
            Some(t) => match slot.get(&t) {
                Some(&k) => {
                    if let ProfileGroup::Text { members, .. } = &mut out[k] {
                        members.push(i);
                    }
                }
                None => {
                    slot.insert(t, out.len());
                    out.push(ProfileGroup::Text {
                        text: t,
                        members: vec![i],
                    });
                }
            },
            None => out.push(ProfileGroup::Single(i)),
        }
    }
    out
}

/// Der Text, aus dem alle Kanten von `r` stammen (nach der Zuordnung `owner`).
fn text_of(r: &ProfileRef, owner: &HashMap<CurveId, TextId>) -> Option<TextId> {
    let mut texts = r
        .outer
        .iter()
        .chain(&r.holes)
        .map(|p| owner.get(&p.curve).copied());
    let first = texts.next()??;
    texts.all(|t| t == Some(first)).then_some(first)
}

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
/// `true`, wenn sich die Wahl geändert hat. Ein Klick auf eine andere Skizze verfällt. Die beim
/// Klick gedrückten Tasten entscheiden: Ohne Strg schaltet ein Klick auf ein Textprofil alle
/// Profile dieses Textes um ([`toggle_group`]), mit Strg nur das eine; andere Profile schalten
/// immer einzeln um.
pub fn take_click(state: &mut AppState, sketch: FeatureId, sel: &mut Vec<usize>) -> bool {
    let Some(pick) = state.profile_pick.as_mut() else {
        return false;
    };
    let Some((i, modifiers)) = pick.clicked.take() else {
        return false;
    };
    if pick.sketch != sketch {
        return false;
    }
    // Strg (unter macOS Befehl) wählt einzeln.
    let single = modifiers.ctrl || modifiers.command;
    if !single && let Some(sk) = state.document.sketch_of(sketch) {
        let groups = state.profile_cache.groups(sk);
        let text = groups.iter().find_map(|g| match g {
            ProfileGroup::Text { members, .. } if members.contains(&i) => Some(members),
            _ => None,
        });
        if let Some(members) = text {
            toggle_group(sel, members);
            return true;
        }
    }
    toggle(sel, i);
    true
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

/// Bezeichnung der Zeile des Textes `t` in der Liste („Text „Hallo““); Zeilenumbrüche des
/// Inhalts werden Leerzeichen, damit die Zeile eine Zeile bleibt.
pub fn text_label(sketch: &Sketch, t: TextId) -> String {
    let content = sketch
        .text(t)
        .map(|d| d.content.lines().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    tr!("create-profile-text", content = content)
}

/// Liste der Profile mit Häkchen (Klick schaltet um), darunter der Hinweis, wenn nichts gewählt
/// ist, und die nicht verwendbaren Flächen mit Grund. Je Text eine Zeile für alle seine Profile
/// (`groups` aus [`crate::state::ProfileCache::groups`] zum selben Stand wie `extracted`), mit
/// Zwischenzustand, wenn nur ein Teil gewählt ist. `true`, wenn sich die Wahl geändert hat.
pub fn list(
    ui: &mut egui::Ui,
    extracted: &ExtractedProfiles,
    groups: &[ProfileGroup],
    sketch: &Sketch,
    sel: &mut Vec<usize>,
) -> bool {
    ui.label(tr!("create-profile"));
    let mut changed = false;
    if extracted.profiles.is_empty() {
        ui.weak(tr!("create-no-profile"));
    }
    for g in groups {
        match g {
            ProfileGroup::Single(i) => {
                let mut on = sel.contains(i);
                if ui.checkbox(&mut on, label(extracted, *i)).changed() {
                    toggle(sel, *i);
                    changed = true;
                }
            }
            ProfileGroup::Text { text, members } => {
                let state = check_state(members, sel);
                let mut on = state == CheckState::All;
                let check = egui::Checkbox::new(&mut on, text_label(sketch, *text))
                    .indeterminate(state == CheckState::Partial);
                if ui.add(check).changed() {
                    toggle_group(sel, members);
                    changed = true;
                }
            }
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
