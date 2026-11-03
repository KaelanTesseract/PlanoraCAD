//! Die Tabelle aller Aktionen mit ihrer Voreinstellung (Windows, wie Fusion; dazu die
//! PlanoraCAD-eigenen Kürzel, die Fusion nicht widersprechen). Eine Zeile je Aktion:
//! Variante, feste Id (Schlüssel in der Konfiguration, nie ändern), Id des Anzeigenamens
//! in den Sprachdateien,
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

            /// Id des Anzeigenamens in den Sprachdateien (`action-…`).
            fn label_id(self) -> &'static str {
                match self {
                    $(Action::$variant => $label,)*
                }
            }

            /// Anzeigename in der Sprache der Oberfläche.
            pub fn label(self) -> String {
                planora_i18n::t(self.label_id())
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
    New, "datei.neu", "action-new", All, [ctrl(Key::N)];
    Projects, "datei.projekte", "action-projects", All, [ctrl(Key::O)];
    Save, "datei.speichern", "action-save", All, [ctrl(Key::S)];
    Close, "datei.schliessen", "action-close", All, [ctrl(Key::W)];
    /// PlanoraCAD-eigenes Kürzel (Fusion hat dafür keins).
    Export, "datei.exportieren", "action-export", All, [ctrl_shift(Key::E)];
    /// Öffnet das Fenster „Rückmeldung“ (wie „Hilfe → Rückmeldung …“). PlanoraCAD-eigenes
    /// Kürzel, „R“ wie Rückmeldung; Fusion hat dafür keins.
    FeedbackOpen, "hilfe.rueckmeldung", "action-feedback", All, [ctrl_shift(Key::R)];
    Undo, "bearbeiten.rueckgaengig", "action-undo", All, [ctrl(Key::Z)];
    /// Strg+Y wie Fusion, Strg+Umschalt+Z wie bisher.
    Redo, "bearbeiten.wiederholen", "action-redo", All, [ctrl(Key::Y), ctrl_shift(Key::Z)];
    /// Mitte wie Fusion, Umschalt+links wie bisher.
    ViewPan, "ansicht.schwenken", "action-view-pan", All,
        [drag(MouseButton::Middle, false), drag(MouseButton::Left, true)];
    /// Umschalt+Mitte wie Fusion, links als Zusatz (wie bisher).
    ViewOrbit, "ansicht.drehen", "action-view-orbit", All,
        [drag(MouseButton::Middle, true), drag(MouseButton::Left, false)];
    /// Strg+Rad zusätzlich: so zoomt auch das Zusammenziehen auf Windows-Touchpads (kommt als
    /// Strg+Rad).
    ViewZoom, "ansicht.zoomen", "action-view-zoom", All, [WHEEL, CTRL_WHEEL];
    /// Wie „Erzeugen → Skizze extrudieren …“; über die Taste nur mit einer gewählten Skizze,
    /// die ein geschlossenes Profil hat.
    ExtrudeSketch, "modell.extrudieren", "action-extrude-sketch", Model, [key(Key::E)];
    /// Öffnet das Fenster Verrunden (wie „Bearbeiten → Verrunden …“).
    Fillet, "modell.verrunden", "action-fillet", Model, [key(Key::F)];
    /// Das Auge des gewählten Körpers im Feature-Baum.
    ToggleVisibility, "modell.sichtbarkeit", "action-toggle-visibility", Model, [key(Key::V)];
    /// Wie „Löschen“ im Kontextmenü des Baums (Rückfrage, wenn etwas davon abhängt).
    DeleteSelection, "modell.loeschen", "action-delete", Model, [key(Key::Delete)];
    /// Startet den zuletzt abgeschlossenen wiederholbaren Befehl noch einmal.
    RepeatLast, "modell.befehl_wiederholen", "action-repeat-last", Model, [key(Key::Space)];
    /// Öffnet das Fenster „Tastenkürzel“ mit aktivem Suchfeld (Fusion „Shortcuts“); wie
    /// „Ansicht → Tastenkürzel …“. In der Skizze gilt [`Action::SketchShowShortcuts`].
    ShowShortcuts, "ansicht.tastenkuerzel", "action-show-shortcuts", Model, [key(Key::S)];
    // Skizze: nur Werkzeuge, die es gibt (wie der Klick auf den Knopf der Leiste). Die
    // Skizzenbemaßung D kommt erst mit ihrer Funktion. Esc, Enter und Entf der Skizze sind
    // fest (`sketch_view`), nicht in der Tabelle.
    SketchLine, "skizze.linie", "action-sketch-line", Sketch, [key(Key::L)];
    // Die Arten der Formen (`sketch_kinds::ToolKind`): Der Name ist der Name der Art, der Knopf
    // der Leiste heißt weiter nach der Form (`sketch_panel::tool_label`). Nur Rechteck und Kreis
    // haben eine Vorgabetaste.
    SketchRectangle, "skizze.rechteck", "action-sketch-rect-2p", Sketch, [key(Key::R)];
    SketchCircle, "skizze.kreis", "action-sketch-circle-center", Sketch, [key(Key::C)];
    SketchRect3P, "skizze.rechteck_3p", "action-sketch-rect-3p", Sketch, [];
    SketchRectCenter, "skizze.rechteck_mitte", "action-sketch-rect-center", Sketch, [];
    SketchCircle2P, "skizze.kreis_2p", "action-sketch-circle-2p", Sketch, [];
    SketchCircle3P, "skizze.kreis_3p", "action-sketch-circle-3p", Sketch, [];
    SketchCircle2T, "skizze.kreis_2t", "action-sketch-circle-2t", Sketch, [];
    SketchCircle3T, "skizze.kreis_3t", "action-sketch-circle-3t", Sketch, [];
    SketchArcCenter, "skizze.bogen_mitte", "action-sketch-arc-center", Sketch, [];
    SketchArc3P, "skizze.bogen_3p", "action-sketch-arc-3p", Sketch, [];
    SketchArcTangent, "skizze.bogen_tangential", "action-sketch-arc-tangent", Sketch, [];
    SketchPolygonCircumscribed, "skizze.polygon_umbeschrieben",
        "action-sketch-polygon-circumscribed", Sketch, [];
    SketchPolygonInscribed, "skizze.polygon_einbeschrieben", "action-sketch-polygon-inscribed",
        Sketch, [];
    SketchPolygonEdge, "skizze.polygon_kante", "action-sketch-polygon-edge", Sketch, [];
    SketchSlotCenterToCenter, "skizze.langloch_mitte_mitte", "action-sketch-slot-center-center",
        Sketch, [];
    SketchSlotOverall, "skizze.langloch_gesamt", "action-sketch-slot-overall", Sketch, [];
    SketchSlotCenterPoint, "skizze.langloch_mittelpunkt", "action-sketch-slot-center-point",
        Sketch, [];
    SketchSlotThreePointArc, "skizze.langloch_bogen_3p", "action-sketch-slot-arc-3p", Sketch, [];
    SketchSlotCenterPointArc, "skizze.langloch_bogen_mitte", "action-sketch-slot-arc-center",
        Sketch, [];
    /// Fusion „Projizieren“.
    SketchProjectEdge, "skizze.projizieren", "action-sketch-project-edge", Sketch, [key(Key::P)];
    /// Fusion „Stutzen“ (T).
    SketchTrim, "skizze.stutzen", "action-sketch-trim", Sketch, [key(Key::T)];
    /// Fusion „Dehnen“: ohne Vorgabetaste wie in Fusion, belegbar.
    SketchExtend, "skizze.dehnen", "action-sketch-extend", Sketch, [];
    /// Fusion „Brechen“: ohne Vorgabetaste wie in Fusion, belegbar.
    SketchBreak, "skizze.brechen", "action-sketch-break", Sketch, [];
    // Skizzen 4 (Ändern). F, O und M sind Annahmen nach Fusion; `F` kollidiert nicht mit dem
    // Verrunden des Modells, weil der Bereich ein anderer ist.
    /// Fusion „Skizzenrundung“ (Art Rundung der Liste).
    SketchFillet, "skizze.rundung", "action-sketch-fillet", Sketch, [key(Key::F)];
    /// Art „Gleicher Abstand“ der Skizzenfase.
    SketchChamferEqual, "skizze.fase_gleich", "action-sketch-chamfer-equal", Sketch, [];
    /// Art „Abstand und Winkel“ der Skizzenfase.
    SketchChamferDistanceAngle, "skizze.fase_winkel", "action-sketch-chamfer-distance-angle",
        Sketch, [];
    /// Art „Zwei Abstände“ der Skizzenfase.
    SketchChamferTwoDistances, "skizze.fase_zwei", "action-sketch-chamfer-two-distances",
        Sketch, [];
    /// Fusion „Versatz“ (O).
    SketchOffset, "skizze.versatz", "action-sketch-offset", Sketch, [key(Key::O)];
    /// Fusion „Verschieben/Kopieren“ (M).
    SketchMoveCopy, "skizze.verschieben", "action-sketch-move-copy", Sketch, [key(Key::M)];
    /// Fusion „Skizzenmaßstab“: ohne Vorgabetaste, belegbar.
    SketchScale, "skizze.massstab", "action-sketch-scale", Sketch, [];
    /// Fusion „Blick auf“ (Look At): wie der Knopf in der Skizzenpalette; ohne Vorgabetaste.
    SketchLookAt, "skizze.blick_auf", "action-sketch-look-at", Sketch, [];
    /// Schalter „Skizzenraster“ der Skizzenpalette; ohne Vorgabetaste.
    SketchToggleGrid, "skizze.raster", "action-sketch-toggle-grid", Sketch, [];
    /// Schalter „Fang“ der Skizzenpalette; ohne Vorgabetaste.
    SketchToggleSnap, "skizze.fang", "action-sketch-toggle-snap", Sketch, [];
    /// Wählt das zuletzt benutzte Skizzenwerkzeug wieder (eigenes Gedächtnis, siehe
    /// `commands::LastCommand`).
    SketchRepeatLast, "skizze.werkzeug_wiederholen", "action-sketch-repeat-last", Sketch,
        [key(Key::Space)];
    /// Wie der Knopf im Skizzenfeld (Fusion „Normal / Hilfslinie“): nur mit ausgewählten Kurven.
    SketchConstruction, "skizze.konstruktion", "action-sketch-construction", Sketch, [key(Key::X)];
    /// Wie der Knopf im Skizzenfeld: schaltet die ausgewählten Linien zwischen Mittellinie und
    /// normal um (ohne Vorgabetaste).
    SketchCenterline, "skizze.mittellinie", "action-sketch-centerline", Sketch, [];
    /// Wie [`Action::ShowShortcuts`], in der offenen Skizze (eine Aktion hat nur einen Bereich).
    SketchShowShortcuts, "skizze.tastenkuerzel", "action-sketch-show-shortcuts", Sketch,
        [key(Key::S)];
    // Zeichnung: die vorhandenen Werkzeuge (wie ihr Schalter). Mittelpunktmarke C, Verschieben
    // M, Projizierte Ansicht P und Text T kommen erst mit ihrer Funktion; Entf (Nummernkreis
    // löschen) ist fest (`drawing_view`).
    /// Werkzeug „Maß“ ein/aus (Fusion „Bemaßung“).
    DrawingDimension, "zeichnung.mass", "action-drawing-dimension", Drawing, [key(Key::D)];
    /// Werkzeug „Position“ ein/aus (Fusion „Ballon“).
    DrawingBalloon, "zeichnung.position", "action-drawing-balloon", Drawing, [key(Key::B)];
}

impl Action {
    /// Merkt sich „Letzten Befehl wiederholen“ diese Aktion? Nur Erzeugen- und
    /// Bearbeiten-Befehle mit eigenem Fenster oder Werkzeug und die Werkzeuge der Skizze (mit
    /// eigenem Gedächtnis); nicht Datei, Rückgängig, Ansicht, Sichtbarkeit, Löschen, die
    /// Schalter der Zeichnung und das Wiederholen selbst.
    pub fn repeatable(self) -> bool {
        matches!(self, Action::ExtrudeSketch | Action::Fillet) || self.sketch_tool().is_some()
    }

    /// Das Skizzenwerkzeug, das diese Aktion wählt.
    pub fn sketch_tool(self) -> Option<crate::sketch_state::Tool> {
        use crate::sketch_state::Tool;
        match self {
            Action::SketchLine => Some(Tool::Line),
            Action::SketchProjectEdge => Some(Tool::ProjectEdge),
            Action::SketchTrim => Some(Tool::Trim),
            Action::SketchExtend => Some(Tool::Extend),
            Action::SketchBreak => Some(Tool::Break),
            Action::SketchOffset => Some(Tool::Offset),
            Action::SketchMoveCopy => Some(Tool::MoveCopy),
            Action::SketchScale => Some(Tool::Scale),
            _ => self.sketch_kind().map(crate::sketch_kinds::ToolKind::tool),
        }
    }

    /// Die Art einer Form, die diese Aktion wählt.
    pub fn sketch_kind(self) -> Option<crate::sketch_kinds::ToolKind> {
        crate::sketch_kinds::ToolKind::ALL
            .iter()
            .copied()
            .find(|k| k.action() == self)
    }

    /// Die Aktion, die die Art `kind` wählt.
    pub fn for_kind(kind: crate::sketch_kinds::ToolKind) -> Action {
        kind.action()
    }

    /// Die Aktion, die das Skizzenwerkzeug `tool` wählt (nicht jedes Werkzeug hat eine); bei
    /// einer Form die ihrer ersten Art.
    pub fn for_sketch_tool(tool: crate::sketch_state::Tool) -> Option<Action> {
        Action::ALL
            .iter()
            .copied()
            .find(|a| a.sketch_tool() == Some(tool))
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
