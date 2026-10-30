//! Datenmodell einer Zeichnung: Blatt, Ansichten, Maßstab und Lage.

use crate::basis::Basis;
use crate::dimension::{DimId, DimSpec};
use crate::parts::BodyKey;
use crate::section::SectionPlane;
use crate::sheet_frame::sheet_scale;
use planora_i18n::{localize_decimal, tr};
use std::collections::BTreeMap;

/// Kennung einer Zeichnung im Dokument.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DrawingId(pub u64);

/// Kennung einer Ansicht innerhalb einer Zeichnung.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViewId(pub u64);

/// Papiergröße (Hochformat als Grundlage).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paper {
    A4,
    A3,
    A2,
    A1,
    A0,
}

impl Paper {
    /// Breite und Höhe im Hochformat in Millimetern.
    pub fn size_mm(self) -> (f64, f64) {
        match self {
            Paper::A4 => (210.0, 297.0),
            Paper::A3 => (297.0, 420.0),
            Paper::A2 => (420.0, 594.0),
            Paper::A1 => (594.0, 841.0),
            Paper::A0 => (841.0, 1189.0),
        }
    }

    /// Kurzname des Formats („A4“ bis „A0“).
    pub fn label(self) -> &'static str {
        match self {
            Paper::A4 => "A4",
            Paper::A3 => "A3",
            Paper::A2 => "A2",
            Paper::A1 => "A1",
            Paper::A0 => "A0",
        }
    }
}

/// Projektionsart: Erstwinkel (Europa) oder Dritter Winkel (USA).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Projection {
    First,
    Third,
}

/// Blatt: Papiergröße und Ausrichtung.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sheet {
    pub paper: Paper,
    pub landscape: bool,
}

impl Sheet {
    /// Breite und Höhe des Blatts in Millimetern (quer: vertauscht).
    pub fn size_mm(&self) -> (f64, f64) {
        let (w, h) = self.paper.size_mm();
        if self.landscape { (h, w) } else { (w, h) }
    }
}

/// Art einer Ansicht.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewKind {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
    Isometric,
}

/// Name einer Ansicht ohne Text (siehe [`Drawing::view_name_parts`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewName {
    /// Art der Ansicht und ihre Nummer unter den Ansichten dieser Art (ab 1).
    Kind(ViewKind, u32),
    /// Schnittansicht mit ihrem Buchstaben.
    Section(char),
}

impl ViewKind {
    /// Alle Ansichtsarten in fester Reihenfolge.
    pub const ALL: [ViewKind; 7] = [
        ViewKind::Front,
        ViewKind::Back,
        ViewKind::Left,
        ViewKind::Right,
        ViewKind::Top,
        ViewKind::Bottom,
        ViewKind::Isometric,
    ];

    /// Name für Blatt und Oberfläche in der aktuellen Sprache.
    pub fn label(self) -> String {
        match self {
            ViewKind::Front => tr!("drawing-view-front"),
            ViewKind::Back => tr!("drawing-view-back"),
            ViewKind::Left => tr!("drawing-view-left"),
            ViewKind::Right => tr!("drawing-view-right"),
            ViewKind::Top => tr!("drawing-view-top"),
            ViewKind::Bottom => tr!("drawing-view-bottom"),
            ViewKind::Isometric => tr!("drawing-view-isometric"),
        }
    }

    /// Vorgabe für „verdeckte Kanten zeigen“ einer neuen Ansicht: an für die Normansichten
    /// (Vorne, Hinten, Links, Rechts, Oben, Unten; Zeichnungen für die Werkstatt), aus für
    /// Isometrisch. Einzige Stelle dieser Regel; je Ansicht umschaltbar.
    pub fn shows_hidden_by_default(self) -> bool {
        self.is_normal()
    }

    /// Alles außer der isometrischen Ansicht blickt senkrecht auf eine Achsenebene.
    pub fn is_normal(self) -> bool {
        self != ViewKind::Isometric
    }
}

/// Maßstab `num:den` (beide größer als null).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale {
    pub num: u32,
    pub den: u32,
}

impl Scale {
    /// Übliche Maßstabsreihe.
    pub const SERIES: [Scale; 10] = [
        Scale { num: 1, den: 1 },
        Scale { num: 1, den: 2 },
        Scale { num: 1, den: 5 },
        Scale { num: 1, den: 10 },
        Scale { num: 1, den: 20 },
        Scale { num: 1, den: 50 },
        Scale { num: 1, den: 100 },
        Scale { num: 2, den: 1 },
        Scale { num: 5, den: 1 },
        Scale { num: 10, den: 1 },
    ];

    /// Verkleinerungs- bzw. Vergrößerungsfaktor (`num / den`).
    pub fn factor(self) -> f64 {
        f64::from(self.num) / f64::from(self.den)
    }

    /// Beschriftung, z. B. `"1:2"`.
    pub fn label(self) -> String {
        format!("{}:{}", self.num, self.den)
    }

    /// Gültig, wenn Zähler und Nenner größer als null sind.
    pub fn is_valid(self) -> bool {
        self.num > 0 && self.den > 0
    }
}

/// Lage einer Ansicht auf dem Blatt.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Placement {
    /// Frei auf dem Blatt (Mitte der Ansicht in mm).
    Free { x: f64, y: f64 },
    /// Abgeleitet: nur längs der Ausrichtungsachse verschiebbar (Abstand der Ansichtsmitten in mm).
    Aligned { distance: f64 },
}

/// Eine Ansicht auf dem Blatt.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewSpec {
    pub id: ViewId,
    pub kind: ViewKind,
    pub scale: Scale,
    pub placement: Placement,
    pub show_hidden: bool,
    pub parent: Option<ViewId>,
    /// Mittellinien zeigen (Vorgabe: an für Normansichten, aus für Isometrisch).
    pub centerlines: bool,
    /// Gesetzt: Schnittansicht (abhängig von ihrer Ausgangsansicht `parent`).
    pub section: Option<SectionSpec>,
}

/// Buchstaben für Schnitte (ohne I, O, Q).
pub const SECTION_LETTERS: &str = "ABCDEFGHJKLMNPRSTUVWXYZ";
/// Höchstzahl der Schnittansichten je Blatt.
pub const MAX_SECTIONS: usize = 12;
/// Kleinster Schraffurabstand auf dem Blatt in Millimetern.
pub const MIN_HATCH_SHEET_MM: f64 = 0.5;
/// Größter Schraffurabstand (A3-Millimeter).
pub const MAX_HATCH_SPACING: f64 = 50.0;

/// Schraffur einer Schnittansicht: an/aus und Linienabstand in A3-Millimetern (wächst mit dem
/// Blatt, siehe `sheet_scale`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HatchSpec {
    pub on: bool,
    pub spacing: f64,
}

impl Default for HatchSpec {
    fn default() -> HatchSpec {
        HatchSpec {
            on: true,
            spacing: 3.0,
        }
    }
}

/// Angaben einer Schnittansicht: Lage der Ebene (Weltkoordinate auf der Achse), Buchstabe und
/// Schraffur. Die Blickrichtung steckt in `ViewSpec.kind`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SectionSpec {
    pub offset: f64,
    pub letter: char,
    pub hatch: HatchSpec,
}

impl SectionSpec {
    /// Schnittebene dieser Angabe für die Ansichtsart `kind`.
    pub fn plane(&self, kind: ViewKind) -> SectionPlane {
        SectionPlane {
            kind,
            offset: self.offset,
        }
    }
}

/// Schlüssel, unter dem die Linien einer Ansicht berechnet und zwischengespeichert werden: die
/// Ansichtsart oder die Schnittebene. Die Schraffur gehört nicht dazu (sie wirkt erst auf dem
/// Blatt).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewKey {
    Plain(ViewKind),
    Section(SectionPlane),
}

impl ViewKey {
    /// Schlüssel einer Ansicht.
    pub fn of(spec: &ViewSpec) -> ViewKey {
        match &spec.section {
            Some(sec) => ViewKey::Section(sec.plane(spec.kind)),
            None => ViewKey::Plain(spec.kind),
        }
    }
}

/// Art der Schnittansicht, wenn man in einer Ansicht `parent` eine waagerechte oder senkrechte
/// Schnittlinie zieht (Blickrichtung nach unten bzw. nach rechts, umgedreht entgegengesetzt).
/// `None` für die isometrische Ansicht.
pub fn section_kind(parent: ViewKind, horizontal: bool, flipped: bool) -> Option<ViewKind> {
    if !parent.is_normal() {
        return None;
    }
    let pb = Basis::of(parent);
    let mut v = if horizontal { (0.0, -1.0) } else { (1.0, 0.0) };
    if flipped {
        v = (-v.0, -v.1);
    }
    let d = pb.ex * v.0 + pb.ey * v.1;
    ViewKind::ALL
        .into_iter()
        .filter(|k| k.is_normal())
        .find(|k| (Basis::of(*k).d - d).length() < 1e-9)
}

/// Welche Blattachse der Ausgangsansicht die Schnittachse trägt: (waagerechte Linie?,
/// Vorzeichen). `None`, wenn die Achse in Blickrichtung der Ausgangsansicht liegt.
fn line_axis(parent: ViewKind, kind: ViewKind) -> Option<(bool, f64)> {
    if !parent.is_normal() || !kind.is_normal() {
        return None;
    }
    let pb = Basis::of(parent);
    let kd = Basis::of(kind).d;
    let a = (0..3).find(|a| kd[*a].abs() > 0.5)?;
    if pb.ey[a].abs() > 0.5 {
        Some((true, pb.ey[a].signum()))
    } else if pb.ex[a].abs() > 0.5 {
        Some((false, pb.ex[a].signum()))
    } else {
        None
    }
}

/// Lage der Schnittlinie in der Ausgangsansicht: (waagerecht?, Ansichtskoordinate). Waagerecht
/// bei y = `offset · ey[a]`, senkrecht bei x = `offset · ex[a]`. `None`, wenn die Ebene zur
/// Ausgangsansicht nicht passt.
pub fn section_line_coord(parent: ViewKind, plane: &SectionPlane) -> Option<(bool, f64)> {
    let (horizontal, sign) = line_axis(parent, plane.kind)?;
    plane.axis()?;
    Some((horizontal, plane.offset * sign + 0.0))
}

/// Umkehrung von [`section_line_coord`]: Versatz der Ebene aus der Ansichtskoordinate.
pub fn plane_offset_from_view(parent: ViewKind, kind: ViewKind, coord: f64) -> Option<f64> {
    let (_, sign) = line_axis(parent, kind)?;
    Some(coord * sign + 0.0)
}

/// Prüft den Schraffurabstand aller Schnitte für das Blatt `sheet`: höchstens 50 A3-mm und auf
/// dem Blatt mindestens 0,5 mm.
pub fn sections_ok_on(d: &Drawing, sheet: Sheet) -> Result<(), String> {
    let f = sheet_scale(sheet);
    let names = d.view_names();
    for v in &d.views {
        let Some(sec) = &v.section else { continue };
        let spacing = sec.hatch.spacing;
        if spacing > MAX_HATCH_SPACING {
            return Err(tr!(
                "drawing-err-hatch-spacing-max",
                max = localize_decimal(&MAX_HATCH_SPACING.to_string())
            ));
        }
        if spacing.is_nan() || spacing * f < MIN_HATCH_SHEET_MM {
            let name = names.get(&v.id).cloned().unwrap_or_default();
            return Err(tr!("drawing-err-hatch-spacing-min", name = name));
        }
    }
    Ok(())
}

/// Kennung eines Nummernkreises (Positionsnummer) innerhalb einer Zeichnung.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BalloonId(pub u64);

/// Ein Nummernkreis: gehört zur Ansicht `view` und zeigt auf den Körper `body`. `anchor` liegt
/// relativ zur Hüllbox der Ansicht in [0, 1]² (folgt Maßstab und Lage der Ansicht), `offset` ist
/// der Weg vom Anker zur Kreismitte in Blatt-mm.
#[derive(Clone, Debug, PartialEq)]
pub struct Balloon {
    pub id: BalloonId,
    pub view: ViewId,
    pub body: BodyKey,
    pub anchor: (f64, f64),
    pub offset: (f64, f64),
}

/// Art einer Zeichnung: Blatt mit Ansichten oder Stücklistenblatt (ohne Ansichten).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DrawingKind {
    #[default]
    Views,
    /// Stücklistenblatt; `page` ab 1.
    PartsList { page: u32 },
}

/// Eine Zeichnung: Blatt, Projektionsart und Ansichten.
#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    pub id: DrawingId,
    pub name: String,
    pub sheet: Sheet,
    pub projection: Projection,
    pub views: Vec<ViewSpec>,
    pub next_view: u64,
    pub dimensions: Vec<DimSpec>,
    pub next_dim: u64,
    /// Blattangaben des Schriftfelds; `None` bei Zeichnungen aus älteren Dateien.
    pub info: Option<crate::sheet_info::SheetInfo>,
    /// Zeichnungsblatt oder Stücklistenblatt.
    pub kind: DrawingKind,
    /// Nummernkreise (Positionsnummern) der Ansichten.
    pub balloons: Vec<Balloon>,
    pub next_balloon: u64,
}

impl Drawing {
    /// Leere Zeichnung.
    pub fn new(id: DrawingId, name: String, sheet: Sheet, projection: Projection) -> Drawing {
        Drawing {
            id,
            name,
            sheet,
            projection,
            views: Vec::new(),
            next_view: 1,
            dimensions: Vec::new(),
            next_dim: 1,
            info: None,
            kind: DrawingKind::Views,
            balloons: Vec::new(),
            next_balloon: 1,
        }
    }

    /// Fügt eine Ansicht hinzu und gibt ihre Kennung zurück. Verdeckte Kanten nach
    /// [`ViewKind::shows_hidden_by_default`].
    pub fn add_view(
        &mut self,
        kind: ViewKind,
        scale: Scale,
        placement: Placement,
        parent: Option<ViewId>,
    ) -> ViewId {
        let id = ViewId(self.next_view);
        self.next_view += 1;
        self.views.push(ViewSpec {
            id,
            kind,
            scale,
            placement,
            show_hidden: kind.shows_hidden_by_default(),
            parent,
            centerlines: kind.is_normal(),
            section: None,
        });
        id
    }

    /// Erster freier Schnittbuchstabe (freigewordene werden wieder vergeben); `None`, wenn alle
    /// vergeben sind.
    pub fn next_letter(&self) -> Option<char> {
        SECTION_LETTERS.chars().find(|c| {
            !self
                .views
                .iter()
                .any(|v| v.section.is_some_and(|s| s.letter == *c))
        })
    }

    /// Fügt eine Schnittansicht aus `parent` hinzu: Maßstab der Ausgangsansicht, abgeleitet im
    /// Abstand `distance`, verdeckte Kanten aus, Mittellinien an, Schraffur nach Vorgabe.
    pub fn add_section(
        &mut self,
        parent: ViewId,
        kind: ViewKind,
        offset: f64,
        distance: f64,
    ) -> Result<ViewId, String> {
        let source = self
            .view(parent)
            .filter(|p| p.kind.is_normal() && p.section.is_none())
            .ok_or_else(|| tr!("drawing-err-section-source"))?;
        let scale = source.scale;
        if self.views.iter().filter(|v| v.section.is_some()).count() >= MAX_SECTIONS {
            return Err(tr!(
                "drawing-err-sections-max",
                max = MAX_SECTIONS.to_string()
            ));
        }
        let letter = self
            .next_letter()
            .ok_or_else(|| tr!("drawing-err-section-letters"))?;
        let id = ViewId(self.next_view);
        self.next_view += 1;
        self.views.push(ViewSpec {
            id,
            kind,
            scale,
            placement: Placement::Aligned { distance },
            show_hidden: false,
            parent: Some(parent),
            centerlines: true,
            section: Some(SectionSpec {
                offset,
                letter,
                hatch: HatchSpec::default(),
            }),
        });
        Ok(id)
    }

    /// Namen der Ansichten ohne Text: Art und laufende Nummer (ab 1, je Art; Schnitte zählen
    /// nicht mit) bzw. der Buchstabe des Schnitts. Aus diesen Teilen entstehen die Namen auf dem
    /// Blatt ([`Drawing::view_names`]) und in der Oberfläche der App.
    pub fn view_name_parts(&self) -> BTreeMap<ViewId, ViewName> {
        let mut counts: BTreeMap<usize, u32> = BTreeMap::new();
        let mut out = BTreeMap::new();
        for v in &self.views {
            if let Some(sec) = &v.section {
                out.insert(v.id, ViewName::Section(sec.letter));
                continue;
            }
            let i = ViewKind::ALL.iter().position(|k| *k == v.kind).unwrap_or(0);
            let n = counts.entry(i).or_insert(0);
            *n += 1;
            out.insert(v.id, ViewName::Kind(v.kind, *n));
        }
        out
    }

    /// Namen der Ansichten wie auf dem Blatt, in der aktuellen Sprache: Normansichten mit ihrer
    /// Art, bei mehrfach gleicher Art mit Nummer („Oben 2“); Schnitte zählen dabei nicht mit und
    /// heißen „Schnitt A–A“.
    pub fn view_names(&self) -> BTreeMap<ViewId, String> {
        self.view_name_parts()
            .into_iter()
            .map(|(id, p)| {
                let name = match p {
                    ViewName::Section(l) => crate::section_marks::section_view_name(l),
                    ViewName::Kind(k, 1) => k.label(),
                    ViewName::Kind(k, n) => {
                        tr!("drawing-view-numbered", name = k.label(), n = n.to_string())
                    }
                };
                (id, name)
            })
            .collect()
    }

    /// Sucht eine Ansicht.
    pub fn view(&self, id: ViewId) -> Option<&ViewSpec> {
        self.views.iter().find(|v| v.id == id)
    }

    /// Entfernt die Ansicht samt allen abgeleiteten; gibt die entfernten Kennungen zurück
    /// (erst die Ansicht, dann ihre Kinder in Listenreihenfolge, rekursiv).
    pub fn remove_view(&mut self, id: ViewId) -> Vec<ViewId> {
        let mut removed = Vec::new();
        if self.view(id).is_none() {
            return removed;
        }
        self.collect_subtree(id, &mut removed);
        self.views.retain(|v| !removed.contains(&v.id));
        self.dimensions.retain(|m| !removed.contains(&m.view));
        self.balloons.retain(|b| !removed.contains(&b.view));
        removed
    }

    /// Fügt ein Maß hinzu; die Kennung in `spec.id` wird ersetzt und zurückgegeben.
    pub fn add_dimension(&mut self, mut spec: DimSpec) -> DimId {
        let id = DimId(self.next_dim);
        self.next_dim += 1;
        spec.id = id;
        self.dimensions.push(spec);
        id
    }

    /// Sucht ein Maß.
    pub fn dimension(&self, id: DimId) -> Option<&DimSpec> {
        self.dimensions.iter().find(|m| m.id == id)
    }

    /// Entfernt ein Maß; `false`, wenn es die Kennung nicht gibt.
    pub fn remove_dimension(&mut self, id: DimId) -> bool {
        let before = self.dimensions.len();
        self.dimensions.retain(|m| m.id != id);
        self.dimensions.len() != before
    }

    /// Fügt einen Nummernkreis hinzu; die Kennung in `b.id` wird ersetzt und zurückgegeben.
    pub fn add_balloon(&mut self, mut b: Balloon) -> BalloonId {
        let id = BalloonId(self.next_balloon);
        self.next_balloon += 1;
        b.id = id;
        self.balloons.push(b);
        id
    }

    /// Sucht einen Nummernkreis.
    pub fn balloon(&self, id: BalloonId) -> Option<&Balloon> {
        self.balloons.iter().find(|b| b.id == id)
    }

    /// Entfernt einen Nummernkreis; `false`, wenn es die Kennung nicht gibt.
    pub fn remove_balloon(&mut self, id: BalloonId) -> bool {
        let before = self.balloons.len();
        self.balloons.retain(|b| b.id != id);
        self.balloons.len() != before
    }

    fn collect_subtree(&self, id: ViewId, out: &mut Vec<ViewId>) {
        if out.contains(&id) {
            return;
        }
        out.push(id);
        let children: Vec<ViewId> = self
            .views
            .iter()
            .filter(|v| v.parent == Some(id))
            .map(|v| v.id)
            .collect();
        for c in children {
            self.collect_subtree(c, out);
        }
    }
}
