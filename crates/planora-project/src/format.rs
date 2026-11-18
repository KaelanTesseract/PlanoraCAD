//! Die Typen der Versionsdatei (Format 1 bis 11). Unabhängig von den internen Strukturen.
//! Absichtlich ohne `deny_unknown_fields`: unbekannte Felder in bekannten Versionen werden
//! ignoriert (unbekannte **Feldschlüssel** in einer Formeltabelle sind dagegen Daten und werden
//! beim Lesen abgelehnt, siehe `convert.rs`).

use planora_i18n::tr;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    /// Parametertabelle (ab Format 9). Leer heißt: fehlt, damit alte Dateien Byte für Byte
    /// bleiben. Beim Lesen auf eins über dem Höchstwert begrenzt, die Prüfung danach meldet die
    /// zu lange Tabelle im Klartext.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "de_parameters"
    )]
    pub parameter: Vec<ParameterDto>,
    /// Nächste Parameterkennung (ab Format 9); nur mit Parametern geschrieben, 0 oder fehlend
    /// heißt „aus den Kennungen ableiten“.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub naechster_parameter: u64,
    /// Rückrollstellen je Körper (ab Format 11, Verlaufsleiste): Kennungen der zurückgesetzten
    /// Schritte in Listenordnung. Leer heißt: fehlt, damit alte Dateien Byte für Byte bleiben.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rueckrollstellen: Vec<u64>,
    /// Was im Browser ausgeblendet ist (ab Format 16). Fehlt im Grundzustand, damit alte Dateien
    /// Byte für Byte bleiben.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sichtbarkeit: Option<SichtbarkeitDto>,
    /// Skizzengruppen im Browser (ab Format 22, Skizzen 11b) in Browser-Reihenfolge. Leer heißt:
    /// fehlt, damit alte Dateien Byte für Byte bleiben. Schon beim Lesen begrenzt.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "de_sketch_groups"
    )]
    pub skizzengruppen: Vec<SkizzengruppeDto>,
    /// Nächste Kennung einer Skizzengruppe (ab Format 22); 0 oder fehlend heißt „aus den
    /// Kennungen ableiten“ (größte + 1). Fehlt ohne Gruppen, damit alte Dateien Byte für Byte
    /// bleiben.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub naechste_gruppe: u64,
    /// Anzeigeschalter je Skizze (ab Format 23, Skizzen 12), aufsteigend nach Skizze; nur
    /// Skizzen, bei denen ein Schalter von der Vorgabe abweicht. Leer heißt: fehlt, damit alte
    /// Dateien Byte für Byte bleiben. Schon beim Lesen begrenzt.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "de_sketch_display"
    )]
    pub skizzenanzeige: Vec<SkizzenanzeigeDto>,
    /// Bilder der Leinwände (ab Format 19, Skizzen 9): Kennung, Format und Base64 der Datei-Bytes,
    /// in jeder Version erneut (wie `netze`). Leer heißt: fehlt, damit alte Dateien Byte für Byte
    /// bleiben. Steht hinten, weil die Daten lang sind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bilder: Vec<ImageDto>,
    /// Formen der STEP-Körper (ab Format 10, Teilprojekt STEP-Import): je benutzte Form der
    /// normalisierte STEP-Text, in jeder Version erneut. Leer heißt: fehlt, damit alte Dateien
    /// Byte für Byte bleiben. Steht am Ende, weil die Texte lang sind.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub step_formen: Vec<StepFormDto>,
}

/// Eine Skizzengruppe (ab Format 22): Kennung, Name, übergeordnete Gruppe (fehlt oben),
/// ausgeblendet (`aus`, fehlt, wenn sichtbar) und die Feature-Kennungen der Skizzen darin.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SkizzengruppeDto {
    pub kennung: u64,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eltern: Option<u64>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub aus: bool,
    #[serde(default)]
    pub skizzen: Vec<u64>,
}

/// Höchstzahl der Skizzengruppen, die beim Lesen überhaupt angenommen wird.
pub const MAX_FILE_SKETCH_GROUPS: usize = 10_000;

fn de_sketch_groups<'de, D>(d: D) -> Result<Vec<SkizzengruppeDto>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    de_bounded(d, MAX_FILE_SKETCH_GROUPS, || tr!("proj-l-sketch-groups"))
}

/// Höchstzahl der Skizzen-Anzeigeeinträge, die beim Lesen überhaupt angenommen wird.
pub const MAX_FILE_SKETCH_DISPLAY: usize = 100_000;

fn de_sketch_display<'de, D>(d: D) -> Result<Vec<SkizzenanzeigeDto>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    de_bounded(d, MAX_FILE_SKETCH_DISPLAY, || tr!("proj-l-sketch-display"))
}

/// Anzeigeschalter einer Skizze (ab Format 23). Jedes Feld steht nur, wenn es von der Vorgabe
/// abweicht (Vorgabe: `slice` aus, `masse_fertig` aus, alle anderen an).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SkizzenanzeigeDto {
    /// Feature-Kennung der Skizze.
    pub skizze: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slice: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profil: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub punkte: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub masse: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub abhaengigkeiten: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub konstruktion: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projiziert: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub masse_fertig: Option<bool>,
}

/// Höhenformel eines Textes (ab Format 22): `text` ist der Index in `texte` (gleich
/// `TextId::index()`, auch gelöschte Texte zählen).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TextFormulaDto {
    pub text: usize,
    pub formel: String,
}

/// Sichtbarkeit im Browser (ab Format 16); jede Liste fehlt, wenn sie leer ist.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SichtbarkeitDto {
    /// Ausgeblendete Körper als [Feature, Index], wie in der Stückliste.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub koerper: Vec<[u64; 2]>,
    /// Ausgeblendete Skizzen und Konstruktionsebenen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<u64>,
    /// Eingeblendete Grundebenen.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub grundebenen: Vec<GrundebeneDto>,
    /// Ausgeblendete Ordner.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ordner: Vec<OrdnerDto>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum GrundebeneDto {
    Xy,
    Xz,
    Yz,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OrdnerDto {
    Koerper,
    Skizzen,
    Konstruktion,
    /// Ordner „Leinwände“ (ab Format 19); nur mit einer Leinwand im Dokument.
    Leinwaende,
}

/// Form eines STEP-Körpers (ab Format 10): Kennung (ab 1), Genauigkeit der Ursprungsdatei in mm
/// (1e-6 … 1e-3, für die Warnung „grob“) und der normalisierte STEP-Text (nur ASCII).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct StepFormDto {
    pub id: u64,
    pub genauigkeit: f64,
    /// Die Ursprungsdatei nennt keine Genauigkeit, `genauigkeit` ist angenommen (fehlt sonst).
    #[serde(default, skip_serializing_if = "is_false")]
    pub genauigkeit_unbekannt: bool,
    pub text: String,
}

/// Ein Parameter (ab Format 9). `art`: `"laenge"`, `"winkel"` oder `"zahl"`; ein leerer
/// Kommentar fehlt.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ParameterDto {
    pub id: u64,
    pub name: String,
    pub formel: String,
    pub art: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kommentar: String,
}

/// Formel an einem Skizzenmaß (ab Format 9): `abhaengigkeit` ist der Index in
/// `abhaengigkeiten` (gleich `ConstraintId::index()`, auch gelöschte Abhängigkeiten zählen).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SketchFormulaDto {
    pub abhaengigkeit: usize,
    pub formel: String,
}

/// Höchstlänge eines Fremdtexts aus der Datei in einer Meldung (Zeichen, samt „…“).
pub(crate) const MAX_QUOTED_CHARS: usize = 40;

/// Kürzt einen Text aus der Datei für eine Meldung auf höchstens [`MAX_QUOTED_CHARS`] Zeichen
/// (39 Zeichen und „…“); zählt Zeichen, nicht Bytes, und schneidet nie mitten in einem Zeichen.
pub(crate) fn quoted(text: &str) -> String {
    if text.chars().count() <= MAX_QUOTED_CHARS {
        text.to_string()
    } else {
        let mut s: String = text.chars().take(MAX_QUOTED_CHARS - 1).collect();
        s.push('…');
        s
    }
}

/// Höchstzahl der Parameter beim Lesen (eine mehr, damit die Prüfung danach im Klartext meldet).
const MAX_FILE_PARAMETERS: usize = planora_feature::MAX_PARAMETERS + 1;

fn de_parameters<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<ParameterDto>, D::Error> {
    de_capped(d, MAX_FILE_PARAMETERS)
}

/// Liest die Formeltabelle eines Features (Feldschlüssel → Formel); ein Schlüssel, der doppelt
/// vorkommt, ist ein Fehler (sonst gälte still der letzte).
fn de_formula_map<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, String>, D::Error> {
    struct Map;
    impl<'de> serde::de::Visitor<'de> for Map {
        type Value = BTreeMap<String, String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str(&tr!("proj-expect-formula-map"))
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut out = BTreeMap::new();
            while let Some((k, v)) = map.next_entry::<String, String>()? {
                if out.contains_key(&k) {
                    return Err(serde::de::Error::custom(tr!(
                        "proj-formula-field-twice",
                        key = quoted(&k)
                    )));
                }
                out.insert(k, v);
            }
            Ok(out)
        }
    }
    d.deserialize_map(Map)
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
            f.write_str(&tr!("proj-expect-list"))
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
    de_bounded(d, MAX_FILE_OVERRIDES, || tr!("proj-l-parts-list"))
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
fn de_bounded<'de, D, T>(d: D, max: usize, what: fn() -> String) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Bounded<T> {
        max: usize,
        what: fn() -> String,
        marker: std::marker::PhantomData<T>,
    }
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Bounded<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str(&tr!("proj-expect-list-max", max = self.max.to_string()))
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(self.max));
            while let Some(x) = seq.next_element()? {
                if out.len() == self.max {
                    return Err(serde::de::Error::custom(tr!(
                        "proj-list-too-many",
                        what = (self.what)(),
                        max = self.max.to_string()
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
    de_bounded(d, MAX_FILE_DRAWINGS, || tr!("proj-l-drawings"))
}

fn de_views<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<AnsichtDto>, D::Error> {
    de_bounded(d, MAX_FILE_VIEWS, || tr!("proj-l-views"))
}

/// Höchstzahl der Maße je Zeichnung, die beim Lesen überhaupt angenommen wird.
pub const MAX_FILE_DIMENSIONS: usize = planora_drawing::limits::MAX_DIMENSIONS;
/// Höchstzahl der Bezüge je Maß, die beim Lesen überhaupt angenommen wird.
pub const MAX_FILE_DIM_REFS: usize = 4;

fn de_dims<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<MassDto>, D::Error> {
    de_bounded(d, MAX_FILE_DIMENSIONS, || tr!("proj-l-dimensions"))
}

fn de_refs<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<BezugDto>, D::Error> {
    de_bounded(d, MAX_FILE_DIM_REFS, || tr!("proj-l-references"))
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
    /// Maßeinheit der Zeichnung (ab Format 12): `"in"` für Zoll. Fehlt bei Millimeter, damit
    /// Zeichnungen in mm Byte für Byte bleiben wie bisher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub einheit: Option<String>,
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
    /// Formeln der Zahlenfelder (ab Format 9): Feldschlüssel (`FieldKey::file_key`) → Formel.
    /// Leer heißt: fehlt (alte Dateien Byte für Byte).
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "de_formula_map"
    )]
    pub formeln: BTreeMap<String, String>,
}

pub type V3 = [f64; 3];

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BodyRefDto {
    pub feature: u64,
    pub index: usize,
}

/// Ein Bild einer Leinwand (ab Format 19): Kennung (ab 1), Format (`"png"`, `"jpeg"`, `"tiff"`) und
/// Base64 der unveränderten Datei-Bytes. Maße und Prüfsumme stehen nicht in der Datei, sie werden
/// beim Laden aus den Bytes gerechnet.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ImageDto {
    pub id: u64,
    pub format: String,
    pub daten: String,
}

/// Leinwand (ab Format 19, Skizzen 9): Bild aus `bilder` auf einer Grundebene, ebenen Fläche oder
/// Konstruktionsebene. `versatz` in mm in der Trägerebene, `winkel` im Bogenmaß gegen den
/// Uhrzeigersinn, `massstab` je Achse, `deckkraft` in Prozent. `letzte_lage` ist das zuletzt
/// berechnete Koordinatensystem des Trägers; fehlt, wenn die Leinwand nie berechnet wurde.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct LeinwandDto {
    pub bild: u64,
    pub lage: PlaneDto,
    pub versatz: [f64; 2],
    pub winkel: f64,
    pub massstab: [f64; 2],
    pub spiegeln_waagerecht: bool,
    pub spiegeln_senkrecht: bool,
    pub deckkraft: u8,
    pub durch_objekte: bool,
    pub auswaehlbar: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letzte_lage: Option<FrameDto>,
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

/// Lage einer Skizze und Bezug einer Konstruktionsebene: Grundebene, ebene Körperfläche oder
/// (ab Format 16) eine frühere Konstruktionsebene.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum PlaneDto {
    Standard { ebene: String },
    Flaeche { flaeche: FaceRefDto },
    Ebene { feature: u64 },
}

/// Fläche und Nummer des Stücks einer Schnitt- oder Silhouettenprojektion (ab Format 16).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FaceIndexDto {
    pub flaeche: FaceRefDto,
    pub index: usize,
}

/// Projizierte oder einbezogene Skizzengeometrie mit genau einem Ziel (`kurve` oder, ab Format
/// 20, `punkt`) und genau einer Herkunft: Kante (alle Formate), Schnitt oder Silhouette (ab
/// Format 16), dazu ab Format 20 `einbezug`, `skizzenkurve`, `auf_flaeche` und `schnittkurve`.
/// `stueck` ist der Stückindex in einer Gruppe gleicher Herkunft (weggelassen, wenn 0), `stuecke`
/// deren gespeicherte Stückzahl (weggelassen, wenn höchstens 1). Alte
/// Dateien haben nur `kurve` und `kante` (oder `schnitt`/`silhouette`) und schreiben sich gleich.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct EdgeCopyDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kurve: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub punkt: Option<usize>,
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub stueck: u32,
    /// Stückzahl der Gruppe; fehlt sie, ist sie 1 (weggelassen, wenn höchstens 1).
    #[serde(default = "one_u32", skip_serializing_if = "at_most_one")]
    pub stuecke: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kante: Option<EdgeRefDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schnitt: Option<FaceIndexDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub silhouette: Option<FaceIndexDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub einbezug: Option<EinbezugDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skizzenkurve: Option<SkizzenkurveDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auf_flaeche: Option<AufFlaecheDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schnittkurve: Option<SchnittkurveDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drehprofil: Option<DrehprofilDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isokurve: Option<IsokurveDto>,
}

/// „Drehprofil“ (ab Format 22): Auswahl (Körper oder Elemente), Achse, Toleranz in mm und
/// „Umkehren“.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DrehprofilDto {
    pub auswahl: DrehauswahlDto,
    pub achse: DrehachseDto,
    pub toleranz: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub umkehren: bool,
}

/// Auswahl des Drehprofils (ab Format 22): ganze Körper oder bestimmte Elemente.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum DrehauswahlDto {
    Koerper {
        #[serde(default)]
        koerper: Vec<BodyRefDto>,
    },
    Elemente {
        #[serde(default)]
        elemente: Vec<KurvenquelleDto>,
    },
}

/// Achse des Drehprofils (ab Format 22): Linie (Kurvennummer) der eigenen Skizze oder ein
/// Achsbezug wie bei der Ebene unter Winkel.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum DrehachseDto {
    Eigene { kurve: usize },
    Bezug { achse: AchseDto },
}

/// Isoparametrische Kurve (ab Format 22): Fläche, Richtung (`"u"` oder `"v"`) und Anteil 0–1.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct IsokurveDto {
    pub flaeche: FaceRefDto,
    pub richtung: String,
    pub anteil: f64,
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

fn one_u32() -> u32 {
    1
}

fn at_most_one(v: &u32) -> bool {
    *v <= 1
}

/// Kurve (Kurvennummer) einer anderen Skizze (ab Format 20).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SkizzenkurveDto {
    pub skizze: u64,
    pub kurve: usize,
}

/// Was „3D-Geometrie einbeziehen“ einzieht (ab Format 20): Kante, Rand einer Fläche, Ecke
/// (Anfang oder Ende einer Kante), Grund- oder Konstruktionsebene, Ursprungsachse (`x`, `y`,
/// `z`), Konstruktionspunkt (Feature), Ursprung oder Kurve einer anderen Skizze.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum EinbezugDto {
    Kante { kante: EdgeRefDto },
    Flaechenrand { flaeche: FaceRefDto },
    Ecke { kante: EdgeRefDto, ende: bool },
    Ebene { ebene: PlaneDto },
    Achse { achse: String },
    Konstruktionspunkt { feature: u64 },
    Ursprung,
    Skizzenkurve { skizze: u64, kurve: usize },
}

/// „Auf Fläche projizieren“ (ab Format 20): Quellkurven, Zielflächen und Projektionsart.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AufFlaecheDto {
    #[serde(default)]
    pub quellen: Vec<KurvenquelleDto>,
    #[serde(default)]
    pub flaechen: Vec<FlaechenzielDto>,
    pub art: ProjektionsartDto,
}

/// Quelle von „Auf Fläche projizieren“ (ab Format 20): Kurve einer anderen Skizze, Körperkante,
/// Ecke (Anfang oder Ende einer Kante) oder Konstruktionspunkt (Feature).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum KurvenquelleDto {
    Skizzenkurve { skizze: u64, kurve: usize },
    Kante { kante: EdgeRefDto },
    Ecke { kante: EdgeRefDto, ende: bool },
    Konstruktionspunkt { feature: u64 },
}

/// Zielfläche von „Auf Fläche projizieren“ (ab Format 20): Körperfläche oder Grund- bzw.
/// Konstruktionsebene.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum FlaechenzielDto {
    Flaeche { flaeche: FaceRefDto },
    Ebene { ebene: PlaneDto },
}

/// Projektionsart (ab Format 20): nächster Punkt oder entlang einer Richtung (Weltkoordinaten).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum ProjektionsartDto {
    NaechsterPunkt,
    EntlangVektor { richtung: V3 },
}

/// „Schnittkurve“ (ab Format 20): Kurven fertiger Skizzen und ihre Ziele.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SchnittkurveDto {
    #[serde(default)]
    pub kurven: Vec<KurvenquelleDto>,
    #[serde(default)]
    pub ziele: Vec<SchnittzielDto>,
}

/// Ziel der „Schnittkurve“ (ab Format 20): Körperfläche, Grund- bzw. Konstruktionsebene oder
/// Kurve einer anderen Skizze.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum SchnittzielDto {
    Flaeche { flaeche: FaceRefDto },
    Ebene { ebene: PlaneDto },
    Skizzenkurve { skizze: u64, kurve: usize },
}

/// Drehachse einer Ebene unter Winkel (ab Format 16): Ursprungsachse (`x`, `y`, `z`), gerade
/// Körperkante oder Linie (Kurvennummer) einer Skizze.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum AchseDto {
    Standard { achse: String },
    Kante { kante: EdgeRefDto },
    Skizzenlinie { skizze: u64, kurve: usize },
}

/// Punkt einer Ebene durch drei Punkte (ab Format 16): Ursprung, Anfang (`ende == false`) oder
/// Ende einer Körperkante, Punkt (Punktnummer) einer Skizze.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum PunktDto {
    Ursprung,
    Kantenende { kante: EdgeRefDto, ende: bool },
    Skizzenpunkt { skizze: u64, punkt: usize },
}

/// Konstruktionsebene (ab Format 16): Art und Eingaben, Längen in mm, Winkel in Grad.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[allow(
    clippy::large_enum_variant,
    reason = "steht in `KindDto` schon in einer Box; die drei Punkte bleiben so lesbar wie im Modell"
)]
#[serde(tag = "art", rename_all = "snake_case")]
pub enum PlaneFeatureDto {
    Versatz {
        basis: PlaneDto,
        abstand: f64,
    },
    Winkel {
        achse: AchseDto,
        winkel: f64,
    },
    Mitte {
        a: PlaneDto,
        b: PlaneDto,
    },
    DreiPunkte {
        punkte: [PunktDto; 3],
    },
    Tangential {
        flaeche: FaceRefDto,
        basis: PlaneDto,
        winkel: f64,
    },
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
    /// Ab Format 20: Index der z-Variable eines 3D-Punkts; fehlt bei einem 2D-Punkt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z: Option<usize>,
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
        /// Ab Format 20: z-Anteil der großen Halbachse einer Ellipse mit Normale.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        az: Option<usize>,
        klein: usize,
    },
    Spline {
        punkte: Vec<usize>,
        /// Ab Format 17: Griffe an Stützpunkten; fehlt, wenn es keine gibt.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        griffe: Vec<GriffDto>,
    },
    /// Ab Format 13: gegen den Uhrzeigersinn von `start` nach `ende` auf der Ellipse.
    Ellipsenbogen {
        mitte: usize,
        ax: usize,
        ay: usize,
        /// Ab Format 20: wie bei [`CurveKindDto::Ellipse`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        az: Option<usize>,
        klein: usize,
        start: usize,
        ende: usize,
    },
    /// Ab Format 17: Spline mit Kontrollpunkten; `grad` ist der wirksame Grad.
    KontrollSpline {
        punkte: Vec<usize>,
        grad: u8,
        knoten: Vec<f64>,
    },
    /// Ab Format 17: Kegelschnitt mit Scheitel und Rho.
    Kegelschnitt {
        start: usize,
        ende: usize,
        scheitel: usize,
        rho: f64,
    },
    /// Ab Format 17: projizierte Freiformkurve, unveränderlich; `punkte` in Skizzenkoordinaten.
    /// Geschlossen: `start == ende`.
    FesterSpline {
        start: usize,
        ende: usize,
        grad: usize,
        knoten: Vec<f64>,
        punkte: Vec<[f64; 2]>,
        gewichte: Vec<f64>,
        /// Ab Format 20: z der Kontrollpunkte einer Freiformkurve im Raum; fehlt, wenn alle
        /// null sind.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        hoehen: Vec<f64>,
    },
}

/// Griff an einem Stützpunkt eines Splines (ab Format 17).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct GriffDto {
    pub an: usize,
    pub linie: usize,
    pub spitze: usize,
    #[serde(default, skip_serializing_if = "GriffKruemmungDto::ist_form")]
    pub kruemmung: GriffKruemmungDto,
}

/// Krümmung an einem Griff (ab Format 17); fehlt in der Datei, wenn sie aus der Form folgt.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Default)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum GriffKruemmungDto {
    #[default]
    Form,
    Gerade,
    Bogen {
        bogen: usize,
        mitte: usize,
        anderes: usize,
    },
}

impl GriffKruemmungDto {
    fn ist_form(&self) -> bool {
        matches!(self, GriffKruemmungDto::Form)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct CurveDto {
    pub art: CurveKindDto,
    pub konstruktion: bool,
    /// Mittellinie (ab Format 14); fehlt in Dateien ohne Mittellinie.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mittellinie: bool,
    pub aktiv: bool,
    /// Kontrollpunktrahmen (ab Format 19); fehlt, wenn aus.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rahmen: bool,
    /// Ab Format 20: Normale einer ebenen Kurve außerhalb der Skizzenebene; fehlt in der
    /// Skizzenebene.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normale: Option<V3>,
    /// Ab Format 21: Krümmungskamm der Kurve; fehlt ohne Kamm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kamm: Option<KammDto>,
}

/// Krümmungskamm einer Kurve (ab Format 21): Zahl der Zähne und Maßstab ihrer Länge.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct KammDto {
    pub dichte: u32,
    pub massstab: f64,
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
    /// Ab Format 13: intern, die Enden eines Ellipsenbogens liegen auf seiner Ellipse.
    EllipsenbogenEnden {
        kurve: usize,
    },
    /// Ab Format 15: zwei Linien auf derselben Geraden.
    Kollinear {
        a: usize,
        b: usize,
    },
    /// Ab Format 17, intern: Krümmungsgriff des Splines `kurve` am Stützpunkt `punkt`.
    KruemmungsGriff {
        kurve: usize,
        punkt: usize,
    },
    /// Ab Format 15: glatter Übergang, Kurve und ihr Ende, Kurve und ihr Ende.
    Glatt {
        a: usize,
        punkt_a: usize,
        b: usize,
        punkt_b: usize,
    },
    /// Ab Format 15: waagerechter Abstand zweier Punkte.
    AbstandWaagerecht {
        a: usize,
        b: usize,
        wert: f64,
    },
    /// Ab Format 15: senkrechter Abstand zweier Punkte.
    AbstandSenkrecht {
        a: usize,
        b: usize,
        wert: f64,
    },
    /// Ab Format 15: doppelter Abstand eines Punktes von einer Mittellinie.
    DurchmesserZurAchse {
        punkt: usize,
        kurve: usize,
        wert: f64,
    },
    /// Ab Format 15: Gesamtlänge eines geraden Langlochs (Mitten a, b, eine Kappe).
    LanglochGesamt {
        a: usize,
        b: usize,
        kappe: usize,
        wert: f64,
    },
    /// Ab Format 15: ein Wert eines Musters (Index des Musters, Feld, Wert).
    MusterWert {
        muster: usize,
        feld: MusterFeldDto,
        wert: f64,
    },
    /// Ab Format 20: Linie oder zwei Punkte parallel zur z-Achse der Skizze.
    EntlangZ {
        ziel: LineOrPointsDto,
    },
    /// Ab Format 20: fester Punkt im Raum.
    #[serde(rename = "fest3")]
    Fest3 {
        punkt: usize,
        x: f64,
        y: f64,
        z: f64,
    },
    /// Ab Format 20, intern: Eine Kurve mit Normale bleibt in ihrer Ebene.
    InEbene {
        kurve: usize,
    },
    /// Ab Format 20, intern: große Halbachse (x, y, z) und kleine Halbachse einer einbezogenen
    /// Ellipse im Raum.
    Ellipsenachsen3 {
        kurve: usize,
        x: f64,
        y: f64,
        z: f64,
        klein: f64,
    },
    /// Ab Format 21: Tangente am Ende, Kurve und ihr Ende, Kurve und ihr Ende; die Richtungen am
    /// gemeinsamen Punkt sind gleich oder entgegengesetzt.
    TangenteEnde {
        a: usize,
        punkt_a: usize,
        b: usize,
        punkt_b: usize,
    },
}

/// Feld eines Musterwerts.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MusterFeldDto {
    Anzahl1,
    Abstand1,
    Anzahl2,
    Abstand2,
    Anzahl,
    Winkel,
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
    /// Ab Format 21: Das Winkelmaß misst den Gegenwinkel; fehlt, wenn aus.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub gegenwinkel: bool,
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
    /// Muster (ab Format 15), auch aufgelöste; fehlt in Skizzen ohne Muster.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub muster: Vec<PatternDto>,
    /// Texte (ab Format 18), auch gelöschte; fehlt in Skizzen ohne Text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texte: Vec<TextDto>,
    /// Haken „3D-Skizze“ (ab Format 20); fehlt, wenn aus.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub drei_d: bool,
}

/// Waagerechte Ausrichtung eines Textes (ab Format 18).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WaagerechtDto {
    Links,
    Mitte,
    Rechts,
}

/// Senkrechte Ausrichtung eines Textes im Rahmen (ab Format 18).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SenkrechtDto {
    Oben,
    Mitte,
    Unten,
}

/// Wo ein Text steht (ab Format 18): im Rahmen aus vier Linien zwischen vier Ecken (Linie `i`
/// von Ecke `i` nach Ecke `i + 1`, Linie 0 ist die Grundlinie) oder auf einer Kurve.
///
/// Bewusst extern getaggt (`"lage": {"rahmen": {…}}`), anders als die übrigen Arten mit
/// `tag = "typ"`: So stehen in der Datei genau die Schlüssel, die Spec G2 nennt (`rahmen` oder
/// `pfad` mit ihren Feldern), ohne zusätzlichen `typ`. Ändern hieße ein neues Format.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextLageDto {
    Rahmen {
        linien: [usize; 4],
        ecken: [usize; 4],
        waagerecht: WaagerechtDto,
        senkrecht: SenkrechtDto,
    },
    Pfad {
        kurve: usize,
        oberhalb: bool,
        anpassen: bool,
        waagerecht: WaagerechtDto,
    },
}

/// Ein Text einer Skizze (ab Format 18, Skizzen 8). Nur der Familienname, Fett und Kursiv, nie
/// Schriftdaten; die erzeugten Kurven und Punkte stehen als normale Einträge in der Skizze.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TextDto {
    pub inhalt: String,
    pub schrift: String,
    pub fett: bool,
    pub kursiv: bool,
    /// Höhe in mm (Geviert der Schrift).
    pub hoehe: f64,
    /// Zeichenabstand in Prozent.
    pub abstand: f64,
    pub umkehren_waagerecht: bool,
    pub umkehren_senkrecht: bool,
    pub lage: TextLageDto,
    /// Ab Format 20: Normale der Ebene eines Textes in 3D; fehlt in der Skizzenebene.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normale: Option<V3>,
    pub kurven: Vec<usize>,
    pub punkte: Vec<usize>,
    pub lebt: bool,
}

/// Richtung eines Rechteckmusters (ab Format 15).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum MusterRichtungDto {
    SkizzeX,
    SkizzeY,
    Linie {
        kurve: usize,
    },
    /// Senkrecht zur ersten Richtung (nur als zweite Richtung).
    Senkrecht,
}

/// Art eines Kreismusters (ab Format 15).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KreisArtDto {
    Voll,
    Winkel,
    Symmetrisch,
}

/// Art eines Musters (ab Format 15). `gesamt`: der Wert ist die Gesamtlänge statt des Abstands.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(tag = "typ", rename_all = "snake_case")]
pub enum MusterArtDto {
    Rechteck {
        richtungen: [MusterRichtungDto; 2],
        gesamt: [bool; 2],
        symmetrisch: [bool; 2],
    },
    Kreis {
        mitte: usize,
        art: KreisArtDto,
    },
}

/// Ein Exemplar eines Musters an seiner Lage im Gitter: Punkte parallel zu den Vorlagenpunkten,
/// Kurven parallel zu den Vorlagen (ab Format 15).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ExemplarDto {
    pub lage: [i32; 2],
    pub punkte: Vec<usize>,
    pub kurven: Vec<usize>,
}

/// Ein Muster (ab Format 15): Punkte, Kurven und Werte (`muster_wert`) als Indizes. Die Exemplare
/// stehen mit ihren Ids in der Datei, damit Bindungen an sie bleiben.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct PatternDto {
    pub art: MusterArtDto,
    pub vorlagen: Vec<usize>,
    pub vorlagenpunkte: Vec<usize>,
    pub werte: Vec<usize>,
    pub exemplare: Vec<ExemplarDto>,
    /// Abgewählte Lagen im Gitter.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abgewaehlt: Vec<[i32; 2]>,
    pub lebt: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SketchFeatureDto {
    pub skizze: SketchDto,
    pub lage: PlaneDto,
    pub kopien: Vec<EdgeCopyDto>,
    /// Formeln der bestimmenden Maße (ab Format 9), aufsteigend nach Abhängigkeit. Leer heißt:
    /// fehlt (alte Dateien Byte für Byte).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formeln: Vec<SketchFormulaDto>,
    /// Höhenformeln der Texte (ab Format 22), aufsteigend nach Text. Leer heißt: fehlt (alte
    /// Dateien Byte für Byte).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hoehenformeln: Vec<TextFormulaDto>,
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
        /// Alte Profilnummer (bisherige Extraktion); genau eins von `profil` und `profile`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        profil: Option<usize>,
        /// Profil-Verweise (ab Format 13, Teilprojekt Skizzen 1), nur wenn sie sich nicht als
        /// alte Profilnummer schreiben lassen.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        profile: Option<Vec<ProfilVerweisDto>>,
        hoehe: f64,
    },
    Rotation {
        skizze: u64,
        /// Alte Profilnummer (bisherige Extraktion); genau eins von `profil` und `profile`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        profil: Option<usize>,
        /// Profil-Verweise (ab Format 13, Teilprojekt Skizzen 1), nur wenn sie sich nicht als
        /// alte Profilnummer schreiben lassen.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        profile: Option<Vec<ProfilVerweisDto>>,
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
    /// STEP-Körper (ab Format 10): Form aus `step_formen`. Keine Lagefelder (Plan P6).
    StepKoerper {
        form: u64,
    },
    /// Konstruktionsebene (ab Format 16, Skizzen 6).
    Konstruktionsebene(Box<PlaneFeatureDto>),
    /// Leinwand (ab Format 19, Skizzen 9).
    Leinwand(Box<LeinwandDto>),
}

/// Ein Profil-Verweis: Stücke des äußeren Rands in Umlaufreihenfolge, danach die Stücke aller
/// Löcher (siehe `planora_sketch::ProfileRef`).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ProfilVerweisDto {
    pub aussen: Vec<StueckDto>,
    pub loecher: Vec<StueckDto>,
}

/// Ein Stück eines Profilrands als Tupel: Kurven-Index, normierter Parameterbereich `u0`, `u1`,
/// liegt die Fläche links der Kurvenrichtung, Länge in mm.
pub type StueckDto = (usize, f64, f64, bool, f64);
