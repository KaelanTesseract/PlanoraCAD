//! Sichtbarkeit im Browser (Augen): gehört zum Dokument und wird mit ihm gespeichert, wie in
//! Fusion. Der Ordner-Schalter steht für sich: Er blendet alles darin aus und lässt die Schalter
//! der einzelnen Zeilen stehen. Ändert nur die Ansicht, nicht das Modell.

use crate::model::{Document, FeatureId, FeatureKind, StandardPlane};
use planora_drawing::BodyKey;
use std::collections::BTreeSet;

/// Ausgeblendete Ordner des Browsers (Grundebenen haben keinen eigenen Ordner-Schalter: Das Auge
/// am Ordner „Ursprung“ schaltet die drei Ebenen selbst).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HiddenFolders {
    pub bodies: bool,
    pub sketches: bool,
    pub planes: bool,
}

/// Was im Browser ausgeblendet ist. Der Grundzustand zeigt alle Körper, Skizzen und
/// Konstruktionsebenen und keine Grundebene (wie Fusion); nur Abweichungen stehen in der Datei.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Visibility {
    /// Selbst ausgeblendete Körper.
    pub bodies: BTreeSet<BodyKey>,
    /// Selbst ausgeblendete Skizzen und Konstruktionsebenen.
    pub features: BTreeSet<FeatureId>,
    /// Eingeblendete Grundebenen (XY, XZ, YZ).
    pub origin_planes: [bool; 3],
    pub folders: HiddenFolders,
}

fn origin_index(p: StandardPlane) -> usize {
    match p {
        StandardPlane::XY => 0,
        StandardPlane::XZ => 1,
        StandardPlane::YZ => 2,
    }
}

impl Visibility {
    /// Der Grundzustand? Dann bleibt die Datei, wie sie war.
    pub fn is_default(&self) -> bool {
        *self == Visibility::default()
    }

    pub fn origin_shown(&self, p: StandardPlane) -> bool {
        self.origin_planes[origin_index(p)]
    }

    pub fn set_origin_shown(&mut self, p: StandardPlane, shown: bool) {
        self.origin_planes[origin_index(p)] = shown;
    }
}

impl Document {
    pub fn visibility(&self) -> &Visibility {
        &self.visibility
    }

    /// Setzt die Sichtbarkeit; Verweise auf nicht vorhandene oder nicht ausblendbare Features
    /// fallen weg. Der Änderungszähler steigt nur bei einer Änderung und das Modell rechnet nicht
    /// neu ([`Document::edit_count`], nicht [`Document::model_edit_count`]).
    pub fn set_visibility(&mut self, v: Visibility) {
        let v = self.pruned_visibility(v);
        if v != self.visibility {
            self.visibility = v;
            self.drawing_edits += 1;
        }
    }

    /// `v` ohne Verweise auf Features, die es nicht gibt oder die sich nicht ausblenden lassen
    /// (gelöschte Skizzen, von Hand geänderte Dateien).
    pub(crate) fn pruned_visibility(&self, mut v: Visibility) -> Visibility {
        v.bodies
            .retain(|k| self.feature(FeatureId(k.feature)).is_some());
        v.features.retain(|id| {
            self.feature(*id)
                .is_some_and(|f| matches!(f.kind, FeatureKind::Sketch(_) | FeatureKind::Plane(_)))
        });
        v
    }
}
