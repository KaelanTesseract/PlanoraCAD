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
use crate::model::{CurveId, CurveKind, PointId, Sketch, within_coord_range};
use crate::text_layout::{place_box, place_path};
use crate::text_shape::{
    Contour, MAX_TEXT_CHARS, MAX_TEXT_CURVES, Seg, TextShape, TextShapeError, TextShaper, TextStyle,
};
use planora_i18n::tr;
use planora_kernel::Id;
use planora_math::DVec2;
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

/// Das zuletzt gesetzte [`TextShape`], nur zur Laufzeit (nicht in der Datei).
///
/// Der Vergleich liefert immer `true` (**Review Focus 1**): Rückgängig-Bilder und „geändert“
/// sehen den Zwischenspeicher nie. Ein bloßes Neusetzen nach dem Laden, das dieselben Kurven
/// liefert, ist so keine Änderung.
#[derive(Clone, Debug, Default)]
pub struct ShapedCache(pub(crate) Option<Arc<TextShape>>);

impl PartialEq for ShapedCache {
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
    /// Die erzeugten Kurven in fester Reihenfolge (Zeile, Glyphe, Kontur, Stück).
    pub curves: Vec<CurveId>,
    /// Ihre Punkte in fester Reihenfolge; je Kontur der Anfang, dann je Stück die Kontrollpunkte
    /// und das Ende, außer beim letzten Stück (es endet am Anfang).
    pub points: Vec<PointId>,
    pub(crate) alive: bool,
    pub(crate) shaped: ShapedCache,
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
}

/// Größter erlaubter Zeichenabstand in Prozent (Spec A7).
pub(crate) const MAX_SPACING: f64 = 1000.0;

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

fn out_of_range() -> SketchError {
    SketchError::InvalidInput(tr!("sketch-text-out-of-range"))
}

/// Was für einen Text auf Pfad gesetzt wird: Die Zeilen werden mit einem Leerzeichen verbunden
/// (Spec B7, **Annahme**); Zeilenumbrüche wie beim Setzen (`\r\n`, `\r`, `\n`). Gespeichert bleibt
/// der Inhalt, wie er ist, damit ein Wechsel in den Rahmen die Zeilen zurückbringt.
pub fn path_content(content: &str) -> String {
    content.replace("\r\n", "\n").replace(['\r', '\n'], " ")
}

/// Was für den Text `d` gesetzt wird: der Inhalt, auf einem Pfad mit verbundenen Zeilen.
fn shaping_content(d: &TextData) -> Cow<'_, str> {
    match d.placement {
        TextPlacement::Box { .. } => Cow::Borrowed(&d.content),
        TextPlacement::Path { .. } => Cow::Owned(path_content(&d.content)),
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

    /// Der Text, dessen Rahmenlinie `c` ist.
    pub fn text_of_frame(&self, c: CurveId) -> Option<TextId> {
        self.texts()
            .find(|(_, d)| matches!(d.placement, TextPlacement::Box { lines, .. } if lines.contains(&c)))
            .map(|(t, _)| t)
    }

    /// Ist der Text gesetzt? Nach dem Laden ist er es erst nach [`Sketch::reshape_texts`]; bis
    /// dahin bleiben die gespeicherten Kurven, wie sie sind.
    pub fn is_text_shaped(&self, t: TextId) -> bool {
        self.text(t).is_some_and(|d| d.shaped.0.is_some())
    }

    /// Die Variablen aller erzeugten Punkte lebender Texte: für den Löser Konstanten, auch bei
    /// einem noch nicht gesetzten Text.
    pub(crate) fn text_vars(&self) -> HashSet<usize> {
        let mut out = HashSet::new();
        for (_, d) in self.texts() {
            for &p in &d.points {
                if self.points[p].alive {
                    out.extend([self.points[p].x, self.points[p].y]);
                }
            }
        }
        out
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

    /// Vor dem Löschen der Kurve `c`: Rahmenlinien und erzeugte Kurven gehen nur mit dem ganzen
    /// Text (Spec B8), ein Pfad mit Text gar nicht.
    pub(crate) fn refuse_text_curve_removal(&self, c: CurveId) -> Result<(), SketchError> {
        if self.texts.is_empty() {
            return Ok(());
        }
        if self.text_of_curve(c).is_some() || self.text_of_frame(c).is_some() {
            return Err(SketchError::InvalidInput(tr!("sketch-text-part")));
        }
        if self.is_text_path(c) {
            return Err(SketchError::InvalidInput(tr!("sketch-text-on-path")));
        }
        Ok(())
    }

    /// Vor dem Löschen des Punktes `p`: Ecken und erzeugte Punkte gehen nur mit dem ganzen Text;
    /// ein Punkt, dessen Kurve ein Pfad mit Text ist, nicht.
    pub(crate) fn refuse_text_point_removal(&self, p: PointId) -> Result<(), SketchError> {
        if self.texts.is_empty() {
            return Ok(());
        }
        if self.is_text_point(p) || self.is_text_corner(p) {
            return Err(SketchError::InvalidInput(tr!("sketch-text-part")));
        }
        for (c, d) in self.curves() {
            if d.kind.points().contains(&p) {
                self.refuse_text_curve_removal(c)?;
            }
        }
        Ok(())
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

    /// Rahmenlinien bleiben Konstruktion: weder normal noch Mittellinie (Ruling zu Spec B4).
    pub(crate) fn refuse_frame_style(&self, c: CurveId) -> Result<(), SketchError> {
        if self.text_of_frame(c).is_some() {
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
        let d = &self.texts[t];
        self.place_text(d.placement, d.height, d.flip_h, d.flip_v, shape)
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
        let placed = match placement {
            TextPlacement::Box {
                corners, h, v: va, ..
            } => {
                if corners.iter().any(|&p| !self.points[p].alive) {
                    return None;
                }
                let at = corners.map(|p| self.pos(p));
                if !at.iter().all(|&p| within_coord_range(p))
                    || at[0].distance(at[1]) <= self.tol.linear
                {
                    return None;
                }
                place_box(shape, height, at, h, va, flip_h, flip_v)
            }
            TextPlacement::Path {
                path,
                above,
                fit,
                h,
            } => {
                let (geom, closed, clockwise) = self.text_path_geom(path)?;
                place_path(
                    shape, height, &geom, closed, clockwise, above, fit, h, flip_h, flip_v,
                )?
            }
        };
        placed_ok(&placed).then_some(placed)
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
            let mut points = [Id::from_index(0); 4];
            for (slot, c) in points.iter_mut().zip(corners) {
                *slot = s.add_point(c.x, c.y)?;
            }
            let mut lines = [Id::from_index(0); 4];
            for (i, slot) in lines.iter_mut().enumerate() {
                *slot = s.add_line(points[i], points[(i + 1) % 4])?;
                s.curves[*slot].construction = true;
            }
            s.push_internal(ConstraintKind::Perpendicular(lines[0], lines[1]));
            s.push_internal(ConstraintKind::Parallel(lines[0], lines[2]));
            s.push_internal(ConstraintKind::Parallel(lines[1], lines[3]));
            s.push_internal(ConstraintKind::Horizontal(LineOrPoints::Line(lines[0])));
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
                curves: Vec::new(),
                points: Vec::new(),
                alive: true,
                shaped: ShapedCache(Some(shape)),
            });
            s.touch_structure();
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
        if self.curve(path).is_none() {
            return Err(SketchError::UnknownObject);
        }
        if self.text_of_curve(path).is_some()
            || self.text_of_frame(path).is_some()
            || self.is_handle_curve(path)
        {
            return Err(SketchError::InvalidInput(tr!("sketch-text-bad-path")));
        }
        check_params(&params)?;
        check_new_shape(&shape)?;
        if self.text_path_geom(path).is_none() {
            return Err(SketchError::Degenerate(tr!("sketch-text-path-degenerate")));
        }
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
            curves: Vec::new(),
            points: Vec::new(),
            alive: true,
            shaped: ShapedCache(Some(shape)),
        });
        self.touch_structure();
        self.sync_text(t, &placed);
        Ok(t)
    }

    /// Ändert Werte und Ausrichtung eines Textes im Rahmen; `shape` ist der mit `params` gesetzte
    /// Text. Die Kurven werden der Reihe nach wiederverwendet (Spec B3): Ändern sich nur Höhe,
    /// Abstand, Ausrichtung oder Umkehren, behalten alle Kurven ihre Ids. Der Rahmen bleibt. Bei
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
        next.shaped = ShapedCache(Some(shape.clone()));
        self.texts[t] = next;
        let Some(placed) = self.placed_contours(t, &shape) else {
            self.texts[t] = before;
            return Err(out_of_range());
        };
        if self.texts[t] != before {
            self.touch_structure();
        }
        self.sync_text(t, &placed);
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
        let used = self.points_used_outside(&curves);
        let points: HashSet<PointId> = points.into_iter().filter(|p| !used.contains(p)).collect();
        for &c in &curves {
            self.dissolve_patterns_of_curve(c);
        }
        for &p in &points {
            self.dissolve_patterns_of_point(p);
        }
        for &c in &curves {
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
        self.texts[t].alive = false;
        self.touch_structure();
        Ok(())
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
    /// Focus 1). Ein Text, den `shaper` nicht setzen kann, bleibt unverändert und ungesetzt. Auf
    /// einem Pfad wird mit verbundenen Zeilen gesetzt ([`path_content`]).
    pub fn reshape_texts(&mut self, shaper: &dyn TextShaper) -> ReshapeReport {
        let mut report = ReshapeReport::default();
        let ids: Vec<TextId> = self.texts().map(|(t, _)| t).collect();
        for t in ids {
            let d = &self.texts[t];
            let Ok(shape) = shaper.shape(&shaping_content(d), &d.style) else {
                continue;
            };
            if check_curve_limit(&shape).is_err() {
                continue;
            }
            if let Some(name) = &shape.substituted {
                report.substituted.push((t, name.clone()));
            }
            if shape.synthetic {
                report.synthetic.push(t);
            }
            if shape.missing_chars {
                report.missing_chars.push(t);
            }
            if d.shaped.0.as_deref() == Some(&shape) {
                continue;
            }
            let shape = Arc::new(shape);
            let placed = self.placed_contours(t, &shape);
            self.texts[t].shaped = ShapedCache(Some(shape));
            if let Some(placed) = placed {
                report.changed |= self.sync_text(t, &placed);
            }
        }
        report
    }
}
