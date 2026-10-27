//! Tastenbelegung: welche Taste oder Mausgeste welche Aktion auslöst, in welchem Bereich sie
//! gilt, Doppelbelegungen, Ändern/Tauschen/Zurücksetzen und der Weg in die Konfiguration (nur
//! Abweichungen von der Voreinstellung). Reine Logik ohne Zeichnen; der Verteiler, der die
//! Tastenereignisse eines Bildes in Aktionen übersetzt, steht in [`dispatch`].
//!
//! Alle Aktionen stehen in einer einzigen Tabelle in [`defaults`] (Variante, Id, Anzeigename,
//! Bereich, Voreinstellung); eine neue Aktion braucht dort nur eine Zeile.

pub mod defaults;
pub mod dispatch;
pub mod mouse;
pub mod overview;
pub mod search;
pub mod settings_ui;
pub mod text;

pub use defaults::Action;

use std::collections::BTreeMap;

/// Wo eine Belegung gilt: Skizze offen → Skizze, Zeichnung offen → Zeichnung, sonst Modell.
/// „Überall“ gilt in jedem Bereich.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Area {
    All,
    Model,
    Sketch,
    Drawing,
}

impl Area {
    /// Anzeigename für Einstellungen und Übersicht.
    pub fn label(self) -> String {
        match self {
            Area::All => planora_i18n::tr!("area-all"),
            Area::Model => planora_i18n::tr!("area-model"),
            Area::Sketch => planora_i18n::tr!("area-sketch"),
            Area::Drawing => planora_i18n::tr!("area-drawing"),
        }
    }
}

/// Eine Taste mit genau diesen Umschalttasten (`ctrl` ist `Modifiers::COMMAND`, unter Windows
/// Strg). Nicht gehaltene Umschalttasten müssen auch beim Drücken fehlen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: egui::Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// Maustaste einer Ziehgeste.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
}

/// Mausgeste der Ansicht: Ziehen mit einer Taste oder Drehen am Rad, jeweils mit genau diesen
/// Umschalttasten. Zwei Gesten sind gleich, wenn Art, Taste und Umschalttasten gleich sind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Gesture {
    Drag {
        button: MouseButton,
        ctrl: bool,
        shift: bool,
        alt: bool,
    },
    Wheel {
        ctrl: bool,
        shift: bool,
        alt: bool,
    },
}

/// Eine Belegung: Taste oder Mausgeste. Tasten und Gesten sind nie gleich, zählen also nie
/// gegeneinander als Doppelbelegung.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Binding {
    Key(KeyChord),
    Mouse(Gesture),
}

/// Ergebnis von [`Keymap::set`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetResult {
    Ok,
    /// Die Belegung gehört schon dieser Aktion (gleicher Bereich oder „Überall“); nichts wurde
    /// geändert.
    Conflict(Action),
}

/// Belegungen aller Aktionen; eine Aktion darf mehrere oder keine haben (sie bleibt dann in
/// der Liste, löst aber nichts aus).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    /// Je Aktion (Index = Stelle in [`Action::ALL`]) ihre Belegungen in Anzeigereihenfolge.
    map: Vec<Vec<Binding>>,
}

impl Default for Keymap {
    fn default() -> Self {
        Keymap::defaults()
    }
}

impl Keymap {
    /// Voreinstellung (wie Fusion, siehe [`defaults`]).
    pub fn defaults() -> Keymap {
        Keymap {
            map: Action::ALL
                .iter()
                .map(|a| a.default_bindings().to_vec())
                .collect(),
        }
    }

    fn empty() -> Keymap {
        Keymap {
            map: vec![Vec::new(); Action::ALL.len()],
        }
    }

    pub fn bindings(&self, a: Action) -> &[Binding] {
        &self.map[a.index()]
    }

    /// Hat `a` genau die Voreinstellung (auch in deren Reihenfolge)?
    pub fn is_default(&self, a: Action) -> bool {
        self.bindings(a) == a.default_bindings()
    }

    /// Können sich Belegungen der beiden Bereiche stören? Ja im selben Bereich und immer, wenn
    /// eine Seite „Überall“ ist; verschiedene Bereiche dürfen dieselbe Taste haben.
    pub fn areas_overlap(a: Area, b: Area) -> bool {
        a == b || a == Area::All || b == Area::All
    }

    /// Die andere Aktion, der `b` schon gehört und die sich im Bereich mit `a` stört; die
    /// eigenen Belegungen von `a` zählen nicht.
    pub fn conflict_for(&self, a: Action, b: &Binding) -> Option<Action> {
        Action::ALL.iter().copied().find(|&o| {
            o != a && Keymap::areas_overlap(a.area(), o.area()) && self.bindings(o).contains(b)
        })
    }

    /// Setzt die Belegung an Stelle `index` von `a` (hinter dem Ende: angehängt). Bei einer
    /// Doppelbelegung bleibt alles, wie es war. Hat `a` dieselbe Belegung schon an anderer
    /// Stelle, entfällt diese (keine doppelte Zeile).
    pub fn set(&mut self, a: Action, index: usize, b: Binding) -> SetResult {
        if let Some(o) = self.conflict_for(a, &b) {
            return SetResult::Conflict(o);
        }
        self.put(a, index, b);
        SetResult::Ok
    }

    /// Wie [`Keymap::set`], nimmt aber bei einer Doppelbelegung die Belegung der anderen
    /// Aktion weg (ersatzlos; ihre übrigen Belegungen bleiben) und setzt sie bei `a`.
    pub fn swap(&mut self, a: Action, index: usize, b: Binding) {
        for &o in Action::ALL.iter() {
            if o != a && Keymap::areas_overlap(a.area(), o.area()) {
                self.map[o.index()].retain(|x| *x != b);
            }
        }
        self.put(a, index, b);
    }

    fn put(&mut self, a: Action, index: usize, b: Binding) {
        let list = &mut self.map[a.index()];
        let at = if index < list.len() {
            list[index] = b;
            index
        } else {
            list.push(b);
            list.len() - 1
        };
        let mut i = 0;
        list.retain(|x| {
            let keep = i == at || *x != b;
            i += 1;
            keep
        });
    }

    /// Nimmt die Belegung an Stelle `index` von `a` weg (gibt es sie nicht: nichts). Ohne
    /// Belegung bleibt die Aktion in der Liste, löst aber nichts aus.
    pub fn remove(&mut self, a: Action, index: usize) {
        let list = &mut self.map[a.index()];
        if index < list.len() {
            list.remove(index);
        }
    }

    /// Eine Aktion auf ihre Voreinstellung. Gehört eine davon inzwischen einer anderen Aktion
    /// (im Bereich störend), verliert diese sie (wie bei [`Keymap::swap`]), damit keine Taste
    /// zweimal belegt ist; zurückgegeben wird, was wem weggenommen wurde.
    pub fn reset(&mut self, a: Action) -> Vec<(Binding, Action)> {
        let mut taken = Vec::new();
        for b in a.default_bindings() {
            while let Some(o) = self.conflict_for(a, b) {
                self.map[o.index()].retain(|x| x != b);
                taken.push((*b, o));
            }
        }
        self.map[a.index()] = a.default_bindings().to_vec();
        taken
    }

    /// Alle Aktionen auf die Voreinstellung.
    pub fn reset_all(&mut self) {
        *self = Keymap::defaults();
    }

    /// Löst `chord` (genau diese Umschalttasten) die Aktion `a` aus?
    pub fn matches_key(&self, a: Action, chord: &KeyChord) -> bool {
        self.bindings(a).contains(&Binding::Key(*chord))
    }

    /// Die Aktion zu einer Taste im Bereich `area`: erst die des Bereichs, dann die von
    /// „Überall“.
    pub fn action_for_key(&self, area: Area, chord: &KeyChord) -> Option<Action> {
        let find = |want: Area| {
            Action::ALL
                .iter()
                .copied()
                .find(|&a| a.area() == want && self.matches_key(a, chord))
        };
        if area == Area::All {
            find(Area::All)
        } else {
            find(area).or_else(|| find(Area::All))
        }
    }

    /// Baut die Belegung aus den Abweichungen der Konfiguration (Id → Texte). Was nicht passt,
    /// wird übergangen und als kurzer Hinweis gemeldet: unbekannte Aktion, ungültige Belegung,
    /// Doppelbelegung in der Datei. Bei einer Doppelbelegung gewinnt die Aktion, die in der
    /// Tabelle zuerst steht; eine Belegung aus der Datei geht immer der Voreinstellung einer
    /// anderen Aktion vor (die Voreinstellung entfällt dann). Bleibt von einem nicht leeren
    /// Eintrag nichts Brauchbares, gilt die Voreinstellung; eine leere Liste heißt „ohne
    /// Belegung“.
    pub fn from_overrides(overrides: &BTreeMap<String, Vec<String>>) -> (Keymap, Vec<String>) {
        let mut notes = Vec::new();
        let mut wanted: Vec<Option<(Vec<Binding>, bool)>> = vec![None; Action::ALL.len()];
        for (id, texts) in overrides {
            let Some(a) = Action::from_id(id) else {
                notes.push(planora_i18n::tr!("keymap-note-unknown-action", id = id));
                continue;
            };
            let mut list = Vec::new();
            for t in texts {
                match text::parse_binding(t) {
                    Some(b) if Keymap::fixed_key_text(a, &b).is_some() => {
                        notes.push(planora_i18n::tr!(
                            "keymap-note-fixed",
                            binding = t,
                            action = a.label(),
                            reason = Keymap::fixed_key_text(a, &b).unwrap_or_default()
                        ))
                    }
                    Some(b) => list.push(b),
                    None => notes.push(planora_i18n::tr!(
                        "keymap-note-invalid",
                        binding = t,
                        action = a.label()
                    )),
                }
            }
            // Nur Ungültiges: als fehlte der Eintrag.
            if texts.is_empty() || !list.is_empty() {
                wanted[a.index()] = Some((list, texts.is_empty()));
            }
        }
        let mut km = Keymap::empty();
        let mut from_file = vec![false; Action::ALL.len()];
        // Zuerst die Belegungen aus der Datei, in der Reihenfolge der Tabelle.
        for &a in Action::ALL {
            let Some((list, explicit_empty)) = &wanted[a.index()] else {
                continue;
            };
            for b in list {
                if let Some(o) = km.conflict_for(a, b) {
                    notes.push(planora_i18n::tr!(
                        "keymap-note-taken",
                        binding = text::binding_text(b),
                        action = a.label(),
                        other = o.label()
                    ));
                } else if !km.map[a.index()].contains(b) {
                    // Zweimal dieselbe Belegung bei einer Aktion schadet nicht: still einmal.
                    km.map[a.index()].push(*b);
                }
            }
            from_file[a.index()] = *explicit_empty || !km.map[a.index()].is_empty();
        }
        // Dann die Voreinstellung der übrigen Aktionen, soweit die Datei sie nicht vergeben hat.
        for &a in Action::ALL {
            if from_file[a.index()] {
                continue;
            }
            for b in a.default_bindings() {
                match km.conflict_for(a, b) {
                    Some(o) => notes.push(planora_i18n::tr!(
                        "keymap-note-default-lost",
                        binding = text::binding_text(b),
                        action = a.label(),
                        other = o.label()
                    )),
                    None => km.map[a.index()].push(*b),
                }
            }
        }
        cap_notes(&mut notes);
        (km, notes)
    }

    /// Ist `b` für `a` eine Taste, die eine Ansicht fest braucht (Eingabe und Entf in der Skizze,
    /// Entf in der Zeichnung, Tab zum Wechseln des Fokus, Pfeil links/rechts an der
    /// Verlaufsleiste für Aktionen des Modells)? Dann der Hinweis „„Entf“ ist in der Skizze fest
    /// belegt.“; solche Tasten ließen sich sonst belegen, würden aber vom Verteiler verschluckt.
    pub fn fixed_key_text(a: Action, b: &Binding) -> Option<String> {
        let Binding::Key(c) = b else {
            return None;
        };
        let name = text::key_name(c.key);
        let plain = !c.ctrl && !c.alt;
        let place = match (c.key, a.area()) {
            (egui::Key::Enter | egui::Key::Delete, Area::Sketch) => {
                planora_i18n::tr!("keymap-fixed-sketch")
            }
            (egui::Key::Delete, Area::Drawing) => planora_i18n::tr!("keymap-fixed-drawing"),
            (egui::Key::Tab, _) if plain => planora_i18n::tr!("keymap-fixed-tab"),
            (egui::Key::ArrowLeft | egui::Key::ArrowRight, Area::Model) if plain && !c.shift => {
                planora_i18n::tr!("keymap-fixed-history-bar")
            }
            _ => return None,
        };
        Some(planora_i18n::tr!("keymap-fixed", key = name, place = place))
    }

    /// Abweichungen von der Voreinstellung für die Konfiguration (Id → Texte); eine Aktion ohne
    /// Belegung steht dort als leere Liste.
    pub fn to_overrides(&self) -> BTreeMap<String, Vec<String>> {
        Action::ALL
            .iter()
            .filter(|&&a| !self.is_default(a))
            .map(|&a| {
                let texts = self.bindings(a).iter().map(text::stored_text).collect();
                (a.id().to_string(), texts)
            })
            .collect()
    }

    /// Alle Belegungen von `a` als Text, mit „ / “ getrennt; ohne Belegung „–“.
    pub fn text(&self, a: Action) -> String {
        let list = self.bindings(a);
        if list.is_empty() {
            return planora_i18n::tr!("keymap-unbound-dash");
        }
        list.iter()
            .map(text::binding_text)
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

/// Höchstens so viele Hinweise aus [`Keymap::from_overrides`]; der Rest wird nur gezählt.
pub const MAX_NOTES: usize = 20;

/// Kürzt `notes` auf [`MAX_NOTES`] und hängt „… und N weitere“ an (eine zerschossene Datei
/// soll den Abschnitt nicht mit Hunderten Zeilen füllen).
fn cap_notes(notes: &mut Vec<String>) {
    if notes.len() > MAX_NOTES {
        let more = notes.len() - MAX_NOTES;
        notes.truncate(MAX_NOTES);
        notes.push(planora_i18n::tr!("keymap-notes-more", count = more));
    }
}

/// Tastentext für Menüs, Tooltips und Hinweise: die erste Tastenbelegung von `a` („Strg+Y“ bei
/// Wiederholen, nicht beide: rechts im Menü ist nur für eine Platz, die übrigen nennen
/// Einstellungen und Übersicht). Ohne Tastenbelegung leer.
pub fn shortcut_hint(km: &Keymap, a: Action) -> String {
    km.bindings(a)
        .iter()
        .find_map(|b| match b {
            Binding::Key(c) => Some(text::chord_text(c)),
            Binding::Mouse(_) => None,
        })
        .unwrap_or_default()
}

/// Tooltip „`text` (Taste)“ mit [`shortcut_hint`]; ohne Taste nur `text`.
pub fn with_hint(km: &Keymap, a: Action, text: &str) -> String {
    match shortcut_hint(km, a) {
        k if k.is_empty() => text.to_string(),
        k => format!("{text} ({k})"),
    }
}
