//! Zwischenspeicher der fertigen Skizzen (Skizzen 12, Task 12b): je Skizzen-Feature die
//! abgetasteten Kurven in Weltkoordinaten und die teuren Mengen, die nicht von Kamera, Zeiger
//! oder Auswahl abhängen (Anzeigeschalter, unsichtbare Griffe, Glyphenpunkte, Kontrollpunkt-
//! rahmen). Vorher tastete jedes Bild jede Kurve neu ab: 50 Skizzen mit je 5 Texten kosteten
//! so rund 109 von 126 ms je Bild.
//!
//! Schlüssel ist der Stand der berechneten Skizze ([`Sketch::revision`], über alle Skizzen
//! eindeutig, jede Änderung gibt einen neuen), dazu ihre Ebene, ihre Toleranz und die
//! Anzeigeschalter samt Projektionen ([`DisplayFilter`]). Bearbeiten, Rückgängig/Wiederholen,
//! Parameter und Rückrollmarke ändern die berechnete Skizze und damit den Stand; ein anderer
//! Schalter ändert den Filter. Sichtbarkeit, Auswahl, Hover und Kamera bleiben außen vor und
//! gelten je Bild. Jeder Tab hat seinen eigenen [`AppState`](crate::state::AppState) und damit
//! seinen eigenen Speicher; nach einem Dokumentwechsel im selben Tab passt der Stand nicht mehr.

use crate::sketch_handles::HiddenHandles;
use crate::sketch_view::{DisplayFilter, DisplayHidden};
use planora_feature::FeatureId;
use planora_math::{DVec3, Frame, Tolerance};
use planora_sketch::model::{CurveId, PointId, Sketch};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Was je Stand einer fertigen Skizze gleich bleibt.
#[derive(Debug, Default)]
pub struct FinishedSketch {
    /// Was die Anzeigeschalter ausblenden.
    pub hidden: DisplayHidden,
    /// Griffe ohne Abhängigkeit (eine fertige Skizze hat keine Auswahl, B6).
    pub handles: HiddenHandles,
    /// Punkte der Glyphen von Texten (nicht anklickbar).
    pub glyphs: HashSet<PointId>,
    /// Die sichtbaren Kurven (weder von Schaltern noch als Griff ausgeblendet) in der
    /// Reihenfolge der Skizze, je mit ihrem Linienzug in Weltkoordinaten.
    pub curves: Vec<(CurveId, Vec<DVec3>)>,
    /// Kontrollpunktrahmen (Skizzen 9) in Weltkoordinaten.
    pub frames: Vec<Vec<DVec3>>,
}

impl FinishedSketch {
    fn of(sk: &Sketch, filter: &DisplayFilter) -> FinishedSketch {
        let hidden = filter.hidden(sk);
        let handles = crate::sketch_handles::hidden_handles_of(sk, &[]);
        let curves = sk
            .curves()
            .map(|(c, _)| c)
            .filter(|c| !hidden.curves.contains(c) && !handles.curves.contains(c))
            .filter_map(|c| Some((c, crate::sketch_view::curve_world_points(sk, c)?)))
            .collect();
        let frame = sk.frame();
        let frames = crate::sketch_view::frame_polygons3(sk)
            .into_iter()
            .map(|line| line.iter().map(|p| frame.point(p.x, p.y, p.z)).collect())
            .collect();
        FinishedSketch {
            glyphs: sk.glyph_points(),
            hidden,
            handles,
            curves,
            frames,
        }
    }
}

/// Wozu ein Eintrag gilt.
#[derive(Debug, PartialEq)]
struct Key {
    revision: u64,
    frame: Frame,
    tolerance: Tolerance,
    filter: DisplayFilter,
}

/// Der Zwischenspeicher; liegt im [`AppState`](crate::state::AppState), nicht im Dokument.
#[derive(Debug, Default)]
pub struct FinishedSketchCache {
    entries: RefCell<HashMap<FeatureId, (Key, Arc<FinishedSketch>)>>,
    computed: Cell<usize>,
}

impl FinishedSketchCache {
    /// Die Daten der fertigen Skizze `sk` des Features `id` mit den Schaltern `filter`, aus dem
    /// Speicher, wenn Stand, Ebene, Toleranz und Schalter gleich geblieben sind.
    pub fn get(&self, id: FeatureId, sk: &Sketch, filter: DisplayFilter) -> Arc<FinishedSketch> {
        let key = Key {
            revision: sk.revision(),
            frame: *sk.frame(),
            tolerance: *sk.tolerance(),
            filter,
        };
        if let Some((k, data)) = self.entries.borrow().get(&id)
            && *k == key
        {
            return Arc::clone(data);
        }
        let data = Arc::new(FinishedSketch::of(sk, &key.filter));
        self.computed.set(self.computed.get() + 1);
        self.entries
            .borrow_mut()
            .insert(id, (key, Arc::clone(&data)));
        data
    }

    /// Behält nur die Einträge der Features, für die `keep` gilt (die gerade gezeigten); so
    /// bleibt nichts von gelöschten, ausgeblendeten oder geöffneten Skizzen liegen.
    pub fn retain(&self, keep: impl Fn(FeatureId) -> bool) {
        self.entries.borrow_mut().retain(|id, _| keep(*id));
    }

    /// Wie oft bisher tatsächlich abgetastet wurde (für Tests).
    pub fn computations(&self) -> usize {
        self.computed.get()
    }
}
