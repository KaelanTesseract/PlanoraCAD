//! Text in der Skizze (Skizzen 8, Spec B).
//!
//! Ein Text steht als eigene Art ([`TextData`]) in der Arena `texts` der Skizze, wie ein Muster.
//! Aus dem gesetzten [`TextShape`] und der Lage im Rahmen oder auf einem Pfad entstehen
//! echte Skizzenkurven: Strecke → `Line`, quadratisch → `Conic` mit ρ = 0,5, kubisch →
//! `ControlSpline` Grad 3. Ihre Punkte sind für den Löser Konstanten wie Musterexemplare;
//! `refresh_texts` (in `text_refresh.rs`) schreibt sie vor und nach jedem Lösen neu, mit
//! gleichen Ids (Spec B3).
//!
//! Das Setzen selbst (Schrift → [`TextShape`]) geschieht nur außerhalb des Lösers, beim Anlegen,
//! beim Ändern und über [`Sketch::reshape_texts`]. Fehlt das gesetzte `TextShape` (gerade
//! geladen), bleiben die gespeicherten Kurven unverändert stehen.

use crate::constraint::{ConstraintKind, LineOrPoints, curve_ends};
use crate::error::SketchError;
use crate::geom::Geom;
use crate::model::{
    CurveId, CurveKind, PointId, Sketch, plane_basis, unit_normal, within_coord_range,
};
use crate::profile::ProfileRef;
use crate::text_layout::{Walk, place_box, place_path};
use crate::text_shape::{
    Contour, MAX_TEXT_CHARS, MAX_TEXT_CURVES, Seg, TextShape, TextShapeError, TextShaper, TextStyle,
};
use planora_i18n::tr;
use planora_kernel::Id;
use planora_math::{DVec2, DVec3};
use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::Arc;

pub type TextId = Id<TextData>;

/// Waagerechte Ausrichtung: an der Grundlinie des Rahmens bzw. am Pfad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

/// Senkrechte Ausrichtung zwischen Grund- und Gegenlinie des Rahmens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VAlign {
    Top,
    Middle,
    Bottom,
}

/// Wo der Text steht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextPlacement {
    /// Im Rahmen aus vier Konstruktionslinien: Linie `i` läuft von Ecke `i` nach Ecke `i + 1`,
    /// Linie 0 ist die Grundlinie, ihre Richtung der Winkel des Textes.
    Box {
        lines: [CurveId; 4],
        corners: [PointId; 4],
        h: HAlign,
        v: VAlign,
    },
    /// Auf der Kurve `path` (Spec B7): oberhalb (`above`, links der Leserichtung) oder
    /// unterhalb, „An Pfad anpassen“ (`fit`), Ausrichtung am Pfad.
    Path {
        path: CurveId,
        above: bool,
        fit: bool,
        h: HAlign,
    },
}

/// Die neue Art und Lage beim Ändern eines Textes ([`Sketch::edit_text`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextKindEdit {
    /// Im Rahmen: der vorhandene, oder beim Wechsel vom Pfad ein neuer um die aktuelle Lage.
    Box { h: HAlign, v: VAlign },
    /// Auf der Kurve `path`, wie bei [`TextPlacement::Path`].
    Path {
        path: CurveId,
        above: bool,
        fit: bool,
        h: HAlign,
    },
}

/// Das zuletzt gesetzte [`TextShape`], nur zur Laufzeit (nicht in der Datei), und ob das letzte
/// Neusetzen ([`Sketch::reshape_texts`]) scheiterte.
///
/// Der Vergleich liefert immer `true` (**Review Focus 1**): Rückgängig-Bilder und „geändert“
/// sehen den Zwischenspeicher nie. Ein bloßes Neusetzen nach dem Laden, das dieselben Kurven
/// liefert, ist so keine Änderung.
#[derive(Clone, Debug, Default)]
pub struct ShapedCache {
    pub(crate) shape: Option<Arc<TextShape>>,
    /// Das letzte Neusetzen lieferte keine Form, zu viele Kurven oder keine Kurve, wo der Text
    /// welche hat. Die Form davor (oder keine) und die gespeicherten Kurven bleiben (Anti-Ziel:
    /// eine fehlende Schrift lässt den Text nicht verschwinden), die Skizze warnt.
    pub(crate) unset: bool,
}

impl ShapedCache {
    /// Eine neu gesetzte Form; ein früheres Scheitern gilt nicht mehr.
    pub(crate) fn set(shape: Arc<TextShape>) -> ShapedCache {
        ShapedCache {
            shape: Some(shape),
            unset: false,
        }
    }
}

impl PartialEq for ShapedCache {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// Die zuletzt benutzte Längentabelle des Pfades eines Textes auf Pfad, nur zur Laufzeit. Sie ist
/// eine reine Funktion der Pfadgeometrie ([`Walk::fits`]) und gilt nur, solange der Pfad gleich
/// bleibt. Der Vergleich liefert wie bei [`ShapedCache`] immer `true`.
#[derive(Clone, Debug, Default)]
pub(crate) struct WalkCache(pub(crate) Option<Arc<Walk>>);

impl PartialEq for WalkCache {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// Ein Text in der Skizze.
#[derive(Clone, Debug, PartialEq)]
pub struct TextData {
    pub content: String,
    /// Familienname wie gewählt, Fett, Kursiv, Zeichenabstand in Prozent.
    pub style: TextStyle,
    /// Höhe in mm (Geviert der Schrift).
    pub height: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub placement: TextPlacement,
    /// Normale der Ebene eines Textes in 3D (Skizzen 10, Spec Entwurf 4): Rahmen und Buchstaben
    /// liegen in der Ebene durch Ecke 0 mit dieser Normale, alle ihre Punkte sind 3D. Sie ist eine
    /// feste Zahl: Interne `InPlane` an den Rahmenlinien 0 bis 2 halten die Ecken in der Ebene,
    /// der Löser dreht sie nie (Spec 1). `None` für einen gewöhnlichen Text in der Skizzenebene.
    pub normal: Option<DVec3>,
    /// Die erzeugten Kurven in fester Reihenfolge (Zeile, Glyphe, Kontur, Stück).
    pub curves: Vec<CurveId>,
    /// Ihre Punkte in fester Reihenfolge; je Kontur der Anfang, dann je Stück die Kontrollpunkte
    /// und das Ende, außer beim letzten Stück (es endet am Anfang).
    pub points: Vec<PointId>,
    pub(crate) alive: bool,
    /// Der gezeigte Inhalt (Skizzen 11b, Parameter im Text): der Inhalt mit ersetzten `{Name}`,
    /// aus dem die Kurven entstehen; `None` heißt: wie `content`. Nur zur Laufzeit, nie in der
    /// Datei ([`TextRecord`](crate::data::TextRecord) hat ihn nicht); gesetzt mit
    /// [`Sketch::set_text_shown`] (die Feature-Seite ersetzt, `planora_feature::text_refs`).
    pub shown: Option<String>,
    pub(crate) shaped: ShapedCache,
    pub(crate) walk: WalkCache,
}

impl TextData {
    /// Das zuletzt gesetzte Ergebnis, nur zur Laufzeit; `None`, solange der Text nicht gesetzt
    /// ist (nach dem Laden bis [`Sketch::reshape_texts`]). Die Feature-Berechnung liest daraus
    /// etwa die ersetzte Schrift (Spec C10).
    pub fn shape(&self) -> Option<&TextShape> {
        self.shaped.shape.as_deref()
    }

    /// Ließ sich der Text beim letzten Neusetzen mit der Schriftquelle nicht setzen (keine Kurven
    /// oder zu viele, etwa mit der Ersatzschrift)? Dann stehen seine Kurven unverändert da, und die
    /// Skizze warnt (`feature-sketch-text-unset`). Nur zur Laufzeit; ein neu gesetzter Text ist es
    /// nicht mehr.
    pub fn is_unset(&self) -> bool {
        self.shaped.unset
    }
}

/// Was ein Text werden soll, ohne die Lage.
#[derive(Clone, Debug, PartialEq)]
pub struct TextParams {
    pub content: String,
    pub style: TextStyle,
    /// Höhe in mm, größer als null.
    pub height: f64,
    pub flip_h: bool,
    pub flip_v: bool,
}

/// Ergebnis von [`Sketch::reshape_texts`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReshapeReport {
    /// Kurven oder Punkte haben sich geändert (dann neuer Stand der Skizze).
    pub changed: bool,
    /// Texte, deren Schrift fehlt, mit dem gewünschten Familiennamen.
    pub substituted: Vec<(TextId, String)>,
    /// Texte mit nachgebildetem Fett oder Kursiv.
    pub synthetic: Vec<TextId>,
    /// Texte, in denen Zeichen fehlen.
    pub missing_chars: Vec<TextId>,
    /// Texte, die sich nicht setzen ließen: Der Setzer lieferte einen Fehler, zu viele Kurven oder
    /// keine Kurve, wo der Text welche hat. Ihre Kurven und ihre Form bleiben, wie sie waren
    /// ([`TextData::is_unset`]).
    pub unset: Vec<TextId>,
}

/// Warum ein Teil eines Textes nicht allein gelöscht werden darf (Spec B8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRemoval {
    /// Rahmenlinie, Ecke oder erzeugte Kurve: nur mit dem ganzen Text
    /// („sketch-text-part“).
    Part,
    /// Pfad (oder Punkt eines Pfades), an dem Text hängt („sketch-text-on-path“).
    OnPath,
}

impl TextRemoval {
    /// Die Ablehnung als Fehler der Skizze.
    pub fn error(self) -> SketchError {
        SketchError::InvalidInput(match self {
            TextRemoval::Part => tr!("sketch-text-part"),
            TextRemoval::OnPath => tr!("sketch-text-on-path"),
        })
    }
}

/// Größter erlaubter Zeichenabstand in Prozent (Spec A7); auch die Oberfläche prüft damit.
pub const MAX_SPACING: f64 = 1000.0;

impl From<TextShapeError> for SketchError {
    fn from(e: TextShapeError) -> SketchError {
        match e {
            TextShapeError::TooLong => SketchError::InvalidInput(tr!(
                "sketch-text-too-long",
                max = MAX_TEXT_CHARS.to_string()
            )),
            TextShapeError::TooManyCurves => SketchError::InvalidInput(tr!(
                "sketch-text-too-many-curves",
                max = MAX_TEXT_CURVES.to_string()
            )),
            TextShapeError::Empty => SketchError::InvalidInput(tr!("sketch-text-empty")),
        }
    }
}

/// Prüft die Werte eines Textes: Höhe endlich und größer als null, Zeichenabstand in
/// `0 ≤ p ≤ 1000`, Inhalt nicht leer und höchstens [`MAX_TEXT_CHARS`] Zeichen.
fn check_params(p: &TextParams) -> Result<(), SketchError> {
    if !(p.height.is_finite() && p.height > 0.0) {
        return Err(SketchError::InvalidInput(tr!("sketch-text-height")));
    }
    if !(0.0..=MAX_SPACING).contains(&p.style.spacing) {
        return Err(SketchError::InvalidInput(tr!(
            "sketch-text-spacing",
            max = format!("{MAX_SPACING}")
        )));
    }
    if p.content.chars().count() > MAX_TEXT_CHARS {
        return Err(TextShapeError::TooLong.into());
    }
    if p.content.trim().is_empty() {
        return Err(TextShapeError::Empty.into());
    }
    Ok(())
}

/// Höchstens [`MAX_TEXT_CURVES`] Kurven.
fn check_curve_limit(shape: &TextShape) -> Result<(), SketchError> {
    if shape.segment_count() > MAX_TEXT_CURVES {
        Err(TextShapeError::TooManyCurves.into())
    } else {
        Ok(())
    }
}

/// Ein neuer oder geänderter Text: höchstens [`MAX_TEXT_CURVES`] Kurven und mindestens eine
/// (ein Text nur aus Zeichen ohne Umriss gilt wie ein leerer).
fn check_new_shape(shape: &TextShape) -> Result<(), SketchError> {
    check_curve_limit(shape)?;
    if shape.segment_count() == 0 {
        return Err(TextShapeError::Empty.into());
    }
    Ok(())
}

/// Alle Punkte endlich und im Rechenbereich.
fn placed_ok(contours: &[Contour]) -> bool {
    contours.iter().all(|c| {
        within_coord_range(c.start)
            && c.segs.iter().all(|s| match *s {
                Seg::Line(p) => within_coord_range(p),
                Seg::Quad(a, p) => within_coord_range(a) && within_coord_range(p),
                Seg::Cubic(a, b, p) => {
                    within_coord_range(a) && within_coord_range(b) && within_coord_range(p)
                }
            })
    })
}

/// Liegt `p` mit allen drei Werten im Rechenbereich?
fn within_range3(p: DVec3) -> bool {
    within_coord_range(p.truncate()) && within_coord_range(DVec2::new(p.z, 0.0))
}

/// Unter dieser Abweichung gilt die Achse eines Textrahmens in 3D als Achse der Skizze.
const FRAME_AXIS_TOL: f64 = 1e-9;

/// Die Ebene eines Textes in 3D als 2D-Basis (Skizzen 10): Ursprung Ecke 0, `u` die Richtung der
/// Grundlinie, `v = n × u` und die gespeicherte Normale `n`. Gesetzt wird in dieser Basis genau
/// wie in 2D, danach in den Raum gelegt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Lift {
    pub(crate) origin: DVec3,
    pub(crate) u: DVec3,
    pub(crate) v: DVec3,
    pub(crate) n: DVec3,
}

impl Lift {
    /// Die Basis des Rahmens mit den Ecken `at` in der Ebene mit der gespeicherten Normale `n`
    /// des Textes (eine feste Zahl, der Löser dreht sie nie): `u` ist die Grundlinie, in die
    /// Ebene gelegt. `None`, wenn sie dort nicht länger als `tol` ist oder etwas nicht endlich
    /// ist.
    pub(crate) fn of_frame(at: [DVec3; 4], n: DVec3, tol: f64) -> Option<Lift> {
        let d = at[1] - at[0];
        let e = d - n * d.dot(n);
        let len = e.length();
        if !(len.is_finite() && len > tol && n.is_finite()) {
            return None;
        }
        let u = e / len;
        Some(Lift {
            origin: at[0],
            u,
            v: n.cross(u),
            n,
        })
    }

    /// Lage in der Ebene (senkrecht hineingelegt).
    pub(crate) fn to2(&self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.u), d.dot(self.v))
    }

    /// Lage im Raum der Skizze.
    pub(crate) fn to3(&self, q: DVec2) -> DVec3 {
        self.origin + self.u * q.x + self.v * q.y
    }
}

/// `r` um die normierte Achse `n` gedreht (Rodrigues), mit `sin` und `cos` des Winkels.
fn rotate_about(r: DVec3, n: DVec3, sin: f64, cos: f64) -> DVec3 {
    r * cos + n.cross(r) * sin + n * (n.dot(r) * (1.0 - cos))
}

/// Liegt die Richtung `d` auf einer Achse der Skizze? Dann die Abhängigkeit dazu an der Linie
/// `line`: X `Horizontal`, Y `Vertical`, Z `AlongZ` (wie beim Rechteck in 3D).
fn axis_binding(d: DVec3, line: CurveId) -> Option<ConstraintKind> {
    let along = |a: DVec3| d.dot(a).abs() >= 1.0 - FRAME_AXIS_TOL;
    let l = LineOrPoints::Line(line);
    if along(DVec3::X) {
        Some(ConstraintKind::Horizontal(l))
    } else if along(DVec3::Y) {
        Some(ConstraintKind::Vertical(l))
    } else if along(DVec3::Z) {
        Some(ConstraintKind::AlongZ(l))
    } else {
        None
    }
}

/// Bindet die interne Abhängigkeit `kind` die Grundlinie oder die Seite des Rahmens mit den
/// Linien `lines` an eine Achse der Skizze? Diese fallen beim Drehen weg.
fn is_frame_axis(kind: &ConstraintKind, lines: &[CurveId; 4]) -> bool {
    matches!(kind,
        ConstraintKind::Horizontal(LineOrPoints::Line(l))
        | ConstraintKind::Vertical(LineOrPoints::Line(l))
        | ConstraintKind::AlongZ(LineOrPoints::Line(l)) if *l == lines[0] || *l == lines[1])
}

/// „Text auf Pfad“ gibt es nur in 2D (Spec Entwurf 4).
fn path_3d() -> SketchError {
    SketchError::InvalidInput(tr!("sketch-err-text-path-3d"))
}

fn out_of_range() -> SketchError {
    SketchError::InvalidInput(tr!("sketch-text-out-of-range"))
}

/// Mitte des Hüllquaders der Punkte; `None` ohne Punkte oder bei nicht endlichen Werten.
fn mid_of(points: impl Iterator<Item = DVec2>) -> Option<DVec2> {
    let (lo, hi) = points.fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(p), hi.max(p)),
    );
    let mid = 0.5 * (lo + hi);
    mid.is_finite().then_some(mid)
}

/// Die Punkte der Konturen, wie `sync_text` sie anlegt: Konturen mit weniger als zwei Stücken
/// entfallen.
fn contour_points(contours: &[Contour]) -> impl Iterator<Item = DVec2> + '_ {
    contours.iter().filter(|c| c.segs.len() >= 2).flat_map(|c| {
        std::iter::once(c.start).chain(c.segs.iter().flat_map(|s| match *s {
            Seg::Line(p) => vec![p],
            Seg::Quad(a, p) => vec![a, p],
            Seg::Cubic(a, b, p) => vec![a, b, p],
        }))
    })
}

/// Was für einen Text auf Pfad gesetzt wird: Die Zeilen werden mit einem Leerzeichen verbunden
/// (Spec B7, **Annahme**); Zeilenumbrüche wie beim Setzen (`\r\n`, `\r`, `\n`). Gespeichert bleibt
/// der Inhalt, wie er ist, damit ein Wechsel in den Rahmen die Zeilen zurückbringt.
pub fn path_content(content: &str) -> String {
    content.replace("\r\n", "\n").replace(['\r', '\n'], " ")
}

/// Was für den Text `d` gesetzt wird: der gezeigte Inhalt ([`TextData::shown`], sonst der
/// Inhalt), auf einem Pfad mit verbundenen Zeilen.
fn shaping_content(d: &TextData) -> Cow<'_, str> {
    let content = d.shown.as_deref().unwrap_or(&d.content);
    match d.placement {
        TextPlacement::Box { .. } => Cow::Borrowed(content),
        TextPlacement::Path { .. } => Cow::Owned(path_content(content)),
    }
}

impl Sketch {
    /// Ein lebender Text.
    pub fn text(&self, t: TextId) -> Option<&TextData> {
        self.texts.get(t).filter(|d| d.alive)
    }

    /// Alle lebenden Texte.
    pub fn texts(&self) -> impl Iterator<Item = (TextId, &TextData)> {
        self.texts.iter().filter(|(_, d)| d.alive)
    }

    /// Der Text, zu dem die erzeugte Kurve `c` gehört (Rahmenlinien zählen nicht, siehe
    /// [`Sketch::text_of_frame`]).
    pub fn text_of_curve(&self, c: CurveId) -> Option<TextId> {
        self.texts()
            .find(|(_, d)| d.curves.contains(&c))
            .map(|(t, _)| t)
    }

    /// Der Text, aus dem das Profil `p` stammt (Spec D2): `Some`, wenn alle Kanten des äußeren
    /// Rands und der Löcher erzeugte Kurven desselben lebenden Textes sind, auch bei Teilflächen
    /// überlappender Glyphen. Rahmenlinien zählen nicht zum Text.
    pub fn text_of_profile(&self, p: &ProfileRef) -> Option<TextId> {
        let mut want: HashSet<CurveId> = p.outer.iter().chain(&p.holes).map(|r| r.curve).collect();
        let &first = want.iter().next()?;
        let t = self.text_of_curve(first)?;
        // Ein Durchgang über die Kurven des Textes statt einer Suche je Kante.
        for c in &self.texts[t].curves {
            if want.remove(c) && want.is_empty() {
                break;
            }
        }
        want.is_empty().then_some(t)
    }

    /// Der Text, dessen Rahmenlinie `c` ist.
    pub fn text_of_frame(&self, c: CurveId) -> Option<TextId> {
        self.texts()
            .find(|(_, d)| matches!(d.placement, TextPlacement::Box { lines, .. } if lines.contains(&c)))
            .map(|(t, _)| t)
    }

    /// Ist der Text gesetzt? Nach dem Laden ist er es erst nach [`Sketch::reshape_texts`]; bis
    /// dahin bleiben die gespeicherten Kurven, wie sie sind.
    pub fn is_text_shaped(&self, t: TextId) -> bool {
        self.text(t).is_some_and(|d| d.shaped.shape.is_some())
    }

    /// Die Variablen aller erzeugten Punkte lebender Texte: für den Löser Konstanten, auch bei
    /// einem noch nicht gesetzten Text.
    pub(crate) fn text_vars(&self) -> HashSet<usize> {
        let mut out = HashSet::new();
        for (_, d) in self.texts() {
            for &p in &d.points {
                let d = &self.points[p];
                if d.alive {
                    out.extend([d.x, d.y]);
                    out.extend(d.z);
                }
            }
        }
        out
    }

    /// Die Normale des Textes in 3D, dessen Rahmenlinie `c` ist, auch eines gelöschten (für
    /// `InPlane` an Rahmenlinien, Task 13); `None` für jede andere Kurve.
    pub(crate) fn text_frame_normal(&self, c: CurveId) -> Option<DVec3> {
        if self.texts.is_empty() {
            return None;
        }
        self.texts.iter().find_map(|(_, d)| match d.placement {
            TextPlacement::Box { lines, .. } if lines.contains(&c) => d.normal,
            _ => None,
        })
    }

    /// Ist `p` ein erzeugter Punkt eines lebenden Textes?
    pub(crate) fn is_text_point(&self, p: PointId) -> bool {
        self.texts().any(|(_, d)| d.points.contains(&p))
    }

    /// Ist `p` eine Ecke des Rahmens eines lebenden Textes?
    fn is_text_corner(&self, p: PointId) -> bool {
        self.texts().any(
            |(_, d)| matches!(d.placement, TextPlacement::Box { corners, .. } if corners.contains(&p)),
        )
    }

    /// Ist `c` der Pfad eines lebenden Textes?
    fn is_text_path(&self, c: CurveId) -> bool {
        self.texts()
            .any(|(_, d)| matches!(d.placement, TextPlacement::Path { path, .. } if path == c))
    }

    /// Warum die Kurve `c` nicht allein gelöscht werden darf (Spec B8): Rahmenlinien und erzeugte
    /// Kurven gehen nur mit dem ganzen Text, ein Pfad mit Text gar nicht. `None`: kein Hindernis
    /// durch einen Text.
    pub fn text_curve_removal(&self, c: CurveId) -> Option<TextRemoval> {
        if self.texts.is_empty() {
            return None;
        }
        if self.text_of_curve(c).is_some() || self.text_of_frame(c).is_some() {
            return Some(TextRemoval::Part);
        }
        self.is_text_path(c).then_some(TextRemoval::OnPath)
    }

    /// Wie [`Sketch::text_curve_removal`] für den Punkt `p`: Ecken und erzeugte Punkte gehen nur
    /// mit dem ganzen Text, ein Punkt, dessen Kurve ein Pfad mit Text ist, gar nicht.
    pub fn text_point_removal(&self, p: PointId) -> Option<TextRemoval> {
        if self.texts.is_empty() {
            return None;
        }
        if self.is_text_point(p) || self.is_text_corner(p) {
            return Some(TextRemoval::Part);
        }
        self.curves()
            .filter(|(_, d)| d.kind.points().contains(&p))
            .find_map(|(c, _)| self.text_curve_removal(c))
    }

    /// Vor dem Löschen der Kurve `c` ([`Sketch::text_curve_removal`]).
    pub(crate) fn refuse_text_curve_removal(&self, c: CurveId) -> Result<(), SketchError> {
        match self.text_curve_removal(c) {
            Some(r) => Err(r.error()),
            None => Ok(()),
        }
    }

    /// Vor dem Löschen des Punktes `p` ([`Sketch::text_point_removal`]).
    pub(crate) fn refuse_text_point_removal(&self, p: PointId) -> Result<(), SketchError> {
        match self.text_point_removal(p) {
            Some(r) => Err(r.error()),
            None => Ok(()),
        }
    }

    /// Maße und Abhängigkeiten gehen an den Rahmen oder den Pfad, nicht an die Buchstaben
    /// (Spec E, Ruling 1 des Plans): abgelehnt, wenn `kind` eine erzeugte Kurve oder einen
    /// erzeugten Punkt benutzt.
    pub(crate) fn refuse_text_glyphs(&self, kind: &ConstraintKind) -> Result<(), SketchError> {
        if self.texts().next().is_none() {
            return Ok(());
        }
        let curves = kind.curves();
        let points = kind.points();
        let hit = self.texts().any(|(_, d)| {
            curves.iter().any(|c| d.curves.contains(c))
                || points.iter().any(|p| d.points.contains(p))
        });
        if hit {
            Err(SketchError::InvalidInput(tr!("sketch-text-no-dimension")))
        } else {
            Ok(())
        }
    }

    /// Die erzeugten Kurven aller lebenden Texte, einmal gesammelt für Schleifen über viele
    /// Kurven (Fang, Richtungen beim Zeichnen); `text_of_curve` sucht dagegen jedes Mal alle Texte
    /// ab.
    pub fn glyph_curves(&self) -> HashSet<CurveId> {
        self.texts()
            .flat_map(|(_, d)| d.curves.iter().copied())
            .collect()
    }

    /// Die erzeugten Punkte aller lebenden Texte (siehe [`Sketch::glyph_curves`]).
    pub fn glyph_points(&self) -> HashSet<PointId> {
        self.texts()
            .flat_map(|(_, d)| d.points.iter().copied())
            .collect()
    }

    /// Ist eine der Kurven `curves` eine erzeugte Kurve eines lebenden Textes?
    pub(crate) fn has_glyph(&self, curves: &[CurveId]) -> bool {
        !self.texts.is_empty() && curves.iter().any(|&c| self.text_of_curve(c).is_some())
    }

    /// Ist eine der Kurven `curves` ein Buchstabe oder eine Rahmenlinie eines lebenden Textes?
    pub fn has_text_part(&self, curves: &[CurveId]) -> bool {
        self.has_glyph(curves)
            || (!self.texts.is_empty() && curves.iter().any(|&c| self.text_of_frame(c).is_some()))
    }

    /// Gemeinsame Prüfung der Befehle aus Spec E: abgelehnt mit der Meldung von `message` (ein
    /// `tr!`, damit der Schlüssel im Code steht), wenn eine der Kurven `curves` eine erzeugte Kurve
    /// eines Textes ist. Rahmenlinien und Pfade gehen wie andere Linien und Kurven. Die Skizze
    /// bleibt unberührt.
    pub fn refuse_text_curves(
        &self,
        curves: &[CurveId],
        message: impl FnOnce() -> String,
    ) -> Result<(), SketchError> {
        if self.has_glyph(curves) {
            return Err(SketchError::InvalidInput(message()));
        }
        Ok(())
    }

    /// Wie [`Sketch::refuse_text_curves`], dazu Rahmenlinien: für Befehle, die eine Kurve
    /// zerschneiden oder ihre Enden versetzen (Stutzen, Dehnen, Brechen, Abrunden, Fase). Am
    /// Rahmen zerrissen sie das Rechteck und lösten Ecken vom Text (**Annahme**, Spec E nennt nur
    /// die Buchstaben).
    pub(crate) fn refuse_text_parts(
        &self,
        curves: &[CurveId],
        message: impl FnOnce() -> String,
    ) -> Result<(), SketchError> {
        if self.has_text_part(curves) {
            return Err(SketchError::InvalidInput(message()));
        }
        Ok(())
    }

    /// Lehnt das Ändern eines erzeugten Punktes ab (Lage setzen): Ihn hält der Text.
    pub(crate) fn refuse_glyph_point(&self, p: PointId) -> Result<(), SketchError> {
        if !self.texts.is_empty() && self.is_text_point(p) {
            return Err(SketchError::InvalidInput(tr!("sketch-text-no-edit")));
        }
        Ok(())
    }

    /// Rahmenlinien bleiben Konstruktion: weder normal noch Mittellinie (Ruling zu Spec B4).
    /// `curves` ist die ganze Stilgruppe: Eine Rahmenlinie als Vorlage eines Musters darf sich
    /// nicht über ein Exemplar mit umschalten, sonst lehnte das Laden die Datei ab.
    pub(crate) fn refuse_frame_styles(&self, curves: &[CurveId]) -> Result<(), SketchError> {
        if !self.texts.is_empty() && curves.iter().any(|&c| self.text_of_frame(c).is_some()) {
            Err(SketchError::InvalidInput(tr!("sketch-text-part")))
        } else {
            Ok(())
        }
    }

    /// Die Kurven, die den Stil der erzeugten Kurve `c` teilen: alle Kurven ihres Textes (Spec
    /// B10). `None`, wenn `c` keine erzeugte Kurve ist.
    pub(crate) fn text_style_group(&self, c: CurveId) -> Option<Vec<CurveId>> {
        let t = self.text_of_curve(c)?;
        Some(self.texts[t].curves.clone())
    }

    /// Die Konturen des Textes `t` mit dem Umriss `shape` an seiner aktuellen Lage, in
    /// Skizzenkoordinaten. `None`, wenn die Lage gerade nicht geht (Ecke gelöscht, Grundlinie
    /// der Länge null, Pfad ohne Länge oder Richtung, Punkte außerhalb des Rechenbereichs): Dann
    /// bleibt die letzte gute Lage (Review Focus 3).
    pub(crate) fn placed_contours(&self, t: TextId, shape: &TextShape) -> Option<Vec<Contour>> {
        self.placed_contours_walk(t, shape).0
    }

    /// Wie [`Sketch::placed_contours`], mit der gemerkten Längentabelle des Pfades, wenn sie
    /// zum Pfad passt. Liefert dazu die benutzte Tabelle, damit der Aufrufer sie merkt (`None`
    /// im Rahmen und wenn der Pfad gerade keine Länge hat).
    pub(crate) fn placed_contours_walk(
        &self,
        t: TextId,
        shape: &TextShape,
    ) -> (Option<Vec<Contour>>, Option<Arc<Walk>>) {
        let d = &self.texts[t];
        self.place_text_walk(
            d.placement,
            d.height,
            (d.flip_h, d.flip_v),
            shape,
            d.walk.0.as_ref(),
            d.normal,
        )
    }

    /// Die Ebene des Textes `t` in 3D an der aktuellen Lage seines Rahmens; `None` für einen
    /// Text in der Skizzenebene, auf einem Pfad oder solange der Rahmen entartet ist.
    pub(crate) fn text_lift(&self, t: TextId) -> Option<Lift> {
        let d = &self.texts[t];
        let n = d.normal?;
        let TextPlacement::Box { corners, .. } = d.placement else {
            return None;
        };
        if corners.iter().any(|&p| !self.points[p].alive) {
            return None;
        }
        Lift::of_frame(corners.map(|p| self.pos3(p)), n, self.tol.linear)
    }

    /// Die Konturen eines Textes auf dem Pfad `path` mit den Werten `params`, gesetzt als `shape`,
    /// ohne etwas in die Skizze zu schreiben: die Vorschau des Werkzeugs (Spec C6), dieselbe
    /// Rechnung wie [`Sketch::add_text_path`]. `None`, wenn die Kurve keinen Text trägt
    /// (Buchstaben, Rahmenlinien, Griffe, unbekannt) oder die Lage gerade nicht geht (siehe
    /// [`Sketch::placed_contours`]).
    pub fn text_path_preview(
        &self,
        path: CurveId,
        above: bool,
        fit: bool,
        h: HAlign,
        params: &TextParams,
        shape: &TextShape,
    ) -> Option<Vec<Contour>> {
        self.refuse_bad_path(path).ok()?;
        let placement = TextPlacement::Path {
            path,
            above,
            fit,
            h,
        };
        self.place_text(
            placement,
            params.height,
            params.flip_h,
            params.flip_v,
            shape,
        )
    }

    /// Die Konturen eines Textes mit `placement`, Höhe und Umkehren (siehe
    /// [`Sketch::placed_contours`]).
    fn place_text(
        &self,
        placement: TextPlacement,
        height: f64,
        flip_h: bool,
        flip_v: bool,
        shape: &TextShape,
    ) -> Option<Vec<Contour>> {
        self.place_text_walk(placement, height, (flip_h, flip_v), shape, None, None)
            .0
    }

    /// [`Sketch::place_text`] mit einer gemerkten Längentabelle `walk` des Pfades: Passt sie zum
    /// Pfad, wird sie benutzt, sonst neu gebaut. Liefert die benutzte Tabelle mit.
    ///
    /// `three_d`: die Normale eines Textes in 3D (Skizzen 10). Dann sind die Konturen in der
    /// Basis seiner Ebene
    /// ([`Sketch::text_lift`]) gesetzt, nicht in Skizzenkoordinaten; geprüft ist auch ihre Lage im
    /// Raum.
    fn place_text_walk(
        &self,
        placement: TextPlacement,
        height: f64,
        (flip_h, flip_v): (bool, bool),
        shape: &TextShape,
        walk: Option<&Arc<Walk>>,
        three_d: Option<DVec3>,
    ) -> (Option<Vec<Contour>>, Option<Arc<Walk>>) {
        let mut used = None;
        let placed = match placement {
            TextPlacement::Box {
                corners, h, v: va, ..
            } => {
                if corners.iter().any(|&p| !self.points[p].alive) {
                    return (None, None);
                }
                if let Some(n) = three_d {
                    let at = corners.map(|p| self.pos3(p));
                    let Some(lift) = Lift::of_frame(at, n, self.tol.linear) else {
                        return (None, None);
                    };
                    if !at.iter().all(|&p| within_range3(p)) {
                        return (None, None);
                    }
                    let local = at.map(|p| lift.to2(p));
                    let placed = place_box(shape, height, local, h, va, flip_h, flip_v);
                    let ok = placed_ok(&placed)
                        && contour_points(&placed).all(|q| within_range3(lift.to3(q)));
                    return (ok.then_some(placed), None);
                }
                let at = corners.map(|p| self.pos(p));
                if !at.iter().all(|&p| within_coord_range(p))
                    || at[0].distance(at[1]) <= self.tol.linear
                {
                    return (None, None);
                }
                Some(place_box(shape, height, at, h, va, flip_h, flip_v))
            }
            TextPlacement::Path {
                path,
                above,
                fit,
                h,
            } => {
                let Some((geom, closed, clockwise)) = self.text_path_geom(path) else {
                    return (None, None);
                };
                let walk = match walk {
                    Some(w) if w.fits(&geom, closed, clockwise) => Some(w.clone()),
                    _ => Walk::new(geom, closed, clockwise).map(Arc::new),
                };
                let Some(walk) = walk else {
                    return (None, None);
                };
                let placed = place_path(shape, height, &walk, above, fit, h, flip_h, flip_v);
                used = Some(walk);
                placed
            }
        };
        (placed.filter(|p| placed_ok(p)), used)
    }

    /// Der Pfad `path` als Geometrie, ob er geschlossen ist (Kreis, Ellipse, Kurve mit gleichen
    /// Enden) und ob er im Uhrzeigersinn gelesen wird (Kreis, Bogen, Ellipse, Ellipsenbogen;
    /// Spec B7, Ruling 5b). `None`, solange er entartet ist: gelöscht, ohne Radius, nicht endlich
    /// oder nicht größer als die Toleranz.
    fn text_path_geom(&self, path: CurveId) -> Option<(Geom, bool, bool)> {
        let kind = &self.curve(path)?.kind;
        let geom = self.geom(path)?;
        let (lo, hi) = geom.bbox();
        if !(lo.is_finite() && hi.is_finite()) || lo.distance(hi) <= self.tol.linear {
            return None;
        }
        let closed = geom.periodic()
            || curve_ends(kind).is_some_and(|[a, b]| {
                a == b || self.pos(a).distance(self.pos(b)) <= self.tol.linear
            });
        let clockwise = matches!(
            kind,
            CurveKind::Circle { .. }
                | CurveKind::Arc { .. }
                | CurveKind::Ellipse { .. }
                | CurveKind::EllipticArc { .. }
        );
        Some((geom, closed, clockwise))
    }

    /// Legt einen Text im Rahmen mit den gegenüberliegenden Ecken `a` und `b` an (Spec B4). Der
    /// Rahmen ist achsenparallel mit Ecke 0 unten links, Linie 0 → 1 ist die Grundlinie. Er
    /// besteht aus vier Konstruktionslinien mit den internen Abhängigkeiten eines Rechtecks über
    /// drei Punkte (1 × `Perpendicular`, 2 × `Parallel`, 5 Freiheitsgrade) und `Horizontal` auf
    /// der Grundlinie (**Annahme**: In Fusion ist der Rahmen nach dem Aufziehen waagerecht).
    /// `shape` ist der mit `params` gesetzte Text. Bei einem Fehler bleibt die Skizze
    /// unverändert.
    pub fn add_text_box(
        &mut self,
        a: DVec2,
        b: DVec2,
        params: TextParams,
        h: HAlign,
        v: VAlign,
        shape: Arc<TextShape>,
    ) -> Result<TextId, SketchError> {
        if !(a.is_finite() && b.is_finite()) {
            return Err(SketchError::InvalidInput(tr!("sketch-err-corners-finite")));
        }
        let (min, max) = (a.min(b), a.max(b));
        let size = max - min;
        if size.x <= self.tol.linear || size.y <= self.tol.linear {
            return Err(SketchError::Degenerate(tr!("sketch-err-rect-size")));
        }
        check_params(&params)?;
        check_new_shape(&shape)?;
        let corners = [min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)];
        let placed = place_box(
            &shape,
            params.height,
            corners,
            h,
            v,
            params.flip_h,
            params.flip_v,
        );
        if !placed_ok(&placed) {
            return Err(out_of_range());
        }
        self.atomically(|s| {
            let (points, lines) = s.push_frame(corners)?;
            let t = s.texts.push(TextData {
                content: params.content,
                style: params.style,
                height: params.height,
                flip_h: params.flip_h,
                flip_v: params.flip_v,
                placement: TextPlacement::Box {
                    lines,
                    corners: points,
                    h,
                    v,
                },
                normal: None,
                curves: Vec::new(),
                points: Vec::new(),
                alive: true,
                shown: None,
                shaped: ShapedCache::set(shape),
                walk: WalkCache::default(),
            });
            s.touch_structure();
            s.sync_text(t, &placed);
            Ok(t)
        })
    }

    /// Legt einen Text in 3D an (Skizzen 10, Spec Entwurf 4): im Rahmen mit den gegenüberliegenden
    /// Ecken `corners` in der Ebene durch `corners[0]` mit der Normale `normal`; `corners[1]` wird
    /// senkrecht in diese Ebene gelegt. Der Rahmen ist achsenparallel zur Basis
    /// [`plane_basis`]`(n)` der Ebene: Ecke 0 unten links, Linie 0 → 1 die Grundlinie entlang `u`,
    /// oben ist `v = n × u`. Er besteht aus vier 3D-Ecken und vier Konstruktionslinien mit den
    /// internen Abhängigkeiten eines Rechtecks (1 × `Perpendicular`, 2 × `Parallel`); liegen
    /// Grundlinie oder Seite auf einer Achse der Skizze, hält sie dazu `Horizontal`, `Vertical`
    /// oder `AlongZ` wie beim 2D-Rahmen das `Horizontal`. Die Buchstaben werden im 2D-Satz gesetzt
    /// und in die Ebene gelegt (3D-Punkte); sie ergeben kein Profil.
    ///
    /// Ist die Normale die Skizzennormale und liegen beide Ecken in der Skizzenebene, ist es ein
    /// gewöhnlicher Text wie [`Sketch::add_text_box`]. Die Normale muss endlich und normierbar
    /// sein. Bei einem Fehler bleibt die Skizze unverändert.
    pub fn add_text_box3(
        &mut self,
        corners: [DVec3; 2],
        normal: DVec3,
        params: TextParams,
        h: HAlign,
        v: VAlign,
        shape: Arc<TextShape>,
    ) -> Result<TextId, SketchError> {
        let [a, b] = corners;
        if !(a.is_finite() && b.is_finite()) {
            return Err(SketchError::InvalidInput(tr!("sketch-err-corners-finite")));
        }
        let n = unit_normal(normal)?;
        let (lin, ang) = (self.tol.linear, self.tol.angular);
        let sketch_plane = n.z > 0.0 && n.x.abs() <= ang && n.y.abs() <= ang;
        if sketch_plane && a.z.abs() <= lin && b.z.abs() <= lin {
            return self.add_text_box(a.truncate(), b.truncate(), params, h, v, shape);
        }
        let n = if sketch_plane { DVec3::Z } else { n };
        let (u, w) = plane_basis(n);
        let flat = |p: DVec3| DVec2::new(p.dot(u), p.dot(w));
        let (qa, qb) = (flat(a), flat(b));
        let (min, max) = (qa.min(qb), qa.max(qb));
        let size = max - min;
        if size.x <= lin || size.y <= lin {
            return Err(SketchError::Degenerate(tr!("sketch-err-rect-size")));
        }
        check_params(&params)?;
        check_new_shape(&shape)?;
        let off = n * a.dot(n);
        let lift = |q: DVec2| off + u * q.x + w * q.y;
        let at = [min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)].map(lift);
        if !at.iter().all(|&p| within_range3(p)) {
            return Err(out_of_range());
        }
        self.atomically(|s| {
            let (points, lines) = s.push_frame3(at, u)?;
            let t = s.texts.push(TextData {
                content: params.content,
                style: params.style,
                height: params.height,
                flip_h: params.flip_h,
                flip_v: params.flip_v,
                placement: TextPlacement::Box {
                    lines,
                    corners: points,
                    h,
                    v,
                },
                normal: Some(n),
                curves: Vec::new(),
                points: Vec::new(),
                alive: true,
                shown: None,
                shaped: ShapedCache::set(shape.clone()),
                walk: WalkCache::default(),
            });
            // Die Ecken bleiben in der Textebene (Spec 1: der Löser dreht keine Ebene).
            for &line in &lines[..3] {
                s.push_internal(ConstraintKind::InPlane(line));
            }
            s.touch_structure();
            let placed = s.placed_contours(t, &shape).ok_or_else(out_of_range)?;
            s.sync_text(t, &placed);
            Ok(t)
        })
    }

    /// Legt einen Text auf dem Pfad `path` an (Spec B7): jede lebende Kurve außer erzeugten
    /// Kurven, Rahmenlinien eines Textes und Griffen, auch Konstruktion und projizierte. `above`
    /// heißt oberhalb (links der Leserichtung), `fit` „An Pfad anpassen“. `shape` ist der mit
    /// `params` gesetzte Text, die Zeilen mit [`path_content`] verbunden. Bei einem Fehler bleibt
    /// die Skizze unverändert.
    ///
    /// Eine Rahmenlinie ist kein Pfad (Ruling 5c, ebenso in `from_data`): Sie gehört zu ihrem
    /// Text und ginge mit ihm verloren (Löschen, Auflösen, Wechsel auf Pfad).
    pub fn add_text_path(
        &mut self,
        path: CurveId,
        params: TextParams,
        above: bool,
        fit: bool,
        h: HAlign,
        shape: Arc<TextShape>,
    ) -> Result<TextId, SketchError> {
        self.refuse_bad_path(path)?;
        check_params(&params)?;
        check_new_shape(&shape)?;
        self.refuse_degenerate_path(path)?;
        let placement = TextPlacement::Path {
            path,
            above,
            fit,
            h,
        };
        let placed = self
            .place_text(
                placement,
                params.height,
                params.flip_h,
                params.flip_v,
                &shape,
            )
            .ok_or_else(out_of_range)?;
        let t = self.texts.push(TextData {
            content: params.content,
            style: params.style,
            height: params.height,
            flip_h: params.flip_h,
            flip_v: params.flip_v,
            placement,
            normal: None,
            curves: Vec::new(),
            points: Vec::new(),
            alive: true,
            shown: None,
            shaped: ShapedCache::set(shape),
            walk: WalkCache::default(),
        });
        self.touch_structure();
        self.sync_text(t, &placed);
        Ok(t)
    }

    /// Ändert Werte und Ausrichtung eines Textes im Rahmen; `shape` ist der mit `params` gesetzte
    /// Text. Die Kurven werden der Reihe nach wiederverwendet (Spec B3): Ändern sich nur Höhe,
    /// Abstand, Ausrichtung oder Umkehren, behalten alle Kurven ihre Ids. Der Rahmen bleibt, außer
    /// der Text passt nicht mehr hinein: dann wächst er mit (Korrektur K2c, nie kleiner). Bei
    /// einem Fehler bleibt die Skizze unverändert.
    pub fn edit_text_box(
        &mut self,
        t: TextId,
        params: TextParams,
        h: HAlign,
        v: VAlign,
        shape: Arc<TextShape>,
    ) -> Result<(), SketchError> {
        let d = self.text(t).ok_or(SketchError::UnknownObject)?;
        let TextPlacement::Box { lines, corners, .. } = d.placement else {
            return Err(SketchError::WrongType(tr!("sketch-text-not-box")));
        };
        check_params(&params)?;
        check_new_shape(&shape)?;
        let before = d.clone();
        let mut next = before.clone();
        next.content = params.content;
        next.style = params.style;
        next.height = params.height;
        next.flip_h = params.flip_h;
        next.flip_v = params.flip_v;
        next.placement = TextPlacement::Box {
            lines,
            corners,
            h,
            v,
        };
        next.shaped = ShapedCache::set(shape.clone());
        self.texts[t] = next;
        let Some(placed) = self.placed_contours(t, &shape) else {
            self.texts[t] = before;
            return Err(out_of_range());
        };
        if self.texts[t] != before {
            self.touch_structure();
        }
        self.sync_text(t, &placed);
        self.grow_frame_to_fit(t);
        Ok(())
    }

    /// Ändert einen Text (Spec C8): Werte, Art und Lage. `shape` ist der mit `params` gesetzte
    /// Text, für einen Pfad mit verbundenen Zeilen ([`path_content`]). Die Kurven werden der Reihe
    /// nach wiederverwendet (Spec B3), auch beim Wechsel der Art.
    ///
    /// - Rahmen → Rahmen wie [`Sketch::edit_text_box`].
    /// - Rahmen → Pfad löscht den Rahmen und die Abhängigkeiten daran. Eine Ecke, die noch eine
    ///   andere lebende Kurve benutzt, bleibt als gewöhnlicher Punkt (wie bei
    ///   [`Sketch::remove_text`]).
    /// - Pfad → Rahmen legt einen Rahmen an, der den Text an seiner Stelle umfasst (Spec C8),
    ///   mit Winkel 0 und dem internen `Horizontal` wie ein neuer Rahmen. Groß ist er nach den
    ///   Maßen der Schrift: so breit wie die breiteste Zeile, so hoch wie von der Oberlänge der
    ///   ersten bis zur Unterlänge der letzten Zeile. Damit setzt jede Ausrichtung den Textblock
    ///   gleich. Er steht so, dass die Mitte des neu gesetzten Textes auf der Mitte des Textes an
    ///   seiner bisherigen Lage liegt; auf einem waagerechten geraden Pfad bleibt jeder Punkt, wo
    ///   er war. Der Pfad bleibt.
    /// - Pfad → Pfad, auch auf eine andere Kurve, mit denselben Prüfungen wie
    ///   [`Sketch::add_text_path`].
    ///
    /// Als Ganzes atomar: Bei einem Fehler bleibt die Skizze unverändert, auch ihr Stand.
    pub fn edit_text(
        &mut self,
        t: TextId,
        params: TextParams,
        kind: TextKindEdit,
        shape: Arc<TextShape>,
    ) -> Result<(), SketchError> {
        let d = self.text(t).ok_or(SketchError::UnknownObject)?;
        let is_box = matches!(d.placement, TextPlacement::Box { .. });
        match kind {
            TextKindEdit::Box { h, v } if is_box => self.edit_text_box(t, params, h, v, shape),
            TextKindEdit::Box { h, v } => self.edit_path_to_box(t, params, h, v, shape),
            TextKindEdit::Path {
                path,
                above,
                fit,
                h,
            } => {
                let placement = TextPlacement::Path {
                    path,
                    above,
                    fit,
                    h,
                };
                self.edit_text_to_path(t, params, path, placement, shape)
            }
        }
    }

    /// Rahmen → Pfad oder Pfad → Pfad (siehe [`Sketch::edit_text`]); `placement` ist die neue
    /// Lage auf dem Pfad `path`.
    fn edit_text_to_path(
        &mut self,
        t: TextId,
        params: TextParams,
        path: CurveId,
        placement: TextPlacement,
        shape: Arc<TextShape>,
    ) -> Result<(), SketchError> {
        // Ein Text in 3D bleibt im Rahmen.
        if self.texts[t].normal.is_some() {
            return Err(path_3d());
        }
        self.refuse_bad_path(path)?;
        check_params(&params)?;
        check_new_shape(&shape)?;
        self.refuse_degenerate_path(path)?;
        self.atomically(|s| {
            let before = s.texts[t].clone();
            if let TextPlacement::Box { lines, corners, .. } = before.placement {
                s.drop_text_parts(&lines.into_iter().collect(), corners.to_vec());
                s.touch_structure();
            }
            let placed = s
                .place_text(
                    placement,
                    params.height,
                    params.flip_h,
                    params.flip_v,
                    &shape,
                )
                .ok_or_else(out_of_range)?;
            s.set_text_values(t, params, placement, shape);
            if s.texts[t] != before {
                s.touch_structure();
            }
            s.sync_text(t, &placed);
            Ok(())
        })
    }

    /// Pfad → Rahmen (siehe [`Sketch::edit_text`]).
    fn edit_path_to_box(
        &mut self,
        t: TextId,
        params: TextParams,
        h: HAlign,
        v: VAlign,
        shape: Arc<TextShape>,
    ) -> Result<(), SketchError> {
        check_params(&params)?;
        check_new_shape(&shape)?;
        let rect_size = || SketchError::Degenerate(tr!("sketch-err-rect-size"));
        // Die Mitte des Textes an seiner aktuellen Lage.
        let d = &self.texts[t];
        let now = mid_of(
            d.points
                .iter()
                .filter(|&&p| self.points[p].alive)
                .map(|&p| self.pos(p)),
        )
        .ok_or_else(rect_size)?;
        // Der Rahmen nach den Maßen der Schrift: Er umfasst den Textblock genau.
        let height = params.height;
        let width = shape.lines.iter().map(|l| l.width).fold(0.0, f64::max) * height;
        let below = shape.lines.len().saturating_sub(1) as f64 * shape.line_height;
        let tall = (shape.ascender - shape.descender + below) * height;
        if !within_coord_range(DVec2::new(width, tall)) {
            return Err(out_of_range());
        }
        if width <= self.tol.linear || tall <= self.tol.linear {
            return Err(rect_size());
        }
        let frame_at = |o: DVec2| {
            [
                o,
                o + DVec2::new(width, 0.0),
                o + DVec2::new(width, tall),
                o + DVec2::new(0.0, tall),
            ]
        };
        let probe = place_box(
            &shape,
            height,
            frame_at(DVec2::ZERO),
            h,
            v,
            params.flip_h,
            params.flip_v,
        );
        let mid = mid_of(contour_points(&probe)).ok_or_else(out_of_range)?;
        let corners = frame_at(now - mid);
        if !corners.iter().all(|&c| within_coord_range(c)) {
            return Err(out_of_range());
        }
        let placed = place_box(
            &shape,
            params.height,
            corners,
            h,
            v,
            params.flip_h,
            params.flip_v,
        );
        if !placed_ok(&placed) {
            return Err(out_of_range());
        }
        self.atomically(|s| {
            let (points, lines) = s.push_frame(corners)?;
            let placement = TextPlacement::Box {
                lines,
                corners: points,
                h,
                v,
            };
            s.set_text_values(t, params, placement, shape);
            s.touch_structure();
            s.sync_text(t, &placed);
            Ok(())
        })
    }

    /// Schreibt Werte, Lage und gesetzte Form in den Text `t`; Kurven und Punkte bleiben.
    fn set_text_values(
        &mut self,
        t: TextId,
        params: TextParams,
        placement: TextPlacement,
        shape: Arc<TextShape>,
    ) {
        let d = &mut self.texts[t];
        d.content = params.content;
        d.style = params.style;
        d.height = params.height;
        d.flip_h = params.flip_h;
        d.flip_v = params.flip_v;
        d.placement = placement;
        d.shaped = ShapedCache::set(shape);
    }

    /// Setzt die Höhe des Textes `t` auf `h` mm (Skizzen 11b, Texthöhe als Formel): wie die Höhe in
    /// [`Sketch::edit_text_box`] und [`Sketch::edit_text`] werden die Kurven der Reihe nach
    /// wiederverwendet; passt der Text nicht mehr in den Rahmen, wächst dieser mit (Korrektur
    /// K2c, nie kleiner). Eine Höhe, die nicht endlich oder nicht größer als die Toleranz ist, lehnt die Skizze wie die Prüfung der Werte ab; ein gleicher Wert ändert nichts,
    /// auch nicht den Stand. Ein Text, der noch nicht gesetzt ist (nach dem Laden), bekommt nur den
    /// Wert; [`Sketch::reshape_texts`] setzt ihn danach. Bei einem Fehler bleibt die Skizze
    /// unverändert.
    pub fn set_text_height(&mut self, t: TextId, h: f64) -> Result<(), SketchError> {
        let d = self.text(t).ok_or(SketchError::UnknownObject)?;
        if !(h.is_finite() && h > self.tol.linear) {
            return Err(SketchError::InvalidInput(tr!("sketch-text-height")));
        }
        if d.height == h {
            return Ok(());
        }
        let before = d.clone();
        let shape = before.shaped.shape.clone();
        self.texts[t].height = h;
        let Some(shape) = shape else {
            self.touch_structure();
            return Ok(());
        };
        let Some(placed) = self.placed_contours(t, &shape) else {
            self.texts[t] = before;
            return Err(out_of_range());
        };
        self.touch_structure();
        self.sync_text(t, &placed);
        self.grow_frame_to_fit(t);
        Ok(())
    }

    /// Setzt den gezeigten Inhalt des Textes `t` ([`TextData::shown`], Skizzen 11b). Nur zur
    /// Laufzeit: Stand und Kurven bleiben; [`Sketch::reshape_texts`] setzt danach neu. `true`,
    /// wenn er sich geändert hat; ein unbekannter Text ändert nichts.
    pub fn set_text_shown(&mut self, t: TextId, shown: Option<String>) -> bool {
        if self.text(t).is_none_or(|d| d.shown == shown) {
            return false;
        }
        self.texts[t].shown = shown;
        true
    }

    /// Setzt nur den Rohinhalt des Textes `t` (Skizzen 11b: Umbenennen eines Parameters schreibt
    /// `{alt}` → `{neu}`). Form, Kurven und gezeigter Inhalt bleiben; der Stand der Skizze ändert
    /// sich (der Inhalt steht in der Datei). `true`, wenn er sich geändert hat.
    pub fn set_text_content(&mut self, t: TextId, content: String) -> bool {
        if self.text(t).is_none_or(|d| d.content == content) {
            return false;
        }
        self.texts[t].content = content;
        self.touch_structure();
        true
    }

    /// Der Winkel eines Textes im Rahmen (Spec C5): die Richtung der Grundlinie von Ecke 0 nach
    /// Ecke 1 im Bogenmaß, in `(−π, π]`. `None` für Text auf Pfad, einen unbekannten Text und
    /// solange die Grundlinie keine Länge hat.
    pub fn text_angle(&self, t: TextId) -> Option<f64> {
        let TextPlacement::Box { corners, .. } = self.text(t)?.placement else {
            return None;
        };
        if corners[..2].iter().any(|&p| !self.points[p].alive) {
            return None;
        }
        // In 3D: gegen `u` der Basis [`plane_basis`] der Ebene, um ihre Normale gemessen.
        if self.texts[t].normal.is_some() {
            let lift = self.text_lift(t)?;
            let (bu, bv) = plane_basis(lift.n);
            return Some(lift.u.dot(bv).atan2(lift.u.dot(bu)));
        }
        let d = self.pos(corners[1]) - self.pos(corners[0]);
        (d.is_finite() && d.length() > self.tol.linear).then(|| d.y.atan2(d.x))
    }

    /// Dreht den Rahmen des Textes `t` um Ecke 0 auf den Winkel `rad` (Spec C5). Das interne
    /// `Horizontal` auf der Grundlinie fällt dabei weg. Die übrige Geometrie der Gruppe des
    /// Rahmens wird bei fest stehenden Ecken nachgelöst; geht das nicht, hält eine Abhängigkeit des
    /// Benutzers den Winkel (etwa ein `Horizontal`, zwei feste Ecken oder ein Winkelmaß zu fester
    /// Geometrie), und die Drehung wird mit `sketch-text-angle-held` abgelehnt. Andere Gruppen der
    /// Skizze zählen nicht, auch wenn sie unlösbar sind. Ein gleicher Winkel ändert nichts.
    /// Bei einem Fehler bleibt die Skizze unverändert.
    pub fn set_text_angle(&mut self, t: TextId, rad: f64) -> Result<(), SketchError> {
        let d = self.text(t).ok_or(SketchError::UnknownObject)?;
        let TextPlacement::Box { lines, corners, .. } = d.placement else {
            return Err(SketchError::WrongType(tr!("sketch-text-not-box")));
        };
        if !rad.is_finite() {
            return Err(SketchError::InvalidInput(tr!(
                "sketch-err-not-finite",
                what = tr!("sketch-text-what-angle")
            )));
        }
        let now = self
            .text_angle(t)
            .ok_or_else(|| SketchError::Degenerate(tr!("sketch-err-rect-size")))?;
        // Der Unterschied in (−π, π]: Ein Winkel ist eine Richtung.
        let delta = (rad - now).sin().atan2((rad - now).cos());
        if delta.abs() <= self.tol.angular {
            return Ok(());
        }
        let (sin, cos) = delta.sin_cos();
        // In 3D um die Normale der Ebene durch Ecke 0; die Ebene bleibt.
        let lift = self.text_lift(t);
        let c0 = self.pos3(corners[0]);
        let targets = corners.map(|p| match lift {
            Some(l) => c0 + rotate_about(self.pos3(p) - c0, l.n, sin, cos),
            None => {
                let r = self.pos3(p) - c0;
                c0 + DVec3::new(r.x * cos - r.y * sin, r.x * sin + r.y * cos, 0.0)
            }
        });
        if !targets.iter().all(|&p| within_range3(p)) {
            return Err(out_of_range());
        }
        let level: Vec<_> = self
            .constraints
            .iter()
            .filter(|(_, k)| k.alive && k.internal && is_frame_axis(&k.kind, &lines))
            .map(|(i, _)| i)
            .collect();
        // Ohne Kopie (offener Punkt C12): Bis zum Lösen ändern sich nur Werte und die Lebendigkeit
        // der Rahmenachsen; gescheitert (auch nach einer Panik) wird beides zurückgenommen.
        let snap = self.value_snapshot();
        for &k in &level {
            self.constraints[k].alive = false;
        }
        self.touch_structure();
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Ecke 0 steht schon; die anderen genau auf die gedrehte Lage.
            for (&p, at) in corners.iter().zip(targets).skip(1) {
                if lift.is_some() {
                    self.set_point_xyz(p, at)?;
                } else {
                    self.set_point_xy(p, at.x, at.y)?;
                }
            }
            if self.solve_holding_connected(&corners) {
                Ok(())
            } else {
                Err(SketchError::InvalidInput(tr!("sketch-text-angle-held")))
            }
        }));
        if matches!(run, Ok(Ok(()))) {
            self.refresh_texts();
            return Ok(());
        }
        for &k in &level {
            self.constraints[k].alive = true;
        }
        self.restore_values(snap);
        match run {
            Ok(r) => r,
            Err(p) => std::panic::resume_unwind(p),
        }
    }

    /// Legt den Rahmen mit den Ecken `corners` an (Spec B4): vier Konstruktionslinien, Linie `i`
    /// von Ecke `i` nach Ecke `i + 1`, mit den internen Abhängigkeiten eines Rechtecks über drei
    /// Punkte (1 × `Perpendicular`, 2 × `Parallel`) und `Horizontal` auf der Grundlinie. Nur
    /// innerhalb von `atomically`: Ein Fehler hinterließe sonst einen halben Rahmen.
    fn push_frame(
        &mut self,
        corners: [DVec2; 4],
    ) -> Result<([PointId; 4], [CurveId; 4]), SketchError> {
        let mut points = [Id::from_index(0); 4];
        for (slot, c) in points.iter_mut().zip(corners) {
            *slot = self.add_point(c.x, c.y)?;
        }
        let mut lines = [Id::from_index(0); 4];
        for (i, slot) in lines.iter_mut().enumerate() {
            *slot = self.add_line(points[i], points[(i + 1) % 4])?;
            self.curves[*slot].construction = true;
        }
        self.push_internal(ConstraintKind::Perpendicular(lines[0], lines[1]));
        self.push_internal(ConstraintKind::Parallel(lines[0], lines[2]));
        self.push_internal(ConstraintKind::Parallel(lines[1], lines[3]));
        self.push_internal(ConstraintKind::Horizontal(LineOrPoints::Line(lines[0])));
        Ok((points, lines))
    }

    /// Wie [`Sketch::push_frame`] für einen Text in 3D (siehe [`Sketch::add_text_box3`]): vier
    /// 3D-Ecken; statt `Horizontal` bindet `Horizontal`, `Vertical` oder `AlongZ` die Grundlinie
    /// (Richtung `u`), wenn sie auf einer Achse der Skizze liegt. Die Ebene halten die internen
    /// `InPlane` an den Linien 0 bis 2, die der Aufrufer nach dem Text anlegt (sie lesen dessen
    /// Normale). Nur innerhalb von `atomically`.
    fn push_frame3(
        &mut self,
        corners: [DVec3; 4],
        u: DVec3,
    ) -> Result<([PointId; 4], [CurveId; 4]), SketchError> {
        let mut points = [Id::from_index(0); 4];
        for (slot, c) in points.iter_mut().zip(corners) {
            *slot = self.add_point3(c)?;
        }
        let mut lines = [Id::from_index(0); 4];
        for (i, slot) in lines.iter_mut().enumerate() {
            *slot = self.add_line(points[i], points[(i + 1) % 4])?;
            self.curves[*slot].construction = true;
        }
        self.push_internal(ConstraintKind::Perpendicular(lines[0], lines[1]));
        self.push_internal(ConstraintKind::Parallel(lines[0], lines[2]));
        self.push_internal(ConstraintKind::Parallel(lines[1], lines[3]));
        if let Some(k) = axis_binding(u, lines[0]) {
            self.push_internal(k);
        }
        Ok((points, lines))
    }

    /// Unbekannte Kurve, oder eine, die keinen Text trägt (Spec B7): erzeugte Kurven, Rahmenlinien
    /// eines Textes und Griffe.
    fn refuse_bad_path(&self, path: CurveId) -> Result<(), SketchError> {
        if self.curve(path).is_none() {
            return Err(SketchError::UnknownObject);
        }
        if self.text_of_curve(path).is_some()
            || self.text_of_frame(path).is_some()
            || self.is_handle_curve(path)
        {
            return Err(SketchError::InvalidInput(tr!("sketch-text-bad-path")));
        }
        if self.is_3d_curve(path) {
            return Err(path_3d());
        }
        Ok(())
    }

    /// Ein Pfad ohne Länge trägt keinen Text.
    fn refuse_degenerate_path(&self, path: CurveId) -> Result<(), SketchError> {
        if self.text_path_geom(path).is_none() {
            return Err(SketchError::Degenerate(tr!("sketch-text-path-degenerate")));
        }
        Ok(())
    }

    /// Löscht den Text als Ganzes (Spec B8): Rahmen, erzeugte Kurven, die Abhängigkeiten daran
    /// (auch die des Benutzers) und die Punkte des Textes. Ein Punkt des Textes, den noch eine
    /// andere lebende Kurve benutzt (etwa eine Linie, gezeichnet mit Fang an einer Ecke), bleibt
    /// als gewöhnlicher Punkt; Abhängigkeiten zwischen ihm und anderer Geometrie bleiben, sofern
    /// sie keine Kurve des Textes benutzen. Ein Pfad bleibt. Muster an den gelöschten Teilen
    /// werden aufgelöst. Eine Rahmenlinie ist nie Pfad eines anderen Textes (`add_text_path` und
    /// `from_data` lehnen das ab), der Rahmen geht also ohne Rückfrage mit.
    pub fn remove_text(&mut self, t: TextId) -> Result<(), SketchError> {
        let d = self.text(t).ok_or(SketchError::UnknownObject)?;
        let mut curves: HashSet<CurveId> = d.curves.iter().copied().collect();
        let mut points: Vec<PointId> = d.points.clone();
        if let TextPlacement::Box { lines, corners, .. } = d.placement {
            curves.extend(lines);
            points.extend(corners);
        }
        self.drop_text_parts(&curves, points);
        self.texts[t].alive = false;
        self.touch_structure();
        Ok(())
    }

    /// Löst die Texte `ts` auf (Spec F), alles oder nichts: Die erzeugten Kurven bleiben mit
    /// ihren Ids und Punkten als gewöhnliche freie Kurven (Linien, Kegelschnitte, Splines mit
    /// Kontrollpunkten), damit Extrusionen an ihren Profilen bleiben. Jeder ihrer Punkte, auch
    /// Scheitel und Kontrollpunkte, bekommt wie in Fusion ein `Fixed` an seiner aktuellen Stelle,
    /// das sich einzeln lösen lässt. Rahmen, Ecken und die Abhängigkeiten daran gehen nach der
    /// Regel von [`Sketch::remove_text`] (eine Ecke an einer anderen Kurve bleibt als gewöhnlicher
    /// Punkt), ein Pfad bleibt. Das `TextData` wird gelöscht (es bleibt tot in der Arena, wie
    /// beim Löschen). Liefert die aufgelösten Kurven in der Reihenfolge von `ts`; ein doppelt
    /// genannter Text zählt einmal, eine leere Liste ändert nichts. Ein unbekannter oder
    /// gelöschter Text: `UnknownObject`, die Skizze bleibt unverändert.
    pub fn explode_texts(&mut self, ts: &[TextId]) -> Result<Vec<CurveId>, SketchError> {
        let mut ids: Vec<TextId> = Vec::with_capacity(ts.len());
        for &t in ts {
            self.text(t).ok_or(SketchError::UnknownObject)?;
            if !ids.contains(&t) {
                ids.push(t);
            }
        }
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.atomically(|s| {
            let mut out = Vec::new();
            for t in ids {
                let d = &s.texts[t];
                let curves = d.curves.clone();
                // Alle Punkte der Kurven, auch wenn `points` einen nicht nennen sollte.
                let mut points: Vec<PointId> = d.points.clone();
                let mut seen: HashSet<PointId> = points.iter().copied().collect();
                for &c in &curves {
                    for p in s.curves[c].kind.points() {
                        if seen.insert(p) {
                            points.push(p);
                        }
                    }
                }
                let frame = match d.placement {
                    TextPlacement::Box { lines, corners, .. } => Some((lines, corners)),
                    TextPlacement::Path { .. } => None,
                };
                // Zuerst tot: Danach sind Kurven und Punkte keine Textteile mehr, und `Fixed`
                // wird nicht als Abhängigkeit an Buchstaben abgelehnt.
                s.texts[t].alive = false;
                s.texts[t].shaped = ShapedCache::default();
                if let Some((lines, corners)) = frame {
                    s.drop_text_parts(&lines.into_iter().collect(), corners.to_vec());
                }
                for p in points {
                    if s.points[p].alive {
                        // Ein 3D-Punkt (Text in 3D) wird im Raum fest (Ruling R2).
                        let pin = if s.points[p].z.is_some() {
                            let q = s.pos3(p);
                            ConstraintKind::Fixed3(p, q.x, q.y, q.z)
                        } else {
                            let xy = s.pos(p);
                            ConstraintKind::Fixed(p, xy.x, xy.y)
                        };
                        s.add_constraint(pin)?;
                    }
                }
                out.extend(curves);
            }
            s.touch_structure();
            Ok(out)
        })
    }

    /// Löscht die Kurven `curves` und die Punkte `points` eines Textes samt allen Abhängigkeiten
    /// daran (auch internen und denen des Benutzers). Ein Punkt, den noch eine andere lebende
    /// Kurve benutzt, bleibt als gewöhnlicher Punkt, ebenso Abhängigkeiten zwischen ihm und
    /// anderer Geometrie, sofern sie keine der Kurven benutzen. Muster an den gelöschten Teilen
    /// werden aufgelöst.
    fn drop_text_parts(&mut self, curves: &HashSet<CurveId>, points: Vec<PointId>) {
        let used = self.points_used_outside(curves);
        let points: HashSet<PointId> = points.into_iter().filter(|p| !used.contains(p)).collect();
        for &c in curves {
            self.dissolve_patterns_of_curve(c);
        }
        for &p in &points {
            self.dissolve_patterns_of_point(p);
        }
        for &c in curves {
            self.curves[c].alive = false;
        }
        for &p in &points {
            self.points[p].alive = false;
        }
        let dead: Vec<_> = self
            .constraints
            .iter()
            .filter(|(_, k)| {
                k.alive
                    && (k.kind.curves().iter().any(|c| curves.contains(c))
                        || k.kind.points().iter().any(|p| points.contains(p)))
            })
            .map(|(i, _)| i)
            .collect();
        for k in dead {
            self.constraints[k].alive = false;
        }
    }

    /// Die Punkte aller lebenden Kurven außer `own`: Solche Punkte eines Textes bleiben beim
    /// Löschen seiner Teile als gewöhnliche Punkte stehen.
    pub(crate) fn points_used_outside(&self, own: &HashSet<CurveId>) -> HashSet<PointId> {
        self.curves()
            .filter(|(c, _)| !own.contains(c))
            .flat_map(|(_, d)| d.kind.points())
            .collect()
    }

    /// Schaltet alle erzeugten Kurven des Textes auf Konstruktion oder zurück (Spec B10); der
    /// Rahmen bleibt Konstruktion. Ein unbekannter Text wird übergangen.
    pub fn set_text_construction(&mut self, t: TextId, on: bool) {
        let Some(d) = self.text(t) else {
            return;
        };
        let curves = d.curves.clone();
        let mut changed = false;
        for c in curves {
            let cd = &mut self.curves[c];
            if cd.alive && (cd.construction != on || cd.centerline) {
                cd.construction = on;
                cd.centerline = false;
                changed = true;
            }
        }
        if changed {
            self.touch_structure();
        }
    }

    /// Setzt alle Texte mit `shaper` neu (beim Laden, wenn die Schriftliste da ist, und wenn sich
    /// Schriften ändern). Die Kurven werden nach Spec B3 wiederverwendet. Liefert dieselbe Form
    /// dieselben Kurven und Punkte, ändert sich nichts, auch nicht der Stand der Skizze (Review
    /// Focus 1). Auf einem Pfad wird mit verbundenen Zeilen gesetzt ([`path_content`]).
    ///
    /// Ein Text, den `shaper` nicht setzen kann (Fehler, mehr als [`MAX_TEXT_CURVES`] Kurven oder
    /// keine Kurve, obwohl der Text Kurven hat, etwa mit der Ersatzschrift), behält Kurven und Form
    /// und steht in [`ReshapeReport::unset`] (Anti-Ziel: eine fehlende Schrift lässt den Text nicht
    /// verschwinden).
    pub fn reshape_texts(&mut self, shaper: &dyn TextShaper) -> ReshapeReport {
        let mut report = ReshapeReport::default();
        let ids: Vec<TextId> = self.texts().map(|(t, _)| t).collect();
        for t in ids {
            let d = &self.texts[t];
            let shape = shaper
                .shape(&shaping_content(d), &d.style)
                .ok()
                .filter(|s| check_curve_limit(s).is_ok())
                .filter(|s| s.segment_count() > 0 || d.curves.is_empty());
            let Some(shape) = shape else {
                self.texts[t].shaped.unset = true;
                report.unset.push(t);
                continue;
            };
            self.texts[t].shaped.unset = false;
            let d = &self.texts[t];
            if let Some(name) = &shape.substituted {
                report.substituted.push((t, name.clone()));
            }
            if shape.synthetic {
                report.synthetic.push(t);
            }
            if shape.missing_chars {
                report.missing_chars.push(t);
            }
            if d.shaped.shape.as_deref() == Some(&shape) {
                continue;
            }
            // Nur ein schon gesetzter Text lässt den Rahmen wachsen (Korrektur K2c): Das erste
            // Setzen nach dem Laden ändert die Datei nicht.
            let had_shape = d.shaped.shape.is_some();
            let shape = Arc::new(shape);
            let placed = self.placed_contours(t, &shape);
            self.texts[t].shaped = ShapedCache::set(shape);
            if let Some(placed) = placed {
                report.changed |= self.sync_text(t, &placed);
                if had_shape {
                    report.changed |= self.grow_frame_to_fit(t);
                }
            }
        }
        report
    }

    /// Lässt den Rahmen des Textes `t` wachsen, bis er alle Punkte der Glyphen (samt
    /// Kontrollpunkten) umschließt (Skizzen 11b, Korrektur K2c, **Annahme**: Fusion nicht
    /// gegengeprüft). Er wächst nur, schrumpft nie. Die Ausrichtung (mit Umkehren) gibt die
    /// Richtung: Die Seite, an der der Text steht, bleibt, bei „Mitte“ bleibt die Mitte. So bleibt
    /// der Text, wo er ist; links/unten (die Vorgabe) bleibt Ecke 0 fest. Halten Abhängigkeiten
    /// oder Maße den Rahmen fest, bleibt er, wie er ist (der Text ragt hinaus, kein Fehler).
    /// Winkel, Ebene und die übrigen Seiten bleiben. `true`, wenn er gewachsen ist.
    pub(crate) fn grow_frame_to_fit(&mut self, t: TextId) -> bool {
        let Some(d) = self.text(t) else {
            return false;
        };
        let TextPlacement::Box { corners, h, v, .. } = d.placement else {
            return false;
        };
        if corners.iter().any(|&p| !self.points[p].alive) {
            return false;
        }
        let tol = self.tol.linear;
        // Basis des Rahmens: Ursprung Ecke 0, x entlang der Grundlinie, y links davon.
        let lift = self.text_lift(t);
        let c0 = self.pos3(corners[0]);
        let (u, n) = match (lift, d.normal) {
            (Some(l), _) => (l.u, l.v),
            (None, Some(_)) => return false,
            (None, None) => {
                let base = self.pos(corners[1]) - self.pos(corners[0]);
                let len = base.length();
                if !(len.is_finite() && len > tol) {
                    return false;
                }
                let u = base / len;
                (u.extend(0.0), u.perp().extend(0.0))
            }
        };
        let local = |q: DVec3| DVec2::new((q - c0).dot(u), (q - c0).dot(n));
        let width = local(self.pos3(corners[1])).x;
        let box_h = local(self.pos3(corners[3])).y;
        if !(width > tol && box_h > tol) {
            return false;
        }
        let mut ink = d.points.iter().map(|&p| local(self.pos3(p)));
        let Some(first) = ink.next() else {
            return false;
        };
        let (lo, hi) = ink.fold((first, first), |(lo, hi), q| (lo.min(q), hi.max(q)));
        if !(lo.is_finite() && hi.is_finite()) {
            return false;
        }
        // Woran der Text steht: Umkehren spiegelt an der Mitte des Rahmens (Spec B6).
        let x_side = match (h, d.flip_h) {
            (HAlign::Center, _) => 0,
            (HAlign::Left, false) | (HAlign::Right, true) => -1,
            _ => 1,
        };
        let y_side = match (v, d.flip_v) {
            (VAlign::Middle, _) => 0,
            (VAlign::Bottom, false) | (VAlign::Top, true) => -1,
            _ => 1,
        };
        let (x0, x1) = grow_span(width, lo.x, hi.x, x_side, tol);
        let (y0, y1) = grow_span(box_h, lo.y, hi.y, y_side, tol);
        if x0 == 0.0 && x1 == width && y0 == 0.0 && y1 == box_h {
            return false;
        }
        let at = |x: f64, y: f64| c0 + u * x + n * y;
        let targets = [at(x0, y0), at(x1, y0), at(x1, y1), at(x0, y1)];
        if !targets.iter().all(|&p| within_range3(p)) {
            return false;
        }
        // Kein Klon der ganzen Skizze (Skizzen 12, Task 12): Die Ecken werden in der Skizze selbst
        // gesetzt; scheitert das Setzen oder das Lösen (es ändert dann nichts), springen ihre
        // Werte und der Stand der Skizze zurück.
        let revision = self.revision();
        let saved: Vec<(usize, f64)> = corners
            .iter()
            .flat_map(|&p| {
                let d = &self.points[p];
                [Some(d.x), Some(d.y), d.z]
            })
            .flatten()
            .map(|v| (v, self.vars[v]))
            .collect();
        let mut ok = true;
        for (&p, q) in corners.iter().zip(targets) {
            let moved = if lift.is_some() {
                self.set_point_xyz(p, q)
            } else {
                self.set_point_xy(p, q.x, q.y)
            };
            if moved.is_err() {
                ok = false;
                break;
            }
        }
        if !(ok && self.solve_holding_connected(&corners)) {
            for (v, x) in saved {
                self.vars[v] = x;
            }
            self.restore_revision(revision);
            return false;
        }
        self.refresh_texts();
        true
    }
}

/// Neue Spanne `[a, b]` einer Seite des Rahmens (bisher `[0, len]`) für Glyphen in `[lo, hi]`:
/// `side` −1 hält den Anfang, 1 das Ende, 0 die Mitte. Nur wachsen; passt es schon (bis `tol`),
/// bleibt sie genau.
fn grow_span(len: f64, lo: f64, hi: f64, side: i8, tol: f64) -> (f64, f64) {
    if lo >= -tol && hi <= len + tol {
        return (0.0, len);
    }
    match side {
        -1 => (0.0, len.max(hi)),
        1 => (lo.min(0.0), len),
        _ => {
            let mid = 0.5 * len;
            let half = (0.5 * len).max(mid - lo).max(hi - mid);
            (mid - half, mid + half)
        }
    }
}
