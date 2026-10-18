//! Datenmodell einer Zeichnung: Blatt, Ansichten, Maßstab und Lage.

use crate::dimension::{DimId, DimSpec};
use crate::parts::BodyKey;

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

    /// Name für die Oberfläche.
    pub fn label(self) -> &'static str {
        match self {
            ViewKind::Front => "Vorne",
            ViewKind::Back => "Hinten",
            ViewKind::Left => "Links",
            ViewKind::Right => "Rechts",
            ViewKind::Top => "Oben",
            ViewKind::Bottom => "Unten",
            ViewKind::Isometric => "Isometrisch",
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
        });
        id
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
