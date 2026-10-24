//! Die Tabelle aller Aktionen mit ihrer Voreinstellung (Windows, wie Fusion; dazu die
//! PlanoraCAD-eigenen Kürzel, die Fusion nicht widersprechen). Eine Zeile je Aktion:
//! Variante, feste Id (Schlüssel in der Konfiguration, nie ändern), deutscher Anzeigename,
//! Bereich, Voreinstellung. Daraus entstehen [`Action`], [`Action::ALL`] und die
//! Voreinstellung von [`super::Keymap::defaults`].

use super::{Area, Binding, Gesture, KeyChord, MouseButton};
use egui::Key;

/// Erzeugt [`Action`] samt Id, Anzeigename, Bereich und Voreinstellung aus einer Tabelle.
macro_rules! action_table {
    ($($(#[$doc:meta])* $variant:ident, $id:literal, $label:literal, $area:ident, [$($bind:expr),* $(,)?];)*) => {
        /// Eine Aktion, die eine Taste oder Mausgeste auslösen kann.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Action {
            $($(#[$doc])* $variant,)*
        }

        impl Action {
            /// Alle Aktionen in der Reihenfolge der Tabelle.
            pub const ALL: &'static [Action] = &[$(Action::$variant),*];

            /// Fester Name (Schlüssel in der Konfiguration), z. B. `"datei.neu"`.
            pub fn id(self) -> &'static str {
                match self {
                    $(Action::$variant => $id,)*
                }
            }

            /// Deutscher Anzeigename.
            pub fn label(self) -> &'static str {
                match self {
                    $(Action::$variant => $label,)*
                }
            }

            /// Bereich, in dem die Belegung gilt.
            pub fn area(self) -> Area {
                match self {
                    $(Action::$variant => Area::$area,)*
                }
            }

            /// Voreinstellung (kann leer sein).
            pub fn default_bindings(self) -> &'static [Binding] {
                match self {
                    $(Action::$variant => {
                        const B: &[Binding] = &[$($bind),*];
                        B
                    })*
                }
            }
        }
    };
}

action_table! {
    New, "datei.neu", "Neues Dokument", All, [ctrl(Key::N)];
    Projects, "datei.projekte", "Projekte", All, [ctrl(Key::O)];
    Save, "datei.speichern", "Speichern", All, [ctrl(Key::S)];
    Close, "datei.schliessen", "Schließen", All, [ctrl(Key::W)];
    /// PlanoraCAD-eigenes Kürzel (Fusion hat dafür keins).
    Export, "datei.exportieren", "Exportieren", All, [ctrl_shift(Key::E)];
    Undo, "bearbeiten.rueckgaengig", "Rückgängig", All, [ctrl(Key::Z)];
    /// Strg+Y wie Fusion, Strg+Umschalt+Z wie bisher.
    Redo, "bearbeiten.wiederholen", "Wiederholen", All, [ctrl(Key::Y), ctrl_shift(Key::Z)];
    /// Mitte wie Fusion, Umschalt+links wie bisher.
    ViewPan, "ansicht.schwenken", "Ansicht schwenken", All,
        [drag(MouseButton::Middle, false), drag(MouseButton::Left, true)];
    /// Umschalt+Mitte wie Fusion, links als Zusatz (wie bisher).
    ViewOrbit, "ansicht.drehen", "Ansicht drehen", All,
        [drag(MouseButton::Middle, true), drag(MouseButton::Left, false)];
    /// Strg+Rad zusätzlich: so zoomt auch das Zusammenziehen auf Windows-Touchpads (kommt als
    /// Strg+Rad).
    ViewZoom, "ansicht.zoomen", "Ansicht zoomen", All, [WHEEL, CTRL_WHEEL];
    /// Wie „Erzeugen → Skizze extrudieren …“; über die Taste nur mit einer gewählten Skizze,
    /// die ein geschlossenes Profil hat.
    ExtrudeSketch, "modell.extrudieren", "Skizze extrudieren", Model, [key(Key::E)];
    /// Öffnet das Fenster Verrunden (wie „Bearbeiten → Verrunden …“).
    Fillet, "modell.verrunden", "Verrunden", Model, [key(Key::F)];
    /// Das Auge des gewählten Körpers im Feature-Baum.
    ToggleVisibility, "modell.sichtbarkeit", "Sichtbarkeit umschalten", Model, [key(Key::V)];
    /// Wie „Löschen“ im Kontextmenü des Baums (Rückfrage, wenn etwas davon abhängt).
    DeleteSelection, "modell.loeschen", "Löschen", Model, [key(Key::Delete)];
    /// Startet den zuletzt abgeschlossenen wiederholbaren Befehl noch einmal.
    RepeatLast, "modell.befehl_wiederholen", "Letzten Befehl wiederholen", Model, [key(Key::Space)];
}

impl Action {
    /// Merkt sich „Letzten Befehl wiederholen“ diese Aktion? Nur Erzeugen- und
    /// Bearbeiten-Befehle mit eigenem Fenster oder Werkzeug; nicht Datei, Rückgängig,
    /// Ansicht, Sichtbarkeit, Löschen und das Wiederholen selbst.
    pub fn repeatable(self) -> bool {
        matches!(self, Action::ExtrudeSketch | Action::Fillet)
    }

    /// Stelle in [`Action::ALL`] (die Varianten stehen dort in Tabellenreihenfolge).
    pub(super) fn index(self) -> usize {
        self as usize
    }

    /// Die Aktion zu einer Id aus der Konfiguration.
    pub fn from_id(id: &str) -> Option<Action> {
        Action::ALL.iter().copied().find(|a| a.id() == id)
    }
}

const fn key(key: Key) -> Binding {
    Binding::Key(KeyChord {
        key,
        ctrl: false,
        shift: false,
        alt: false,
    })
}

const fn ctrl(key: Key) -> Binding {
    Binding::Key(KeyChord {
        key,
        ctrl: true,
        shift: false,
        alt: false,
    })
}

const fn ctrl_shift(key: Key) -> Binding {
    Binding::Key(KeyChord {
        key,
        ctrl: true,
        shift: true,
        alt: false,
    })
}

const fn drag(button: MouseButton, shift: bool) -> Binding {
    Binding::Mouse(Gesture::Drag {
        button,
        ctrl: false,
        shift,
        alt: false,
    })
}

const WHEEL: Binding = Binding::Mouse(Gesture::Wheel {
    ctrl: false,
    shift: false,
    alt: false,
});

const CTRL_WHEEL: Binding = Binding::Mouse(Gesture::Wheel {
    ctrl: true,
    shift: false,
    alt: false,
});
