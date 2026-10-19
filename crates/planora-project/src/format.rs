//! Die Typen der Versionsdatei (Format 1 bis 6). Unabhängig von den internen Strukturen.
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
    /// Zeichnungen (ab Format 4, Teilprojekt 11). Fehlt das Feld oder ist die Liste leer,
    /// bleibt die Datei Byte für Byte wie in Format 1 bis 3. Schon beim Lesen begrenzt, damit
    /// eine feindliche Datei keine riesige Liste anlegen lässt.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "de_drawings"
    )]
    pub zeichnungen: Vec<ZeichnungDto>,
    /// Nächste Zeichnungskennung; 0 oder fehlend heißt „aus den Kennungen ableiten“.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub naechste_zeichnung: u64,
    /// Projektangaben für das Schriftfeld (ab Format 6). Fehlt das Feld, bleibt die Datei Byte
    /// für Byte wie in Format 1 bis 5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projektangaben: Option<ProjektangabenDto>,
    /// Eigene Texte der Stücklistenzeilen (ab Format 7). Leer heißt: fehlt, damit alte Dateien
    /// Byte für Byte bleiben. Schon beim Lesen begrenzt.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "de_overrides"
    )]
    pub stueckliste: Vec<StuecklisteZeileDto>,
}

/// Eigene Texte einer Stücklistenzeile (ab Format 7); `koerper` ist `[Feature, Ergebnis-Index]`
/// des ersten Körpers der Zeile, ein fehlender Text heißt automatisch.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct StuecklisteZeileDto {
    pub koerper: [u64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub benennung: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menge: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub werkstoff: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bemerkung: Option<String>,
}

/// Ein Nummernkreis (ab Format 7): Ansicht, Körper, Anker (relativ zur Hüllbox der Ansicht) und
/// Versatz zur Kreismitte (Blatt-mm).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PositionDto {
    pub id: u64,
    pub ansicht: u64,
    pub koerper: [u64; 2],
    pub anker: [f64; 2],
    pub versatz: [f64; 2],
}

/// Höchstzahl der Hinweiszeilen beim Lesen; eine mehr, damit die Prüfung danach mit dem Namen
/// der Zeichnung melden kann.
const MAX_FILE_NOTES: usize = planora_drawing::sheet_info::MAX_NOTES + 1;
/// Höchstzahl der Änderungen beim Lesen (eine mehr, aus demselben Grund).
const MAX_FILE_REVISIONS: usize = planora_drawing::sheet_info::MAX_REVISIONS + 1;

/// Wie `de_bounded`, nimmt aber nur die ersten `max` Einträge an und überliest den Rest, ohne ihn
/// zu speichern. Die Prüfung danach meldet die zu lange Liste mit dem richtigen Höchstwert und
/// dem Namen der Zeichnung.
fn de_capped<'de, D, T>(d: D, max: usize) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Capped<T> {
        max: usize,
        marker: std::marker::PhantomData<T>,
    }
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Capped<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "eine Liste")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(self.max));
            while out.len() < self.max {
                match seq.next_element()? {
                    Some(x) => out.push(x),
                    None => return Ok(out),
                }
            }
            while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
            Ok(out)
        }
    }
    d.deserialize_seq(Capped {
        max,
        marker: std::marker::PhantomData,
    })
}

fn de_notes<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Vec<String>>, D::Error> {
    struct Lines(Vec<String>);
    impl<'de> Deserialize<'de> for Lines {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            de_capped(d, MAX_FILE_NOTES).map(Lines)
        }
    }
    Ok(Option::<Lines>::deserialize(d)?.map(|l| l.0))
}

/// Höchstzahl der Nummernkreise beim Lesen (eine mehr, aus demselben Grund).
const MAX_FILE_POSITIONS: usize = planora_drawing::parts::MAX_BALLOONS + 1;
/// Höchstzahl der Zeilentexte beim Lesen; mehr bricht das Lesen ab.
const MAX_FILE_OVERRIDES: usize = planora_drawing::parts::MAX_OVERRIDES;

fn de_positions<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<PositionDto>, D::Error> {
    de_capped(d, MAX_FILE_POSITIONS)
}

fn de_overrides<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<StuecklisteZeileDto>, D::Error> {
    de_bounded(d, MAX_FILE_OVERRIDES, "Stückliste")
}

fn de_revisions<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<AenderungDto>, D::Error> {
    de_capped(d, MAX_FILE_REVISIONS)
}

/// Projektangaben (ab Format 6). `None` bei einem Eintrag heißt automatisch.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ProjektangabenDto {
    #[serde(default)]
    pub anmeldename: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eigentuemer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ersteller: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geprueft: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub freigabe: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub werkstoff: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allgemeintoleranz: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_notes"
    )]
    pub hinweise: Option<Vec<String>>,
}

/// Blattangaben einer Zeichnung (ab Format 6). `None` bei einem Eintrag heißt automatisch.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BlattangabenDto {
    #[serde(default)]
    pub angelegt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nummer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub titel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zusatztitel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dokumentart: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dokumentstatus: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ausgabedatum: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aenderungsstand: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blatt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub massstab: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gewicht: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_notes"
    )]
    pub hinweise: Option<Vec<String>>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "de_revisions"
    )]
    pub aenderungen: Vec<AenderungDto>,
    /// Stückliste auf dem Zeichnungsblatt (ab Format 7); aus heißt: fehlt.
    #[serde(default, skip_serializing_if = "is_false")]
    pub stueckliste: bool,
}

/// Eine Zeile der Änderungstabelle; fehlender Buchstabe heißt automatisch.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AenderungDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buchstabe: Option<String>,
    #[serde(default)]
    pub beschreibung: String,
    #[serde(default)]
    pub datum: String,
    #[serde(default)]
    pub name: String,
}

fn is_false(x: &bool) -> bool {
    !*x
}

fn is_zero(x: &u64) -> bool {
    *x == 0
}

/// Höchstzahl der Zeichnungen, die beim Lesen überhaupt angenommen wird.
pub const MAX_FILE_DRAWINGS: usize = planora_feature::MAX_DRAWINGS;
/// Höchstzahl der Ansichten je Zeichnung, die beim Lesen überhaupt angenommen wird.
pub const MAX_FILE_VIEWS: usize = planora_feature::MAX_VIEWS;

/// Liest eine Liste, bricht aber ab, sobald sie mehr als `max` Einträge hat (vor dem Anlegen
/// weiterer Einträge; der Rest der Datei wird nicht mehr gelesen).
fn de_bounded<'de, D, T>(d: D, max: usize, what: &'static str) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Bounded<T> {
        max: usize,
        what: &'static str,
        marker: std::marker::PhantomData<T>,
    }
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Bounded<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            write!(f, "eine Liste mit höchstens {} Einträgen", self.max)
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(self.max));
            while let Some(x) = seq.next_element()? {
                if out.len() == self.max {
                    return Err(serde::de::Error::custom(format!(
                        "{}: zu viele Einträge (höchstens {})",
                        self.what, self.max
                    )));
                }
                out.push(x);
            }
            Ok(out)
        }
    }
    d.deserialize_seq(Bounded {
        max,
        what,
        marker: std::marker::PhantomData,
    })
}

fn de_drawings<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<ZeichnungDto>, D::Error> {
    de_bounded(d, MAX_FILE_DRAWINGS, "Zeichnungen")
}

fn de_views<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<AnsichtDto>, D::Error> {
    de_bounded(d, MAX_FILE_VIEWS, "Ansichten")
}

/// Höchstzahl der Maße je Zeichnung, die beim Lesen überhaupt angenommen wird.
pub const MAX_FILE_DIMENSIONS: usize = planora_drawing::limits::MAX_DIMENSIONS;
/// Höchstzahl der Bezüge je Maß, die beim Lesen überhaupt angenommen wird.
pub const MAX_FILE_DIM_REFS: usize = 4;

fn de_dims<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<MassDto>, D::Error> {
    de_bounded(d, MAX_FILE_DIMENSIONS, "Maße")
}

fn de_refs<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<BezugDto>, D::Error> {
    de_bounded(d, MAX_FILE_DIM_REFS, "Bezüge")
}

/// Eine Zeichnung (ab Format 4).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ZeichnungDto {
    pub id: u64,
    pub name: String,
    pub blatt: BlattDto,
    /// `"erst"` (Erstwinkel) oder `"dritt"` (Dritter Winkel).
    pub projektion: String,
    /// Nächste Ansichtskennung; 0 oder fehlend heißt „aus den Kennungen ableiten“.
    #[serde(default)]
    pub naechste_ansicht: u64,
    #[serde(deserialize_with = "de_views")]
    pub ansichten: Vec<AnsichtDto>,
    /// Maße (ab Format 5). Fehlt das Feld oder ist die Liste leer, bleibt die Zeichnung Byte
    /// für Byte wie in Format 4. Schon beim Lesen begrenzt.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "de_dims"
    )]
    pub masse: Vec<MassDto>,
    /// Nächste Maßkennung (ab Format 5); 0 oder fehlend heißt „aus den Kennungen ableiten“.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub naechstes_mass: u64,
    /// Blattangaben (ab Format 6). Fehlt das Feld, bleibt die Zeichnung Byte für Byte wie in
    /// Format 4 und 5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blattangaben: Option<BlattangabenDto>,
    /// Seitennummer eines Stücklistenblatts (ab Format 7); fehlt bei Ansichtenzeichnungen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stuecklistenseite: Option<u32>,
    /// Nummernkreise (ab Format 7). Leer heißt: fehlt. Beim Lesen auf eins über dem Höchstwert
    /// begrenzt, die Prüfung danach meldet mit dem Namen der Zeichnung.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "de_positions"
    )]
    pub positionen: Vec<PositionDto>,
    /// Nächste Kennung eines Nummernkreises (ab Format 7); 0 oder fehlend heißt „aus den
    /// Kennungen ableiten“.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub naechste_position: u64,
}

/// Blatt: Papier `"A4"` bis `"A0"`, quer oder hoch.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BlattDto {
    pub papier: String,
    pub quer: bool,
}

/// Eine Ansicht. `art`: `"vorne"`, `"hinten"`, `"links"`, `"rechts"`, `"oben"`, `"unten"`,
/// `"iso"`; `massstab`: `[Zähler, Nenner]`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AnsichtDto {
    pub id: u64,
    pub art: String,
    pub massstab: [u32; 2],
    pub lage: LageDto,
    pub verdeckt_zeigen: bool,
    pub eltern: Option<u64>,
    /// Mittellinien zeigen (ab Format 5). Fehlt das Feld, gilt „aus“; in Format 1 bis 4 wird
    /// es nicht geschrieben (alte Dateien Byte für Byte).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mittellinien: Option<bool>,
    /// Schnittansicht (ab Format 8); fehlt bei allen anderen Ansichten.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schnitt: Option<SchnittDto>,
}

/// Angaben einer Schnittansicht (ab Format 8): Lage der Ebene in mm (Weltkoordinate),
/// Buchstabe (genau ein Zeichen), Schraffur an/aus und ihr Abstand in A3-Millimetern.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SchnittDto {
    pub versatz: f64,
    pub buchstabe: String,
    pub schraffur: bool,
    pub schraffurabstand: f64,
}

/// Ein Maß (ab Format 5). `art`: `"waagerecht"`, `"senkrecht"`, `"schraeg"`,
/// `"durchmesser"`, `"radius"`, `"winkel"`; `abstand` und `textversatz` in Blatt-mm, `winkel`
/// (Führungslinie bei Ø/R) in Grad; `stellen`: Nachkommastellen; `text`: eigener Text statt
/// des Werts.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MassDto {
    pub id: u64,
    pub ansicht: u64,
    pub art: String,
    #[serde(deserialize_with = "de_refs")]
    pub bezuege: Vec<BezugDto>,
    pub abstand: f64,
    pub winkel: f64,
    pub textversatz: [f64; 2],
    pub toleranz: ToleranzDto,
    pub stellen: u8,
    pub text: Option<String>,
    pub kette: Option<KetteDto>,
    pub vorgeschlagen: bool,
}

/// Bezug eines Maßes: Kante (zwei Flächenmarken `a`, `b` und Hinweispunkt), Kantenende
/// (dazu `ende`: `"anfang"` oder `"ende"`), Mitte eines Kreises oder Bogens (Kante) oder
/// Fläche (eine Marke).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum BezugDto {
    Kante {
        a: FaceRefDto,
        b: FaceRefDto,
        hinweis: V3,
    },
    Kantenende {
        a: FaceRefDto,
        b: FaceRefDto,
        hinweis: V3,
        ende: String,
    },
    Mitte {
        a: FaceRefDto,
        b: FaceRefDto,
        hinweis: V3,
    },
    Flaeche {
        marke: FaceRefDto,
    },
}

/// Toleranz eines Maßes in mm. `abmasse` und `grenzmasse` tragen beide die **Abweichungen**
/// vom Nennmaß (`oben`, `unten`); `grenzmasse` zeigt daraus Höchst- und Mindestmaß.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum ToleranzDto {
    Keine,
    Symmetrisch { wert: f64 },
    Abmasse { oben: f64, unten: f64 },
    Grenzmasse { oben: f64, unten: f64 },
    Passung { text: String },
}

/// Zugehörigkeit zu einer Maßkette: Gruppe, `modus` `"kette"` (gemeinsame Maßlinie) oder
/// `"bezug"` (gestapelte Maßlinien) und Stelle in der Gruppe.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct KetteDto {
    pub gruppe: u64,
    pub modus: String,
    pub reihenfolge: u32,
}

/// Lage einer Ansicht in mm (eigenes Objekt mit Schlüssel `typ` wie die übrigen Arten).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ")]
pub enum LageDto {
    #[serde(rename = "frei")]
    Frei { x: f64, y: f64 },
    #[serde(rename = "ausgerichtet")]
    Ausgerichtet { abstand: f64 },
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
