//! Proben für den Handtest der Maße und Mittellinien (Teilprojekt 12): je Körper eine Zeichnung
//! (A3 quer, Ansichten Vorne, Oben, Links und Isometrisch) als SVG und DXF, dazu `ERWARTUNG.txt`
//! mit den Maßwerten aus Formeln und den Prüfpunkten für Fusion 360 und den Browser.
//!
//! Je Probe entstehen zwei Zeichnungen:
//! - `<Name>.svg/.dxf`: die Maße, die „Maße vorschlagen“ setzt (Grundmaße, automatisch),
//! - `<Name>_Hand.svg/.dxf`: von Hand gesetzte Maße mit allen Maßarten der Probe.
//!
//! Der Test `write_dimension_samples_for_the_hand_test` ist ausgeschaltet (er rechnet lange,
//! besser mit `--release`) und wird so gestartet:
//!
//! `cargo test --release -p planora-drawing --test hand_samples_dims -- --ignored`
//!
//! Der Zielordner ist `$PLANORA_SAMPLES_DIR`, sonst `D:\Testumgebung\zeichnung-proben-masse`.
//!
//! Die Sollwerte der handgesetzten Maße stehen als Zahlen im Code (aus den Körpermaßen
//! gerechnet, nicht aus der Bibliothek) und werden beim Schreiben gegen die Messung geprüft.

mod common;

use common::model::*;
use planora_drawing::annotate::{DEFAULT_OFFSET, DIM_STEP, ResolvedDims, dimension_text};
use planora_drawing::dimension::{
    Anchor, ChainMode, ChainSpec, DimError, DimId, DimKind, DimSpec, Geo3, Measured, Tolerance,
    measure,
};
use planora_drawing::layout::{GAP_MM, align_dir, view_extent};
use planora_drawing::model::{
    Drawing, DrawingId, Paper, Placement, Projection, Scale, Sheet, ViewId, ViewKind, ViewSpec,
};
use planora_drawing::sheet::{FRAME_MARGIN_MM, build_sheet_with};
use planora_drawing::stroke::extent_of;
use planora_drawing::{Basis, DrawBody, ViewLines, compute_view, dxf_string, suggest, svg_string};
use planora_exchange::Progress;
use planora_kernel::{Profile, ProfileLoop, Segment, Solid};
use planora_math::{DVec2, DVec3, Frame};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Die vier Ansichten jeder Probe.
const KINDS: [ViewKind; 4] = [
    ViewKind::Front,
    ViewKind::Top,
    ViewKind::Left,
    ViewKind::Isometric,
];

/// Platz um jede Normansicht für Maßlinien und Texte (Blatt-mm, je Seite).
const DIM_PAD: f64 = 22.0;

const NAMES: [&str; 8] = [
    "Quader_Masse",
    "Zylinder_Masse",
    "Bohrung_Masse",
    "Winkel_Masse",
    "Rundung_Masse",
    "Kette_Bezug",
    "Toleranzen",
    "Verloren",
];

// ---------------------------------------------------------------------------------------------
// Beschreibung der Proben

/// Ein von Hand gesetztes Maß mit seinem Sollwert (aus einer Formel).
struct HandDim {
    view: ViewKind,
    kind: DimKind,
    geos: Vec<Geo3>,
    /// Sollwert in mm (Winkel in Grad), von Hand aus den Körpermaßen gerechnet.
    expect: f64,
    label: &'static str,
    offset: f64,
    angle: f64,
    tolerance: Tolerance,
    decimals: u8,
    text_override: Option<&'static str>,
    chain: Option<ChainSpec>,
    /// Erwarteter Text auf dem Blatt (`Haupttext` oder `Haupttext | oben | unten`), von Hand.
    shows: Option<&'static str>,
}

fn hd(view: ViewKind, kind: DimKind, geos: Vec<Geo3>, expect: f64, label: &'static str) -> HandDim {
    HandDim {
        view,
        kind,
        geos,
        expect,
        label,
        offset: DEFAULT_OFFSET,
        angle: 45.0,
        tolerance: Tolerance::None,
        decimals: 1,
        text_override: None,
        chain: None,
        shows: None,
    }
}

impl HandDim {
    fn off(mut self, v: f64) -> Self {
        self.offset = v;
        self
    }
    fn ang(mut self, v: f64) -> Self {
        self.angle = v;
        self
    }
    fn tol(mut self, t: Tolerance) -> Self {
        self.tolerance = t;
        self
    }
    fn dec(mut self, n: u8) -> Self {
        self.decimals = n;
        self
    }
    fn text(mut self, t: &'static str) -> Self {
        self.text_override = Some(t);
        self
    }
    fn chain(mut self, group: u64, mode: ChainMode, order: u32) -> Self {
        self.chain = Some(ChainSpec { group, mode, order });
        self
    }
    fn shows(mut self, s: &'static str) -> Self {
        self.shows = Some(s);
        self
    }
}

/// Ein Maß, dessen Bezug verloren ist (Gegenprobe: rot, nicht exportierbar).
struct LostDim {
    view: ViewKind,
    kind: DimKind,
    /// Lage des Maßes vor dem Verlust (aus einem gültigen Maß gemessen), sonst nur „?“.
    like: Option<usize>,
    reason: &'static str,
}

struct Sample {
    name: &'static str,
    about: &'static str,
    solid: Solid,
    hand: Vec<HandDim>,
    lost: Vec<LostDim>,
    /// Werte (mm), die der Vorschlag laut Formel enthalten muss (nur zur Anzeige und, wenn
    /// `strict`, als Pflicht).
    suggest_values: Vec<f64>,
    strict: bool,
    /// Worauf man in beiden Zeichnungen der Probe achten soll (leer: nichts Gemeinsames).
    focus: &'static str,
    /// Worauf man nur in der Zeichnung mit den von Hand gesetzten Maßen achten soll.
    focus_hand: &'static str,
    /// Worauf man nur in der Zeichnung mit dem Vorschlag achten soll (leer ohne Vorschlag).
    focus_suggest: &'static str,
}

fn p(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

fn ln(a: (f64, f64, f64), b: (f64, f64, f64)) -> Geo3 {
    Geo3::Line {
        a: p(a.0, a.1, a.2),
        b: p(b.0, b.1, b.2),
    }
}

fn pt(x: f64, y: f64, z: f64) -> Geo3 {
    Geo3::Point(p(x, y, z))
}

fn circle_z(x: f64, y: f64, z: f64, r: f64) -> Geo3 {
    Geo3::Circle {
        center: p(x, y, z),
        normal: DVec3::Z,
        radius: r,
        arc: None,
    }
}

/// Bogen um `(x, y, z)` (Normale z) von `a` bis `b` Grad, wie ihn das Programm aus einer
/// Rundungskante auflöst (Anfang, Mitte, Ende).
fn arc_z(x: f64, y: f64, z: f64, r: f64, a: f64, b: f64) -> Geo3 {
    let at = |deg: f64| {
        let t = f64::to_radians(deg);
        p(x + r * t.cos(), y + r * t.sin(), z)
    };
    Geo3::Circle {
        center: p(x, y, z),
        normal: DVec3::Z,
        radius: r,
        arc: Some(planora_drawing::dimension::Arc3 {
            start: at(a),
            mid: at(0.5 * (a + b)),
            end: at(b),
        }),
    }
}

fn axis_z(x: f64, y: f64, r: f64, z0: f64, z1: f64) -> Geo3 {
    Geo3::Axis {
        origin: p(x, y, 0.0),
        dir: DVec3::Z,
        radius: r,
        t0: z0,
        t1: z1,
    }
}

/// Trapez-Winkelstück: Grundriss `(0,0) → (40,0) → (20,20) → (0,20)`, 10 mm hoch.
fn trapezoid() -> Solid {
    let line = |x: f64, y: f64| Segment::Line {
        to: DVec2::new(x, y),
    };
    let profile = Profile {
        outer: ProfileLoop {
            start: DVec2::ZERO,
            segments: vec![
                line(40.0, 0.0),
                line(20.0, 20.0),
                line(0.0, 20.0),
                line(0.0, 0.0),
            ],
        },
        holes: vec![],
    };
    planora_kernel::extrude(
        &profile,
        &Frame::WORLD,
        10.0,
        &planora_math::Tolerance::default(),
    )
    .expect("Trapez")
}

fn sample(name: &str) -> Sample {
    use ChainMode::{Baseline, Chain};
    use DimKind::{Aligned, Angle, Diameter, Horizontal, Radius, Vertical};
    use ViewKind::{Front, Left, Top};
    match name {
        "Quader_Masse" => {
            let (a, b, c) = (10.0, 20.0, 30.0);
            Sample {
                name: "Quader_Masse",
                about: "Quader 10 x 20 x 30 mm",
                solid: box_solid(a, b, c),
                hand: vec![
                    hd(
                        Front,
                        Horizontal,
                        vec![ln((0., 0., 0.), (0., 0., c)), ln((a, 0., 0.), (a, 0., c))],
                        10.0,
                        "Breite",
                    ),
                    hd(
                        Front,
                        Vertical,
                        vec![ln((0., 0., 0.), (a, 0., 0.)), ln((0., 0., c), (a, 0., c))],
                        30.0,
                        "Höhe",
                    ),
                    hd(
                        Front,
                        Aligned,
                        vec![pt(0., 0., 0.), pt(a, 0., c)],
                        (a * a + c * c).sqrt(),
                        "schräg über die Ecken (Wurzel aus 10² + 30²)",
                    )
                    .off(24.0),
                    hd(
                        Top,
                        Horizontal,
                        vec![ln((0., 0., c), (0., b, c)), ln((a, 0., c), (a, b, c))],
                        10.0,
                        "Breite",
                    ),
                    hd(
                        Top,
                        Vertical,
                        vec![ln((0., 0., c), (a, 0., c)), ln((0., b, c), (a, b, c))],
                        20.0,
                        "Tiefe",
                    ),
                    hd(
                        Left,
                        Horizontal,
                        vec![ln((0., 0., 0.), (0., 0., c)), ln((0., b, 0.), (0., b, c))],
                        20.0,
                        "Tiefe",
                    ),
                    hd(
                        Left,
                        Vertical,
                        vec![ln((0., 0., 0.), (0., b, 0.)), ln((0., 0., c), (0., b, c))],
                        30.0,
                        "Höhe",
                    ),
                ],
                lost: vec![],
                suggest_values: vec![10.0, 20.0, 30.0],
                strict: true,
                focus: "Einfache Quaderkanten: Maßlinien mit Pfeilen innen, Text oberhalb bzw. senkrecht von unten nach oben lesbar. Keine Mittellinien (keine runden Teile).",
                focus_hand: "Zusätzlich ein schräges Maß über die Ecken vorne (31,6), parallel zur Diagonale.",
                focus_suggest: "Nur drei Maße: Breite und Höhe vorne, Tiefe oben (die Breite oben wäre dieselbe und fehlt deshalb).",
            }
        }
        "Zylinder_Masse" => {
            let (r, h) = (5.0, 12.0);
            Sample {
                name: "Zylinder_Masse",
                about: "Zylinder Radius 5, Höhe 12 mm, stehend",
                solid: cylinder_solid(r, h),
                hand: vec![
                    hd(
                        Front,
                        Diameter,
                        vec![axis_z(0., 0., r, 0., h)],
                        10.0,
                        "Durchmesser (von der Seite, quer zur Achse)",
                    ),
                    hd(
                        Front,
                        Vertical,
                        vec![circle_z(0., 0., 0., r), circle_z(0., 0., h, r)],
                        12.0,
                        "Höhe zwischen Boden- und Deckelkreis",
                    ),
                    hd(
                        Left,
                        Diameter,
                        vec![axis_z(0., 0., r, 0., h)],
                        10.0,
                        "Durchmesser (von der Seite)",
                    ),
                    hd(
                        Left,
                        Vertical,
                        vec![circle_z(0., 0., 0., r), circle_z(0., 0., h, r)],
                        12.0,
                        "Höhe",
                    ),
                    hd(
                        Top,
                        Diameter,
                        vec![circle_z(0., 0., h, r)],
                        10.0,
                        "Durchmesser (von oben, Führungslinie)",
                    ),
                    hd(
                        Top,
                        Radius,
                        vec![circle_z(0., 0., h, r)],
                        5.0,
                        "Radius (von oben)",
                    )
                    .ang(225.0),
                ],
                lost: vec![],
                suggest_values: vec![10.0, 12.0],
                strict: true,
                focus: "Mittellinien: von der Seite eine Linie in der Achse (strichpunktiert, steht oben und unten 3 mm über), von oben ein Kreuz.",
                focus_hand: "Ø von der Seite als Maßlinie quer zur Achse (vorne und links); von oben Ø und R mit Führungslinie, das R unter 225° (nach links unten), Pfeilspitzen auf dem Kreis.",
                focus_suggest: "Nur zwei Maße: Höhe vorne und Ø oben mit Führungslinie unter 45°.",
            }
        }
        "Bohrung_Masse" => {
            let (a, b, c, r) = (40.0, 30.0, 12.0, 5.0);
            Sample {
                name: "Bohrung_Masse",
                about: "Platte 40 x 30 x 12 mm mit durchgehender Bohrung Ø10 senkrecht, Mitte bei x = 20, y = 15",
                solid: box_with_hole(a, b, c, r, DVec3::Z),
                hand: vec![
                    hd(
                        Top,
                        Diameter,
                        vec![circle_z(20., 15., c, r)],
                        10.0,
                        "Bohrung Ø",
                    ),
                    hd(
                        Top,
                        Horizontal,
                        vec![ln((0., 0., c), (0., b, c)), pt(20., 15., c)],
                        20.0,
                        "Bohrungsmitte von links (Bezugsmaß)",
                    )
                    .chain(1, Baseline, 0),
                    hd(
                        Top,
                        Horizontal,
                        vec![ln((0., 0., c), (0., b, c)), ln((a, 0., c), (a, b, c))],
                        40.0,
                        "Gesamtbreite (Bezugsmaß, zweite Stufe)",
                    )
                    .chain(1, Baseline, 1),
                    hd(
                        Top,
                        Vertical,
                        vec![ln((0., 0., c), (a, 0., c)), pt(20., 15., c)],
                        15.0,
                        "Bohrungsmitte von unten (Bezugsmaß)",
                    )
                    .chain(2, Baseline, 0),
                    hd(
                        Top,
                        Vertical,
                        vec![ln((0., 0., c), (a, 0., c)), ln((0., b, c), (a, b, c))],
                        30.0,
                        "Gesamttiefe (Bezugsmaß, zweite Stufe)",
                    )
                    .chain(2, Baseline, 1),
                    hd(
                        Front,
                        Horizontal,
                        vec![ln((0., 0., 0.), (0., 0., c)), ln((a, 0., 0.), (a, 0., c))],
                        40.0,
                        "Breite",
                    ),
                    hd(
                        Front,
                        Vertical,
                        vec![ln((0., 0., 0.), (a, 0., 0.)), ln((0., 0., c), (a, 0., c))],
                        12.0,
                        "Dicke",
                    ),
                ],
                lost: vec![],
                suggest_values: vec![10.0, 20.0, 15.0, 40.0, 30.0, 12.0],
                strict: true,
                focus: "Mittellinienkreuz in der Bohrung von oben; von vorne und links die verdeckte Bohrung mit einer Mittellinie.",
                focus_hand: "Oben zwei Gruppen Bezugsmaße (waagerecht 20 und 40, senkrecht 15 und 30): gestapelt mit 7 mm Abstand, jede Gruppe geht von derselben Kante aus.",
                focus_suggest: "Oben Ø10 mit Führungslinie, die Bohrungsmitte von links (20) und von unten (15) und die Gesamttiefe (30); Maße derselben Richtung stehen 7 mm gestaffelt. Vorne Breite und Dicke.",
            }
        }
        "Winkel_Masse" => Sample {
            name: "Winkel_Masse",
            about: "Trapez-Platte: Grundriss (0,0) - (40,0) - (20,20) - (0,20), 10 mm hoch (rechter Winkel links unten, 45°-Schräge rechts)",
            solid: trapezoid(),
            hand: vec![
                hd(
                    Top,
                    Angle,
                    vec![
                        ln((0., 0., 10.), (40., 0., 10.)),
                        ln((0., 0., 10.), (0., 20., 10.)),
                    ],
                    90.0,
                    "rechter Winkel links unten",
                ),
                hd(
                    Top,
                    Angle,
                    vec![
                        ln((0., 0., 10.), (40., 0., 10.)),
                        ln((40., 0., 10.), (20., 20., 10.)),
                    ],
                    45.0,
                    "Winkel der Schräge zur Unterkante (45°)",
                )
                .off(14.0),
                hd(
                    Top,
                    Horizontal,
                    vec![ln((0., 0., 10.), (0., 20., 10.)), pt(40., 0., 10.)],
                    40.0,
                    "untere Länge",
                )
                .off(17.0),
                hd(
                    Top,
                    Horizontal,
                    vec![ln((0., 0., 10.), (0., 20., 10.)), pt(20., 20., 10.)],
                    20.0,
                    "obere Länge",
                ),
                hd(
                    Top,
                    Vertical,
                    vec![
                        ln((0., 0., 10.), (40., 0., 10.)),
                        ln((0., 20., 10.), (20., 20., 10.)),
                    ],
                    20.0,
                    "Höhe des Grundrisses",
                ),
                hd(
                    Front,
                    Horizontal,
                    vec![
                        ln((0., 0., 0.), (0., 0., 10.)),
                        ln((40., 0., 0.), (40., 0., 10.)),
                    ],
                    40.0,
                    "Breite",
                ),
                hd(
                    Front,
                    Vertical,
                    vec![
                        ln((0., 0., 0.), (40., 0., 0.)),
                        ln((0., 0., 10.), (40., 0., 10.)),
                    ],
                    10.0,
                    "Dicke",
                ),
            ],
            lost: vec![],
            suggest_values: vec![40.0, 20.0, 10.0],
            strict: false,
            focus: "Keine runden Teile, also keine Mittellinien.",
            focus_hand: "Winkelmaße: 90° mit Bogen zwischen den Schenkeln, 45° für die Schräge (Bogen im Winkelraum, Pfeile auf den Schenkeln, Gradzeichen im Text).",
            focus_suggest: "Keine Winkelmaße (der Vorschlag setzt nie Winkel): nur Breite und Dicke vorne und die Höhe des Grundrisses oben.",
        },
        "Rundung_Masse" => {
            let (a, b, c, r) = (30.0, 20.0, 15.0, 4.0);
            Sample {
                name: "Rundung_Masse",
                about: "Quader 30 x 20 x 15 mm, senkrechte Kanten mit Radius 4 gerundet",
                solid: rounded_box(a, b, c, r),
                hand: vec![
                    hd(
                        Top,
                        Radius,
                        vec![arc_z(r, r, c, r, 180.0, 270.0)],
                        4.0,
                        "Rundung links unten (R; 45° liegt nicht auf dem Viertelkreis, die Führungslinie geht durch seine Mitte bei 225°)",
                    ),
                    hd(
                        Top,
                        Radius,
                        vec![arc_z(a - r, b - r, c, r, 0.0, 90.0)],
                        4.0,
                        "Rundung rechts oben (R, Führungslinie unter 60°)",
                    )
                    .ang(60.0),
                    hd(
                        Top,
                        Horizontal,
                        vec![ln((0., r, c), (0., b - r, c)), ln((a, r, c), (a, b - r, c))],
                        30.0,
                        "Gesamtbreite (Abstand 18 mm = 10 mm neben dem Umriss: die Bezugskante endet 4 mm · 2 vor der Rundung)",
                    )
                    .off(18.0),
                    hd(
                        Top,
                        Vertical,
                        vec![ln((r, 0., c), (a - r, 0., c)), ln((r, b, c), (a - r, b, c))],
                        20.0,
                        "Gesamttiefe (Abstand 18 mm = 10 mm neben dem Umriss: die Bezugskante endet 4 mm · 2 vor der Rundung)",
                    )
                    .off(18.0),
                    hd(
                        Front,
                        Vertical,
                        vec![
                            ln((r, 0., 0.), (a - r, 0., 0.)),
                            ln((r, 0., c), (a - r, 0., c)),
                        ],
                        15.0,
                        "Höhe (Abstand 18 mm = 10 mm neben dem Umriss: die Bezugskante endet 4 mm · 2 vor der Rundung)",
                    )
                    .off(18.0),
                    hd(
                        Front,
                        Horizontal,
                        vec![axis_z(r, r, r, 0., c), axis_z(a - r, r, r, 0., c)],
                        22.0,
                        "Abstand der Rundungsachsen (30 - 2 x 4)",
                    )
                    .off(16.0),
                ],
                lost: vec![],
                suggest_values: vec![30.0, 20.0, 15.0, 4.0],
                strict: false,
                focus: "Mittellinien der vier Rundungen von oben. Jede R-Pfeilspitze sitzt genau auf ihrem Viertelkreis, die Führungslinie zeigt von der Rundungsmitte nach außen. Die Maßlinien der Gesamtmaße liegen 10 mm neben dem Umriss (nicht neben dem Ende der geraden Kante), der Text der senkrechten Maße (20,0 oben, 15,0 vorne) steht frei zwischen Umriss und Maßlinie.",
                focus_hand: "Zwei R: links unten durch die Bogenmitte (225°; der gewünschte Winkel 45° liegt nicht auf diesem Viertelkreis), rechts oben unter 60° (liegt auf dem Bogen, bleibt). Dazu vorne der Abstand der Rundungsachsen (22).",
                focus_suggest: "Vier R4, eine je Ecke: oben rechts unter 45°, oben links 135°, unten links 225°, unten rechts 315° (je durch die Bogenmitte, wo 45° nicht auf dem Bogen liegt).",
            }
        }
        "Kette_Bezug" => {
            let (a, b, c) = (40.0, 30.0, 20.0);
            Sample {
                name: "Kette_Bezug",
                about: "Quader 40 x 30 x 20 mm mit Tasche 20 x 10 mm, 8 mm tief, oben mittig (Tasche von x = 10 bis 30, y = 10 bis 20, Boden bei z = 12)",
                solid: box_with_pocket(a, b, c, 20.0, 10.0, 8.0),
                hand: vec![
                    hd(
                        Top,
                        Horizontal,
                        vec![pt(0., 0., c), pt(10., 10., c)],
                        10.0,
                        "Maßkette: links bis Tasche",
                    )
                    .chain(1, Chain, 0),
                    hd(
                        Top,
                        Horizontal,
                        vec![pt(10., 10., c), pt(30., 10., c)],
                        20.0,
                        "Maßkette: Taschenbreite",
                    )
                    .chain(1, Chain, 1),
                    hd(
                        Top,
                        Horizontal,
                        vec![pt(30., 10., c), pt(40., 0., c)],
                        10.0,
                        "Maßkette: Tasche bis rechts",
                    )
                    .chain(1, Chain, 2),
                    hd(
                        Top,
                        Vertical,
                        vec![pt(0., 0., c), pt(0., 10., c)],
                        10.0,
                        "Bezugsmaße von unten: bis Tasche",
                    )
                    .chain(2, Baseline, 0),
                    hd(
                        Top,
                        Vertical,
                        vec![pt(0., 0., c), pt(0., 20., c)],
                        20.0,
                        "Bezugsmaße von unten: bis Taschenende",
                    )
                    .chain(2, Baseline, 1),
                    hd(
                        Top,
                        Vertical,
                        vec![pt(0., 0., c), pt(0., 30., c)],
                        30.0,
                        "Bezugsmaße von unten: Gesamttiefe",
                    )
                    .chain(2, Baseline, 2),
                    hd(
                        Front,
                        Vertical,
                        vec![pt(0., 0., 0.), pt(0., 0., 12.)],
                        12.0,
                        "Bezugsmaße von unten: Taschenboden",
                    )
                    .chain(3, Baseline, 0),
                    hd(
                        Front,
                        Vertical,
                        vec![pt(0., 0., 0.), pt(0., 0., c)],
                        20.0,
                        "Bezugsmaße von unten: Höhe",
                    )
                    .chain(3, Baseline, 1),
                    hd(
                        Front,
                        Horizontal,
                        vec![pt(0., 0., c), pt(10., 10., c)],
                        10.0,
                        "Bezugsmaße von links: Taschenanfang",
                    )
                    .chain(4, Baseline, 0),
                    hd(
                        Front,
                        Horizontal,
                        vec![pt(0., 0., c), pt(30., 10., c)],
                        30.0,
                        "Bezugsmaße von links: Taschenende",
                    )
                    .chain(4, Baseline, 1),
                    hd(
                        Front,
                        Horizontal,
                        vec![pt(0., 0., c), pt(40., 0., c)],
                        40.0,
                        "Bezugsmaße von links: Gesamtbreite",
                    )
                    .chain(4, Baseline, 2),
                ],
                lost: vec![],
                suggest_values: vec![40.0, 30.0, 20.0],
                strict: false,
                focus: "",
                focus_hand: "Die Maßkette (oben, 10 / 20 / 10) liegt auf einer gemeinsamen Maßlinie, die Texte zwischen den Pfeilen. Die Bezugsmaße (oben senkrecht 10 / 20 / 30, vorne senkrecht 12 / 20 und waagerecht 10 / 30 / 40) stehen gestapelt, jedes von derselben Kante aus.",
                focus_suggest: "Keine Maßketten und keine Bezugsmaße (der Vorschlag setzt nur Gesamtmaße, Ø/R und Bohrungsmitten): Breite 40 und Höhe 20 vorne, Tiefe 30 oben.",
            }
        }
        "Toleranzen" => {
            let (a, b, c, r) = (40.0, 30.0, 12.0, 5.0);
            Sample {
                name: "Toleranzen",
                about: "Platte 40 x 30 x 12 mm mit Bohrung Ø10 (wie Bohrung_Masse), alle Toleranzarten",
                solid: box_with_hole(a, b, c, r, DVec3::Z),
                hand: vec![
                    hd(
                        Top,
                        Horizontal,
                        vec![ln((0., 0., c), (0., b, c)), ln((a, 0., c), (a, b, c))],
                        40.0,
                        "symmetrisch ±0,1",
                    )
                    .tol(Tolerance::Symmetric(0.1))
                    .shows("40,0 ±0,1"),
                    hd(
                        Top,
                        Vertical,
                        vec![ln((0., 0., c), (a, 0., c)), ln((0., b, c), (a, b, c))],
                        30.0,
                        "oben +0,2 / unten -0,1 (gestapelt, kleiner)",
                    )
                    .tol(Tolerance::Limits {
                        upper: 0.2,
                        lower: -0.1,
                    })
                    .shows("30,0 | +0,2 | −0,1"),
                    hd(
                        Top,
                        Diameter,
                        vec![circle_z(20., 15., c, r)],
                        10.0,
                        "Passung H7 als Text hinter dem Wert",
                    )
                    .tol(Tolerance::Fit("H7".into()))
                    .shows("Ø10,0 H7"),
                    hd(
                        Front,
                        Vertical,
                        vec![ln((0., 0., 0.), (a, 0., 0.)), ln((0., 0., c), (a, 0., c))],
                        12.0,
                        "Grenzmaße +0,1 / -0,1: 12,1 und 11,9 statt des Nennmaßes",
                    )
                    .tol(Tolerance::LimitSizes {
                        upper: 0.1,
                        lower: -0.1,
                    })
                    .shows(" | 12,1 | 11,9"),
                    hd(
                        Front,
                        Horizontal,
                        vec![ln((0., 0., 0.), (0., 0., c)), ln((a, 0., 0.), (a, 0., c))],
                        40.0,
                        "symmetrisch ±0,05 mit 2 Nachkommastellen",
                    )
                    .tol(Tolerance::Symmetric(0.05))
                    .dec(2)
                    .shows("40,00 ±0,05"),
                    hd(
                        Left,
                        Vertical,
                        vec![ln((0., 0., 0.), (0., b, 0.)), ln((0., 0., c), (0., b, c))],
                        12.0,
                        "eigener Text mit Umlaut und Prozent",
                    )
                    .text("Höhe <> (95%)")
                    .shows("Höhe 12,0 (95%)"),
                    hd(
                        Left,
                        Horizontal,
                        vec![ln((0., 0., 0.), (0., 0., c)), ln((0., b, 0.), (0., b, c))],
                        30.0,
                        "eigener Text mit Rückstrich",
                    )
                    .text("<> \\ breit")
                    .shows("30,0 \\ breit"),
                ],
                lost: vec![],
                suggest_values: vec![],
                strict: false,
                focus: "",
                focus_hand: "Die Toleranztexte: ±, gestapelte Zeilen, Grenzmaße ohne Nennmaß, Passung als Text. In der DXF-Datei: ±-Code, Umlaut als \\U+00F6, einzelnes Prozentzeichen als %%%, Rückstrich als \\U+005C.",
                focus_suggest: "Ohne Toleranzen (der Vorschlag setzt nie Toleranzen oder eigene Texte): dieselben Maße wie bei Bohrung_Masse.",
            }
        }
        "Verloren" => {
            let (a, b, c) = (10.0, 20.0, 30.0);
            Sample {
                name: "Verloren",
                about: "Quader 10 x 20 x 30 mm; zwei Maße haben ihren Bezug verloren (Gegenprobe)",
                solid: box_solid(a, b, c),
                hand: vec![
                    hd(
                        Front,
                        Horizontal,
                        vec![ln((0., 0., 0.), (0., 0., c)), ln((a, 0., 0.), (a, 0., c))],
                        10.0,
                        "gültig: Breite",
                    ),
                    hd(
                        Front,
                        Vertical,
                        vec![ln((0., 0., 0.), (a, 0., 0.)), ln((0., 0., c), (a, 0., c))],
                        30.0,
                        "(Lage für das verlorene Maß in Vorne)",
                    ),
                ],
                lost: vec![
                    LostDim {
                        view: Front,
                        kind: Vertical,
                        like: Some(1),
                        reason: "Die Kante wurde gelöscht.",
                    },
                    LostDim {
                        view: Top,
                        kind: Horizontal,
                        like: None,
                        reason: "Die Fläche gibt es nicht mehr.",
                    },
                ],
                suggest_values: vec![],
                strict: false,
                focus: "",
                focus_hand: "Rote Maße: In Vorne steht ein rotes Maß an der letzten bekannten Stelle mit dem Grund als Text; in Oben nur ein rotes Fragezeichen. Das zweite Maß in Vorne (30) ist in dieser Probe NICHT gezeichnet, es dient nur als Lagevorlage. Das Programm sperrt den Export solcher Zeichnungen; diese Dateien sind mit der Sperre umgangen geschrieben.",
                focus_suggest: "",
            }
        }
        other => panic!("unbekannte Probe {other}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Blattaufbau

type Rect = (f64, f64, f64, f64);

fn overlaps_or_close(a: Rect, b: Rect, gap: f64) -> bool {
    a.0 < b.2 + gap && b.0 < a.2 + gap && a.1 < b.3 + gap && b.1 < a.3 + gap
}

/// Legt die Ansichten für den Maßstab `scale` aus (wie `hand_samples.rs`, aber jede Normansicht
/// mit [`DIM_PAD`] Rand ringsum für die Maße). `None`, wenn es nicht passt.
fn arrange(sheet: Sheet, lines: &[ViewLines; 4], scale: Scale) -> Option<Drawing> {
    let projection = Projection::First;
    let ext = |i: usize| {
        let (w, h) = view_extent(lines[i].bbox, scale);
        if i == 3 {
            (w, h)
        } else {
            (w + 2.0 * DIM_PAD, h + 2.0 * DIM_PAD)
        }
    };
    let (fw, fh) = ext(0);
    let mut rects: Vec<Rect> = vec![(-fw / 2.0, -fh / 2.0, fw / 2.0, fh / 2.0)];
    let mut dists = Vec::new();
    for i in [1usize, 2] {
        let (dx, dy) = align_dir(projection, ViewKind::Front, KINDS[i]).unwrap();
        let (cw, ch) = ext(i);
        let dist = if dx != 0.0 {
            (fw + cw) / 2.0 + GAP_MM
        } else {
            (fh + ch) / 2.0 + GAP_MM
        };
        dists.push(dist);
        let (cx, cy) = (dx * dist, dy * dist);
        rects.push((cx - cw / 2.0, cy - ch / 2.0, cx + cw / 2.0, cy + ch / 2.0));
    }
    let union = rects.iter().fold(
        (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ),
        |u, r| (u.0.min(r.0), u.1.min(r.1), u.2.max(r.2), u.3.max(r.3)),
    );
    let (w, h) = sheet.size_mm();
    let m = FRAME_MARGIN_MM + 0.5 * GAP_MM;
    let (ox, oy) = (m - union.0, m - union.1);
    let group = (union.0 + ox, union.1 + oy, union.2 + ox, union.3 + oy);
    let (iw, ih) = ext(3);
    let iso = (w - m - iw, h - m - ih, w - m, h - m);
    let inside =
        |r: Rect| r.0 >= m - 1e-9 && r.1 >= m - 1e-9 && r.2 <= w - m + 1e-9 && r.3 <= h - m + 1e-9;
    if !inside(group) || !inside(iso) || overlaps_or_close(group, iso, GAP_MM) {
        return None;
    }
    let mut d = Drawing::new(DrawingId(1), "Probe".into(), sheet, projection);
    let front = d.add_view(
        ViewKind::Front,
        scale,
        Placement::Free { x: ox, y: oy },
        None,
    );
    for (i, dist) in [1usize, 2].into_iter().zip(dists) {
        d.add_view(
            KINDS[i],
            scale,
            Placement::Aligned { distance: dist },
            Some(front),
        );
    }
    d.add_view(
        ViewKind::Isometric,
        scale,
        Placement::Free {
            x: 0.5 * (iso.0 + iso.2),
            y: 0.5 * (iso.1 + iso.3),
        },
        None,
    );
    for v in &mut d.views {
        v.show_hidden = true;
    }
    Some(d)
}

fn choose_layout(sheet: Sheet, lines: &[ViewLines; 4]) -> (Drawing, Scale) {
    let mut series = Scale::SERIES;
    series.sort_by(|a, b| b.factor().total_cmp(&a.factor()));
    for s in series {
        if let Some(d) = arrange(sheet, lines, s) {
            return (d, s);
        }
    }
    panic!("keine Anordnung gefunden");
}

/// Dateiname (ohne Endung) der Zeichnung mit den von Hand gesetzten Maßen.
fn hand_stem(s: &Sample) -> String {
    if s.lost.is_empty() {
        format!("{}_Hand", s.name)
    } else {
        s.name.to_string()
    }
}

fn mm(x: f64) -> String {
    let x = if x.abs() < 0.005 { 0.0 } else { x };
    format!("{x:.2}").replace('.', ",")
}

fn kind_name(k: DimKind) -> &'static str {
    match k {
        DimKind::Horizontal => "waagerecht",
        DimKind::Vertical => "senkrecht",
        DimKind::Aligned => "schräg",
        DimKind::Diameter => "Durchmesser",
        DimKind::Radius => "Radius",
        DimKind::Angle => "Winkel",
    }
}

fn unit(k: DimKind) -> &'static str {
    if k == DimKind::Angle { "°" } else { " mm" }
}

fn spec_of(id: u64, view: ViewId, h: &HandDim) -> DimSpec {
    DimSpec {
        id: DimId(id),
        view,
        kind: h.kind,
        refs: Vec::new(),
        offset: h.offset,
        angle: h.angle,
        text_shift: (0.0, 0.0),
        tolerance: h.tolerance.clone(),
        decimals: h.decimals,
        text_override: h.text_override.map(String::from),
        chain: h.chain,
        suggested: false,
    }
}

/// Text eines Maßes als `Haupttext` oder `Haupttext | oben | unten`.
fn shown_text(spec: &DimSpec, m: &Measured) -> String {
    let t = dimension_text(spec, m);
    match t.stack {
        Some((up, lo)) => format!("{} | {up} | {lo}", t.main),
        None => t.main,
    }
}

fn is_horizontal(a: &Anchor) -> Option<bool> {
    match a {
        Anchor::Linear { dir, .. } => Some(dir.x.abs() >= dir.y.abs()),
        _ => None,
    }
}

type Entries = BTreeMap<ViewId, Vec<(DimSpec, Result<Measured, DimError>)>>;

fn view_of(drawing: &Drawing, k: ViewKind) -> &ViewSpec {
    drawing.views.iter().find(|v| v.kind == k).unwrap()
}

/// Die Maße, die „Maße vorschlagen“ setzen würde (Abstände wie im Programm: erste Stufe 10 mm,
/// jede weitere Stufe 7 mm mehr, jeweils ab dem Umriss der Ansicht (`Candidate::clear`);
/// Bezugsmaße einer Gruppe teilen die erste Stufe).
fn suggested_dims(
    s: &Sample,
    drawing: &Drawing,
    lines: &[ViewLines; 4],
    extent: f64,
) -> (Entries, Vec<(ViewKind, DimKind, f64)>) {
    let body = [DrawBody {
        name: s.name.to_string(),
        solid: &s.solid,
    }];
    let views: Vec<(ViewSpec, &ViewLines)> = drawing
        .views
        .iter()
        .map(|v| {
            let i = KINDS.iter().position(|&k| k == v.kind).unwrap();
            (v.clone(), &lines[i])
        })
        .collect();
    let cands = suggest(&body, &views, &[]);
    let mut entries: Entries = BTreeMap::new();
    let mut shown = Vec::new();
    let mut stack: HashMap<(ViewId, bool), usize> = HashMap::new();
    let mut groups: HashMap<u64, (f64, u32)> = HashMap::new();
    let mut id = 100u64;
    for c in &cands {
        let vk = drawing.view(c.view).unwrap().kind;
        let basis = Basis::of(vk);
        let m = measure(c.kind, &c.geos, &basis, vk, extent)
            .unwrap_or_else(|e| panic!("{}: Vorschlag nicht messbar: {e}", s.name));
        let h = is_horizontal(&m.anchor);
        // Wie im Programm: dazu der Überstand des Umrisses über die Bezüge (mal Maßstab).
        let out = c.clear * drawing.view(c.view).unwrap().scale.factor();
        let level = |stack: &HashMap<(ViewId, bool), usize>| {
            DEFAULT_OFFSET
                + DIM_STEP * h.map_or(0, |h| stack.get(&(c.view, h)).copied().unwrap_or(0)) as f64
                + out
        };
        let (offset, chain) = match c.chain {
            Some(ch) => {
                let e = *groups.entry(ch.group).or_insert((level(&stack), 0));
                groups.insert(ch.group, (e.0, e.1 + 1));
                (
                    e.0,
                    Some(ChainSpec {
                        group: ch.group,
                        mode: ch.mode,
                        order: e.1,
                    }),
                )
            }
            None => (level(&stack), None),
        };
        if let Some(h) = h {
            *stack.entry((c.view, h)).or_insert(0) += 1;
        }
        id += 1;
        let spec = DimSpec {
            id: DimId(id),
            view: c.view,
            kind: c.kind,
            refs: Vec::new(),
            offset,
            angle: 45.0,
            text_shift: (0.0, 0.0),
            tolerance: Tolerance::None,
            decimals: 1,
            text_override: None,
            chain,
            suggested: true,
        };
        shown.push((vk, c.kind, m.value));
        entries.entry(c.view).or_default().push((spec, Ok(m)));
    }
    (entries, shown)
}

fn resolved(entries: Entries, last_known: BTreeMap<DimId, Anchor>) -> ResolvedDims {
    ResolvedDims {
        by_view: entries,
        last_known,
    }
}

/// Schreibt die Dateien einer Probe in `dir` und gibt den Abschnitt für `ERWARTUNG.txt` zurück.
fn write_sample(s: &Sample, dir: &Path) -> String {
    let body = [DrawBody {
        name: s.name.to_string(),
        solid: &s.solid,
    }];
    let extent = extent_of(&body);
    let computed: Vec<ViewLines> = KINDS
        .iter()
        .map(|&k| {
            compute_view(&body, k, &Progress::new())
                .unwrap_or_else(|e| panic!("{}: Ansicht {}: {e}", s.name, k.label()))
        })
        .collect();
    let lines: [ViewLines; 4] = computed.try_into().unwrap();
    let sheet = Sheet {
        paper: Paper::A3,
        landscape: true,
    };
    let (drawing, scale) = choose_layout(sheet, &lines);
    let results: BTreeMap<ViewId, &ViewLines> = drawing
        .views
        .iter()
        .map(|v| {
            let i = KINDS.iter().position(|&k| k == v.kind).unwrap();
            (v.id, &lines[i])
        })
        .collect();

    let mut text = String::new();
    writeln!(
        text,
        "== {} == {}, Erstwinkel, A3 quer, Maßstab {}",
        s.name,
        s.about,
        scale.label()
    )
    .unwrap();
    if !s.focus.is_empty() {
        writeln!(
            text,
            "  Worauf man achten soll (beide Zeichnungen): {}",
            s.focus
        )
        .unwrap();
    }

    // Handgesetzte Maße: Messung gegen den Sollwert aus der Formel prüfen.
    let mut hand_entries: Entries = BTreeMap::new();
    let mut last_known: BTreeMap<DimId, Anchor> = BTreeMap::new();
    let mut measured_hand: Vec<Option<Measured>> = Vec::new();
    let lost_like: Vec<usize> = s.lost.iter().filter_map(|l| l.like).collect();
    writeln!(
        text,
        "  Von Hand gesetzte Maße ({}.svg / {}.dxf):",
        hand_stem(s),
        hand_stem(s)
    )
    .unwrap();
    for (i, h) in s.hand.iter().enumerate() {
        let vs = view_of(&drawing, h.view);
        let basis = Basis::of(h.view);
        let m = measure(h.kind, &h.geos, &basis, h.view, extent).unwrap_or_else(|e| {
            panic!("{}: Maß {} ({}) nicht messbar: {e}", s.name, i + 1, h.label)
        });
        assert!(
            (m.value - h.expect).abs() <= 1e-6,
            "{}: Maß {} ({}): Formel {}, Messung {}",
            s.name,
            i + 1,
            h.label,
            h.expect,
            m.value
        );
        let spec = spec_of(i as u64 + 1, vs.id, h);
        let shown = shown_text(&spec, &m);
        if let Some(want) = h.shows {
            assert_eq!(shown, want, "{}: Maß {} ({}): Text", s.name, i + 1, h.label);
        }
        measured_hand.push(Some(m.clone()));
        if lost_like.contains(&i) {
            // Lagevorlage für ein verlorenes Maß: nicht zeichnen.
            last_known.insert(DimId(1000 + i as u64), m.anchor.clone());
            continue;
        }
        writeln!(
            text,
            "    {:<8} {:<11} {}{}   Text auf dem Blatt: \"{}\"   ({})",
            h.view.label(),
            kind_name(h.kind),
            mm(h.expect),
            unit(h.kind),
            shown,
            h.label
        )
        .unwrap();
        hand_entries.entry(vs.id).or_default().push((spec, Ok(m)));
    }
    for (j, l) in s.lost.iter().enumerate() {
        let vs = view_of(&drawing, l.view);
        let id = DimId(900 + j as u64);
        let spec = DimSpec {
            id,
            view: vs.id,
            kind: l.kind,
            refs: Vec::new(),
            offset: DEFAULT_OFFSET,
            angle: 45.0,
            text_shift: (0.0, 0.0),
            tolerance: Tolerance::None,
            decimals: 1,
            text_override: None,
            chain: None,
            suggested: false,
        };
        if let Some(i) = l.like {
            let anchor = measured_hand[i].as_ref().unwrap().anchor.clone();
            last_known.insert(id, anchor);
        }
        writeln!(
            text,
            "    {:<8} {:<11} ROT (Bezug verloren, Text: \"{}\"){}",
            l.view.label(),
            kind_name(l.kind),
            l.reason,
            if l.like.is_some() {
                ", an der letzten bekannten Stelle"
            } else {
                ", nur ein rotes Fragezeichen, ohne Lage"
            }
        )
        .unwrap();
        hand_entries
            .entry(vs.id)
            .or_default()
            .push((spec, Err(DimError::Lost(l.reason.into()))));
    }
    if !s.focus_hand.is_empty() {
        writeln!(text, "    Nur in {}: {}", hand_stem(s), s.focus_hand).unwrap();
    }
    // Lagevorlagen (nur zum Messen) verwerfen, damit ihr Maß nicht doppelt gezeichnet wird.
    let hand = resolved(hand_entries, last_known);
    let model = build_sheet_with(&drawing, &results, &hand);
    assert_eq!(
        model.views.len(),
        4,
        "{}: nicht alle Ansichten gelegt",
        s.name
    );
    let svg = svg_string(&model).unwrap_or_else(|e| panic!("{}: SVG: {e}", s.name));
    let dxf = dxf_string(&model).unwrap_or_else(|e| panic!("{}: DXF: {e}", s.name));
    std::fs::write(dir.join(format!("{}.svg", hand_stem(s))), svg).unwrap();
    std::fs::write(dir.join(format!("{}.dxf", hand_stem(s))), dxf).unwrap();

    // Vorschlag (außer bei der Gegenprobe).
    if s.lost.is_empty() {
        let (entries, shown) = suggested_dims(s, &drawing, &lines, extent);
        let model = build_sheet_with(&drawing, &results, &resolved(entries, BTreeMap::new()));
        let svg = svg_string(&model).unwrap_or_else(|e| panic!("{}: SVG: {e}", s.name));
        let dxf = dxf_string(&model).unwrap_or_else(|e| panic!("{}: DXF: {e}", s.name));
        std::fs::write(dir.join(format!("{}.svg", s.name)), svg).unwrap();
        std::fs::write(dir.join(format!("{}.dxf", s.name)), dxf).unwrap();
        writeln!(
            text,
            "  Vorschlag ({}.svg / {}.dxf), {} Maße gesetzt:",
            s.name,
            s.name,
            shown.len()
        )
        .unwrap();
        for (vk, k, v) in &shown {
            writeln!(
                text,
                "    {:<8} {:<11} {}{}",
                vk.label(),
                kind_name(*k),
                mm(*v),
                unit(*k)
            )
            .unwrap();
        }
        let missing: Vec<f64> = s
            .suggest_values
            .iter()
            .copied()
            .filter(|w| !shown.iter().any(|(_, _, v)| (v - w).abs() <= 1e-6))
            .collect();
        if !s.suggest_values.is_empty() {
            let list: Vec<String> = s.suggest_values.iter().map(|v| mm(*v)).collect();
            if missing.is_empty() {
                writeln!(
                    text,
                    "    Erwartet (aus der Formel): {} mm - alle vorhanden.",
                    list.join(", ")
                )
                .unwrap();
            } else {
                let m: Vec<String> = missing.iter().map(|v| mm(*v)).collect();
                writeln!(
                    text,
                    "    Erwartet (aus der Formel): {} mm - FEHLT im Vorschlag: {} mm.",
                    list.join(", "),
                    m.join(", ")
                )
                .unwrap();
            }
        }
        if !s.focus_suggest.is_empty() {
            writeln!(text, "    Nur in {}: {}", s.name, s.focus_suggest).unwrap();
        }
        if s.strict {
            assert!(
                missing.is_empty(),
                "{}: Vorschlag ohne erwartete Werte {missing:?}; vorgeschlagen: {shown:?}",
                s.name
            );
        }
    }
    text.push('\n');
    text
}

const CHECKLIST: &str = "\
WAS MAN ANSEHEN SOLL

Zu jeder Probe gibt es zwei Zeichnungen: <Name> (die Maße, die \"Maße vorschlagen\" setzt) und
<Name>_Hand (von Hand gesetzte Maße, mit allen Maßarten). Die Zahlen oben sind aus den
Körpermaßen gerechnet; auf dem Blatt muss dasselbe stehen. Die Gegenprobe \"Verloren\" gibt es nur
einmal (ohne Zusatz _Hand). Bei jeder Probe gilt \"Worauf man achten soll (beide Zeichnungen)\"
für beide Dateien, die Zeilen \"Nur in <Name>_Hand\" bzw. \"Nur in <Name>\" nur für diese eine
Datei. Der Vorschlag setzt nur Gesamtmaße, Ø/R und Bohrungsmitten (als Bezugsmaße); Maßketten,
Winkel, schräge Maße, Toleranzen und eigene Texte gibt es nur in den _Hand-Dateien.

Im Browser oder in Inkscape (SVG-Dateien)
- Maßlinien mit Pfeilen: die Pfeilspitzen sitzen genau auf den Hilfslinien, die Hilfslinien
  beginnen 1 mm vor dem Körper und stehen 2 mm über die Maßlinie.
- R und Ø mit Führungslinie: die Pfeilspitze sitzt immer auf der gezeichneten Kreislinie, auch
  bei Viertel- und Halbkreisen (dann zeigt die Führungslinie von der Mitte durch den Bogen nach
  außen, nie in den Körper).
- Text aufrecht lesbar (waagerecht von links nach rechts, senkrechte Maße von unten nach
  oben gelesen), nicht auf dem Kopf und nicht gespiegelt.
- Zahlen mit deutschem Komma (10,0), Zeichen richtig: Ø, °, ±, Minus als langer Strich.
- Mittellinien strichpunktiert (Strich, Lücke, Punkt, Lücke), 3 mm über die Kanten hinaus;
  runde Teile von oben als Kreuz, von der Seite eine Linie in der Achse.
- \"Verloren\": die verlorenen Maße sind ROT. In Vorne steht der Grund als Text an der
  letzten bekannten Stelle, in Oben nur ein rotes Fragezeichen.
- Maße nehmen nichts anderes mit: keine Maße in der Isometrischen Ansicht.

In Fusion 360 (DXF-Dateien; das Ergebnis ist OFFEN bis zum Handtest)
- Fusion 360 liest DXF-TEXT und -SOLID: Das ist die größte offene Frage dieser Proben. Werden
  die Texte (TEXT) und die Pfeilspitzen (SOLID, gefüllte Dreiecke) beim Einfügen angezeigt?
  Falls Fusion die Datei ablehnt oder Pfeile/Texte fehlen: melden, welche Probe; dann werden
  die Bestandteile einzeln mit ezdxf getauscht, bis klar ist, was Fusion stört.
- Die Texte stehen aufrecht und lesbar, an der richtigen Stelle über der Maßlinie.
- Sonderzeichen: Ø (Code %%c), ° (%%d), ± (%%p) werden als Zeichen angezeigt, nicht als
  Rohtext. Umlaute (Probe \"Toleranzen\", Text \"Höhe 12,0 (95%)\") erscheinen richtig; in der
  Datei stehen sie als \\U+00F6 geschrieben.
- Einzelnes Prozentzeichen: im Text \"Höhe 12,0 (95%)\" muss \"95%\" erscheinen (in der Datei
  steht %%%). Rückstrich: Text \"30,0 \\ breit\" muss den Rückstrich zeigen (in der Datei \\U+005C).
- Linientyp: Mittellinien auf der Ebene MITTEL sind strichpunktiert (Linientyp CENTER), nicht
  durchgezogen; Maßlinien/Hilfslinien auf der Ebene MASSE durchgezogen. Die Strichlänge muss
  zum Maßstab passen.
- Pfeile an den Hilfslinien (nicht daneben), gefüllt.
- \"Verloren\": die roten Maße haben eine eigene Farbe (rot), erscheinen aber sonst wie die anderen.
- Einheit Millimeter, Blatt 420 x 297 mm, Rahmen 10 mm; keine Warnung beim Öffnen; die
  Ansichten (Blöcke) wie bei den früheren Proben.

Hinweise
- Alle Proben sind im Erstwinkel (Europa).
- \"Verloren\" wird vom Programm normalerweise nicht exportiert (der Export ist bei verlorenen
  Maßen gesperrt); die Dateien hier entstehen ohne diese Sperre, nur als Gegenprobe der Farbe.
- Die Textbreite wird nur geschätzt (0,6 mal Texthöhe je Zeichen); echte Schrift kann breiter
  sein, dann liegen Texte dichter an Pfeilen als gedacht.
";

fn write_samples(dir: &Path, names: &[&str]) {
    std::fs::create_dir_all(dir).unwrap();
    let mut all = String::from(
        "ERWARTUNG: Maße und Mittellinien (Teilprojekt 12)\n\
         Je Probe: die Maßwerte aus Formeln der Körpermaße (Maßstab 1:1, Werte in Modell-mm),\n\
         dazu der Text, der auf dem Blatt stehen soll, und was in Fusion 360 zu prüfen ist.\n\n",
    );
    for name in names {
        let s = sample(name);
        all.push_str(&write_sample(&s, dir));
    }
    all.push_str(CHECKLIST);
    std::fs::write(dir.join("ERWARTUNG.txt"), all).unwrap();
}

fn samples_dir() -> PathBuf {
    match std::env::var_os("PLANORA_SAMPLES_DIR") {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => PathBuf::from(r"D:\Testumgebung\zeichnung-proben-masse"),
    }
}

/// Schreibt alle Proben für den Handtest (siehe Kopf der Datei).
#[test]
#[ignore = "schreibt Dateien für den Handtest und rechnet lange: --release -- --ignored"]
fn write_dimension_samples_for_the_hand_test() {
    let dir = samples_dir();
    write_samples(&dir, &NAMES);
    for n in &NAMES {
        let stem = if *n == "Verloren" {
            n.to_string()
        } else {
            format!("{n}_Hand")
        };
        let mut files = vec![format!("{stem}.svg"), format!("{stem}.dxf")];
        if *n != "Verloren" {
            files.push(format!("{n}.svg"));
            files.push(format!("{n}.dxf"));
        }
        for f in files {
            let p = dir.join(&f);
            assert!(std::fs::metadata(&p).unwrap().len() > 100, "{p:?} leer");
        }
    }
    assert!(dir.join("ERWARTUNG.txt").is_file());
    println!("Proben liegen in {}", dir.display());
}

/// Damit der Schreiber nicht verrottet: zwei Proben in einen Ordner unter dem Temp-Verzeichnis.
#[test]
fn dimension_sample_writer_still_works() {
    let dir =
        std::env::temp_dir().join(format!("planora-hand-samples-dims-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write_samples(&dir, &["Quader_Masse", "Zylinder_Masse"]);
    for n in [
        "Quader_Masse",
        "Quader_Masse_Hand",
        "Zylinder_Masse",
        "Zylinder_Masse_Hand",
    ] {
        let svg = std::fs::read_to_string(dir.join(format!("{n}.svg"))).unwrap();
        assert!(svg.contains("<svg"), "{n}.svg ohne SVG-Wurzel");
        let dxf = std::fs::read_to_string(dir.join(format!("{n}.dxf"))).unwrap();
        assert!(dxf.contains("EOF"), "{n}.dxf ohne Dateiende");
        assert!(dxf.contains("MASSE"), "{n}.dxf ohne Maßebene");
    }
    // Der Zylinder hat Mittellinien, die Hand-Probe des Quaders die Textzahl 31,6.
    let cyl = std::fs::read_to_string(dir.join("Zylinder_Masse_Hand.dxf")).unwrap();
    assert!(
        cyl.contains("CENTER"),
        "Zylinder ohne Mittellinien-Linientyp"
    );
    let quader = std::fs::read_to_string(dir.join("Quader_Masse_Hand.svg")).unwrap();
    assert!(quader.contains("31,6"), "schräges Maß 31,6 fehlt");
    let text = std::fs::read_to_string(dir.join("ERWARTUNG.txt")).unwrap();
    assert!(text.contains("== Quader_Masse =="));
    assert!(text.contains("== Zylinder_Masse =="));
    assert!(text.contains("WAS MAN ANSEHEN SOLL"));
    assert!(text.contains("TEXT"));
    let _ = std::fs::remove_dir_all(&dir);
}
