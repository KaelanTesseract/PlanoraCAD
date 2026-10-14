//! Die Typen der Versionsdatei (Format 1). Unabhängig von den internen Strukturen.
//! Absichtlich ohne `deny_unknown_fields`: unbekannte Felder in bekannten Versionen werden
//! ignoriert.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct VersionFile {
    pub format: u32,
    pub marker: usize,
    pub naechste_id: u64,
    pub features: Vec<FeatureDto>,
    /// Netze der Netz-Körper (ab Format 3, Plan P12). Fehlt das Feld oder ist die Liste leer,
    /// bleibt die Datei Byte für Byte wie in Format 1 und 2.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub netze: Vec<MeshDto>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FeatureDto {
    pub id: u64,
    pub name: String,
    pub unterdrueckt: bool,
    pub art: KindDto,
}

pub type V3 = [f64; 3];

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BodyRefDto {
    pub feature: u64,
    pub index: usize,
}

/// Ein Netz: Kennung und Base64 der Bytes aus `ExchangeMesh::to_bytes`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MeshDto {
    pub id: u64,
    pub daten: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FaceRefDto {
    pub herkunft: u64,
    pub rolle: String,
    pub hinweis: V3,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EdgeRefDto {
    pub a: FaceRefDto,
    pub b: FaceRefDto,
    pub hinweis: V3,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum PlaneDto {
    Standard { ebene: String },
    Flaeche { flaeche: FaceRefDto },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EdgeCopyDto {
    pub kurve: usize,
    pub kante: EdgeRefDto,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FrameDto {
    pub ursprung: V3,
    pub x: V3,
    pub y: V3,
    pub z: V3,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PointDto {
    pub x: usize,
    pub y: usize,
    pub konstruktion: bool,
    pub aktiv: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum CurveKindDto {
    Linie {
        a: usize,
        b: usize,
    },
    Kreis {
        mitte: usize,
        radius: usize,
    },
    Bogen {
        mitte: usize,
        start: usize,
        ende: usize,
    },
    Ellipse {
        mitte: usize,
        ax: usize,
        ay: usize,
        klein: usize,
    },
    Spline {
        punkte: Vec<usize>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CurveDto {
    pub art: CurveKindDto,
    pub konstruktion: bool,
    pub aktiv: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum LineOrPointsDto {
    Linie { kurve: usize },
    Punkte { a: usize, b: usize },
}

/// Abhängigkeiten; Punkte und Kurven als Indizes, Maße als Zahlen.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum ConstraintKindDto {
    Deckungsgleich {
        a: usize,
        b: usize,
    },
    PunktAufKurve {
        punkt: usize,
        kurve: usize,
    },
    Waagerecht {
        ziel: LineOrPointsDto,
    },
    Senkrecht {
        ziel: LineOrPointsDto,
    },
    Parallel {
        a: usize,
        b: usize,
    },
    Rechtwinklig {
        a: usize,
        b: usize,
    },
    Tangential {
        a: usize,
        b: usize,
    },
    Gleich {
        a: usize,
        b: usize,
    },
    Fest {
        punkt: usize,
        x: f64,
        y: f64,
    },
    Symmetrisch {
        a: usize,
        b: usize,
        achse: usize,
    },
    Mittelpunkt {
        punkt: usize,
        kurve: usize,
    },
    Abstand {
        a: usize,
        b: usize,
        wert: f64,
    },
    AbstandPunktLinie {
        punkt: usize,
        kurve: usize,
        wert: f64,
    },
    Radius {
        kurve: usize,
        wert: f64,
    },
    Durchmesser {
        kurve: usize,
        wert: f64,
    },
    Winkel {
        a: usize,
        b: usize,
        wert: f64,
    },
    BogenRadius {
        kurve: usize,
    },
    EllipseGross {
        kurve: usize,
        wert: f64,
    },
    EllipseKlein {
        kurve: usize,
        wert: f64,
    },
    EllipseWinkel {
        kurve: usize,
        wert: f64,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ConstraintDto {
    pub art: ConstraintKindDto,
    pub zweig: i8,
    pub gesteuert: bool,
    pub aktiv: bool,
    pub intern: bool,
    pub extra: Option<usize>,
    pub lebt: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SketchDto {
    pub name: String,
    pub ebene: FrameDto,
    pub linear: f64,
    pub winkel: f64,
    pub werte: Vec<f64>,
    pub punkte: Vec<PointDto>,
    pub kurven: Vec<CurveDto>,
    pub abhaengigkeiten: Vec<ConstraintDto>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SketchFeatureDto {
    pub skizze: SketchDto,
    pub lage: PlaneDto,
    pub kopien: Vec<EdgeCopyDto>,
}

/// Art einer Verrundung (Grad, Prozent und Millimeter wie im Feature).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum BlendArtDto {
    Rundung { radius: f64 },
    RundungVariabel { punkte: Vec<[f64; 2]> },
    FaseGleich { abstand: f64 },
    FaseZwei { abstand1: f64, abstand2: f64 },
    FaseWinkel { abstand: f64, winkel: f64 },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum KindDto {
    Skizze(Box<SketchFeatureDto>),
    Quader {
        groesse: V3,
        ursprung: V3,
    },
    Zylinder {
        radius: f64,
        hoehe: f64,
        ursprung: V3,
    },
    Kugel {
        radius: f64,
        ursprung: V3,
    },
    Kegel {
        radius: f64,
        hoehe: f64,
        ursprung: V3,
    },
    Extrusion {
        skizze: u64,
        profil: usize,
        hoehe: f64,
    },
    Rotation {
        skizze: u64,
        profil: usize,
        achse: usize,
    },
    Boolesch {
        ziel: BodyRefDto,
        werkzeug: BodyRefDto,
        operation: String,
    },
    Verschieben {
        koerper: BodyRefDto,
        versatz: V3,
    },
    Verrunden {
        koerper: BodyRefDto,
        kanten: Vec<EdgeRefDto>,
        art: BlendArtDto,
        tangentenkette: bool,
    },
    /// Netz-Körper (ab Format 3): Netz aus `netze`, Lage in mm, Drehung als Quaternion
    /// [x, y, z, w], Maßstab.
    NetzKoerper {
        netz: u64,
        versatz: V3,
        drehung: [f64; 4],
        massstab: f64,
    },
}
