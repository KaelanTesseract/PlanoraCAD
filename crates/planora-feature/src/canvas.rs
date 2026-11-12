//! Leinwand (Skizzen 9, Spec D): ein Bild als Zeichenhilfe auf einer Ebene oder ebenen Fläche.
//! Die Leinwand ist ein eigenes Feature im Verlauf, folgt ihrem Träger und erzeugt keinen
//! Körper. Die Bilddaten liegen einmal je Dokument in einer Tabelle; Leinwände und
//! Rückgängig-Bilder halten nur Verweise (`Arc`), die Bytes werden geteilt.

use crate::model::{Document, FeatureId, FeatureKind, FeatureStatus, StandardPlane};
use crate::refs::FaceRef;
use planora_exchange::ImageFormat;
use planora_math::{DVec2, DVec3, Frame};
use std::collections::BTreeSet;
use std::sync::Arc;

/// Kleinster und größter Maßstab einer Leinwand je Achse (wie bei Netz-Körpern).
pub const MIN_CANVAS_SCALE: f64 = 1e-6;
pub const MAX_CANVAS_SCALE: f64 = 1e6;
/// Größte Deckkraft in Prozent.
pub const MAX_CANVAS_OPACITY: u8 = 100;

/// Kennung eines Bildes in der Bildtabelle des Dokuments (ab 1); wird nie wiederverwendet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageId(pub u64);

/// Eingebettetes Bild: die Bytes der Datei (unverändert, so wie sie gespeichert werden), Format,
/// Maße in Pixeln und Prüfsumme der Bytes (vom Aufrufer gerechnet).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanvasImage {
    pub bytes: Arc<[u8]>,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub hash: u64,
}

/// Prüfsumme der Bytes eines Bildes (FNV-1a, 64 Bit) für [`CanvasImage::hash`]. Fest
/// festgelegt, damit Einfügen und Laden dieselbe Zahl ergeben; nur zum schnellen Vergleich,
/// gleiche Bilder erkennt [`Document::add_image`] erst an gleichen Bytes.
pub fn image_hash(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes
        .iter()
        .fold(OFFSET, |h, b| (h ^ u64::from(*b)).wrapping_mul(PRIME))
}

/// Träger einer Leinwand: Grundebene, ebene Fläche eines früheren Features oder eine frühere
/// Konstruktionsebene (wie bei Skizzen).
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasBase {
    Standard(StandardPlane),
    Face(FaceRef),
    Plane(FeatureId),
}

/// Leinwand-Feature. Lage im Träger: Erst spiegeln, dann skalieren (1 px = 1 mm vor dem
/// Maßstab), dann um `angle` (Bogenmaß, gegen den Uhrzeigersinn um die Normale) drehen und die
/// Bildmitte auf `offset` (mm in der Trägerebene) schieben.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasFeature {
    pub image: ImageId,
    pub base: CanvasBase,
    pub offset: DVec2,
    pub angle: f64,
    pub scale: DVec2,
    pub flip_h: bool,
    pub flip_v: bool,
    /// Deckkraft in Prozent, 0..=100.
    pub opacity: u8,
    /// Durch Objekte anzeigen (ohne Tiefentest nach den Körpern zeichnen).
    pub display_through: bool,
    /// Auswählbar in der Ansicht.
    pub selectable: bool,
    /// Zuletzt aufgelöstes Koordinatensystem des Trägers (ohne Versatz, Winkel und Maßstab).
    /// Wird nach jeder Berechnung ohne Versionsänderung zurückgeschrieben (wie der Hinweispunkt
    /// der Fläche), steht deshalb im Snapshot und in der Datei. Geht die Trägerfläche verloren,
    /// bleibt die Leinwand hier, auch nach Rückgängig oder Laden.
    pub last_base: Option<Frame>,
}

impl CanvasFeature {
    /// Leinwand mit den Vorgaben wie in Fusion: Deckkraft 50, Durch Objekte anzeigen an,
    /// Auswählbar an, 1 px = 1 mm, Bildmitte im Ursprung des Trägers.
    pub fn new(image: ImageId, base: CanvasBase) -> CanvasFeature {
        CanvasFeature {
            image,
            base,
            offset: DVec2::ZERO,
            angle: 0.0,
            scale: DVec2::ONE,
            flip_h: false,
            flip_v: false,
            opacity: 50,
            display_through: true,
            selectable: true,
            last_base: None,
        }
    }

    /// Features, auf die der Träger verweist.
    pub fn dependencies(&self) -> Vec<FeatureId> {
        match &self.base {
            CanvasBase::Standard(_) => Vec::new(),
            CanvasBase::Face(r) => vec![FeatureId(r.origin)],
            CanvasBase::Plane(id) => vec![*id],
        }
    }

    /// Lage der Leinwand aus dem Koordinatensystem `base` des Trägers: `origin` ist die
    /// Bildmitte, `x` und `y` sind die Schritte eines Pixels nach rechts und nach oben im Bild
    /// (samt Maßstab und Spiegelung, darum nicht normiert), `z` die Normale des Trägers.
    pub fn placed_frame(&self, base: &Frame) -> Frame {
        let (s, c) = self.angle.sin_cos();
        let sx = if self.flip_h {
            -self.scale.x
        } else {
            self.scale.x
        };
        let sy = if self.flip_v {
            -self.scale.y
        } else {
            self.scale.y
        };
        Frame {
            origin: base.origin + base.x * self.offset.x + base.y * self.offset.y,
            x: (base.x * c + base.y * s) * sx,
            y: (base.y * c - base.x * s) * sy,
            z: base.z,
        }
    }

    /// Die vier Ecken des Bildes in Weltkoordinaten aus der Lage `frame` (Ergebnis von
    /// [`Document::canvas_frame`]): oben links, oben rechts, unten rechts, unten links, so wie das
    /// Bild ungespiegelt erscheint (Texturkoordinaten (0,0), (1,0), (1,1), (0,1)).
    pub fn corners(&self, frame: &Frame, img: &CanvasImage) -> [DVec3; 4] {
        let hx = frame.x * (f64::from(img.width) / 2.0);
        let hy = frame.y * (f64::from(img.height) / 2.0);
        let o = frame.origin;
        [o - hx + hy, o + hx + hy, o + hx - hy, o - hx - hy]
    }
}

impl Document {
    /// Legt ein Bild in die Tabelle (ohne Feature; zählt nicht als Änderung). Gibt es schon ein
    /// Bild mit gleicher Prüfsumme und gleichen Bytes, kommt dessen Kennung zurück. Nicht
    /// benutzte Bilder werden nicht gespeichert und fallen beim nächsten vollen Wiederherstellen
    /// weg.
    pub fn add_image(&mut self, img: CanvasImage) -> ImageId {
        if let Some((id, _)) = self
            .images
            .iter()
            .find(|(_, known)| known.hash == img.hash && known.bytes == img.bytes)
        {
            return *id;
        }
        let id = ImageId(self.next_image);
        // Mehr als u64::MAX Bilder gibt es nicht; ein Überlauf wäre ein kaputter Zähler.
        self.next_image = self.next_image.saturating_add(1);
        self.images.insert(id, Arc::new(img));
        id
    }

    pub fn image(&self, id: ImageId) -> Option<&Arc<CanvasImage>> {
        self.images.get(&id)
    }

    /// Berechnete Lage der Leinwand `id` (siehe [`CanvasFeature::placed_frame`]); `None`, wenn
    /// das Feature keine berechnete Leinwand ist (unterdrückt, Fehler, zurückgestellt). Bei
    /// verlorener Fläche ist es die letzte Lage.
    ///
    /// **Achtung: Das Ergebnis ist kein orthonormales System.** `x` und `y` sind die Schritte
    /// eines Bildpixels samt Maßstab und Spiegelung: Sie sind nicht normiert, und mit genau
    /// einer Spiegelung ist das System linkshändig (`x × y = −z`). `origin` ist die Bildmitte,
    /// `z` die Normale des Trägers. Für Ecken [`CanvasFeature::corners`] benutzen; wer eine
    /// echte Ebene braucht, nimmt `z` und `origin`.
    pub fn canvas_frame(&self, id: FeatureId) -> Option<Frame> {
        let f = self.feature(id)?;
        let FeatureKind::Canvas(c) = &f.kind else {
            return None;
        };
        if f.status != FeatureStatus::Ok {
            return None;
        }
        let base = f.result.as_ref()?.canvas?;
        Some(c.placed_frame(&base))
    }

    /// Entfernt Bilder, auf die keine Leinwand mehr verweist (wie bei Netzen).
    pub(crate) fn prune_images(&mut self) {
        let used = self.used_images();
        self.images.retain(|id, _| used.contains(id));
    }

    /// Bilder, auf die eine Leinwand verweist.
    pub(crate) fn used_images(&self) -> BTreeSet<ImageId> {
        self.features
            .iter()
            .filter_map(|f| match &f.kind {
                FeatureKind::Canvas(c) => Some(c.image),
                _ => None,
            })
            .collect()
    }
}

/// Fehler „Bild fehlt im Dokument“.
pub(crate) fn image_missing(id: ImageId) -> crate::error::FeatureError {
    crate::error::FeatureError::InvalidInput(planora_i18n::tr!(
        "feature-canvas-image-missing",
        id = id.0.to_string()
    ))
}
