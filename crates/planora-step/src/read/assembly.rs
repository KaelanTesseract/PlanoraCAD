//! Körper, Namen und Baugruppen-Lagen (STEP-Import, Aufgabe 8).
//!
//! **Was ein Körper ist (P2):** Je Vorkommen einer Darstellung mit mindestens einem
//! `MANIFOLD_SOLID_BREP`/`BREP_WITH_VOIDS` ein Körper; alle Volumen-Einträge der Darstellung
//! werden Schalen dieses einen Körpers (steht derselbe Volumen-Eintrag in zwei Darstellungen
//! desselben Raums, zählt er einmal). Volumen-Einträge, die in keiner Darstellung stehen, sind
//! je ein eigener Körper. Ein Teil, das eine Baugruppe zweimal benutzt, ergibt zwei Körper.
//!
//! **Lagen (P3):** Die Baugruppe ist ein Graph aus Darstellungen. Kanten (Eltern → Kind, mit der
//! Transformation von Kind- in Elternkoordinaten):
//! - `CONTEXT_DEPENDENT_SHAPE_REPRESENTATION` (je CDSR ein Vorkommen, auch wenn zwei dieselbe
//!   Beziehung nennen): Die Beziehung (`rep_1`, `rep_2`) ist nach ISO `rep_1` = Kind; die
//!   Richtung wird aber über das zugehörige `NEXT_ASSEMBLY_USAGE_OCCURRENCE` bestimmt
//!   (Elternteil = `relating_product_definition`): Gehört genau eine der beiden Darstellungen zum
//!   Kind, entscheidet das, sonst genau eine zum Elternteil, sonst „Richtung unklar“.
//!   `ITEM_DEFINED_TRANSFORMATION(it1, it2)`: `it1` gehört zu `rep_1`, `it2` zu `rep_2` (ISO
//!   10303-43); stehen die Lagen in den `items` der Darstellungen gekreuzt, werden sie getauscht,
//!   widersprechen sie beiden Zuordnungen: „Richtung unklar“. `T = A(Eltern-Lage) ·
//!   A(Kind-Lage)⁻¹`. Ein `CARTESIAN_TRANSFORMATION_OPERATOR_3D` bildet `rep_1` auf `rep_2` ab
//!   (vertauscht: sein Kehrwert). Ohne Transformation: Einheitslage – außer eine `MAPPED_ITEM`
//!   verbindet dieselben Gruppen schon (sie trägt dann die Lage, das Vorkommen ohne
//!   Transformation entfällt, sonst gäbe es einen zweiten Körper am Ursprung).
//! - `MAPPED_ITEM` in einer Darstellung: Eltern = diese Darstellung, Kind = die abgebildete;
//!   `T = A(mapping_target) · A(mapping_origin)⁻¹`.
//! - Beziehung ohne CDSR und ohne Transformation (etwa `SHAPE_REPRESENTATION` ↔
//!   `ADVANCED_BREP_SHAPE_REPRESENTATION` eines Teils): beide Darstellungen liegen im selben
//!   Raum (eine Gruppe). Ebenso eine nicht platzierte Darstellung derselben Produktdefinition wie
//!   eine platzierte (zwei `SHAPE_DEFINITION_REPRESENTATION` eines Teils).
//! - Beziehung mit Transformation, aber ohne Vorkommen: Eltern ist die Darstellung, die als
//!   einzige zu einem Produkt gehört, sonst `rep_2` (ISO).
//!
//! Jede Lage wird in den Einheiten der Darstellung gelesen, zu der sie gehört (`units_of`), und
//! ist danach eine Bewegung in mm. Verschachtelte Lagen werden bis zur Wurzel multipliziert
//! (`M = T_Wurzel · … · T_Kind`). Spiegelung (negative Determinante), Skalierung (Maßstab ≠ 1,
//! auch nicht gleichmäßig), schiefe Achsen (Kosinus über 1e-4 oder entartet; darunter wie ISO
//! projiziert), unbekannte Arten von Lagen und
//! unklare Richtungen ergeben eine Meldung ([`placement_note`]); der Körper und alles darunter
//! liegt dann am Ursprung. Darstellungen in einem Kreis (und alles, was nur über einen Kreis
//! erreichbar ist) erscheinen je einmal mit „Verweise im Kreis“; tiefer als
//! [`MAX_ASSEMBLY_DEPTH`] Ebenen: „zu tief verschachtelt“.
//!
//! **Kaputte Einträge** treffen nur, was sie betreffen: Darstellungen werden nur gelesen, wenn
//! sie Volumen-Einträge oder `MAPPED_ITEM`s enthalten oder Endpunkt einer Beziehung sind;
//! Einträge werden ohne Verweisprüfung zerlegt ([`StepFile::entity_unchecked`]), damit ein
//! fehlender Verweis nur seinen Endpunkt kostet. Ein unlesbares Vorkommen (PDS/NAUO) hinter einer
//! lesbaren Beziehung → „Richtung unklar – …“. Abgelehnt wird die ganze Datei nur, wenn ein
//! Körper sonst still an falscher Stelle läge: eine Beziehung, deren einer Endpunkt fehlt oder
//! keine Darstellung ist, während unter dem anderen Körper hängen; ein CDSR ohne lesbare
//! Beziehung, dessen Kind Körper hat; eine unlesbare `MAPPED_ITEM`/`REPRESENTATION_MAP`. Eine
//! unlesbare Darstellung könnte freie Volumen-Einträge enthalten haben: Diese bekommen dann die
//! Meldung mit dem Grund.
//!
//! **Reihenfolge:** Tiefensuche ab den Wurzeln (Gruppen ohne eingehende Kante) in
//! Dateireihenfolge, Kinder in der Reihenfolge ihrer Einträge; dann nicht erreichte
//! Darstellungen; dann freie Volumen-Einträge. Teilbäume ohne Körper werden nicht betreten
//! (vorab gezählt), so bleibt die Suche auch bei vielen leeren Verzweigungen linear.
//!
//! **Namen (P10):** Produktname (`PRODUCT.name`, nicht die Kennung), sonst Name der Darstellung,
//! sonst Name des ersten Volumen-Eintrags; Steuerzeichen → Leerzeichen, getrimmt, höchstens
//! [`MAX_BODY_NAME_CHARS`] Zeichen; Doppelte „Name (2)“, „Name (3)“ … (auch dann höchstens 64
//! Zeichen); leere Namen bleiben leer (die App vergibt „STEP-Körper n“).

use crate::read::error::StepReadError;
use crate::read::geom::{Resolver, Skip};
use crate::read::index::StepFile;
use crate::read::units::Units;
use crate::read::value::{Entity, Value};
use planora_i18n::tr;
use planora_math::{DMat4, DVec3, Frame};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Höchstens so viele Ebenen von Vorkommen liegen zwischen Wurzel und Körper.
pub const MAX_ASSEMBLY_DEPTH: usize = 32;
/// Höchstens so viele Zeichen hat ein Körpername.
pub const MAX_BODY_NAME_CHARS: usize = 64;

/// Ein Körper (Vorkommen) der Datei.
#[derive(Clone, Debug, PartialEq)]
pub struct FoundBody {
    /// Nach P10 gekürzt und eindeutig; leer, wenn die Datei keinen Namen hat.
    pub name: String,
    /// `MANIFOLD_SOLID_BREP` / `BREP_WITH_VOIDS`.
    pub items: Vec<u64>,
    /// `GEOMETRIC_REPRESENTATION_CONTEXT` der Darstellung (`None` bei freien Volumen-Einträgen).
    pub context: Option<u64>,
    /// Bewegung in mm (Einheitslage bei `placement_note`).
    pub placement: DMat4,
    pub placement_note: Option<String>,
    /// Die Richtung einer Beziehung auf dem Weg zur Wurzel wurde angenommen (Beziehung mit
    /// Transformation ohne Vorkommen); die Lage ist angewandt, Meldung [`guessed_note`]. Nur
    /// ohne `placement_note`.
    pub placement_guessed: bool,
}

/// Meldung zu einer Lage, die sich nicht bestimmen ließ.
pub fn placement_note(name: &str, reason: &str) -> String {
    tr!("step-r-placement-failed", name = name, reason = reason)
}

/// Meldung zu einer Lage, deren Richtung in der Baugruppe angenommen wurde (Beziehung mit
/// Transformation ohne Vorkommen: Eltern ist die Darstellung mit Produkt, sonst `rep_2`). Der
/// Körper liegt an der angenommenen Stelle.
pub fn guessed_note(name: &str) -> String {
    tr!("step-r-placement-guessed", name = name)
}

/// Arten von Körpern, die (noch) nicht übernommen werden: Typ, Text-Id der Einzahl, der Mehrzahl.
const UNSUPPORTED: &[(&str, &str, &str)] = &[
    (
        "FACETED_BREP",
        "step-r-kind-faceted-one",
        "step-r-kind-faceted-many",
    ),
    (
        "SHELL_BASED_SURFACE_MODEL",
        "step-r-kind-surface-one",
        "step-r-kind-surface-many",
    ),
    (
        "FACE_BASED_SURFACE_MODEL",
        "step-r-kind-surface-one",
        "step-r-kind-surface-many",
    ),
    (
        "SHELL_BASED_WIREFRAME_MODEL",
        "step-r-kind-wire-one",
        "step-r-kind-wire-many",
    ),
    (
        "EDGE_BASED_WIREFRAME_MODEL",
        "step-r-kind-wire-one",
        "step-r-kind-wire-many",
    ),
];

/// Je Art von Körper, die die Datei enthält, aber nicht übernommen wird, eine Meldung
/// (Reihenfolge wie [`UNSUPPORTED`]).
pub fn unsupported_kinds(file: &StepFile) -> Vec<String> {
    UNSUPPORTED
        .iter()
        .filter_map(|(ty, one, many)| {
            let n = file.ids_of(ty).len();
            (n > 0).then(|| {
                tr!(
                    "step-r-unsupported-kind",
                    kind = planora_i18n::t(if n == 1 { one } else { many }),
                    n = n,
                    ty = *ty
                )
            })
        })
        .collect()
}

/// Gründe (wörtlich, Plan Aufgabe 8).
fn mirror() -> String {
    tr!("step-r-reason-mirror")
}
fn scaling() -> String {
    tr!("step-r-reason-scaling")
}
fn oblique() -> String {
    tr!("step-r-reason-oblique")
}
fn cycle() -> String {
    tr!("step-r-reason-cycle")
}
fn too_deep() -> String {
    tr!("step-r-reason-too-deep")
}
fn unclear() -> String {
    tr!("step-r-reason-unclear")
}

/// Typen, deren Einträge als Darstellungen nach Volumen-Einträgen durchsucht werden.
const REP_TYPES: &[&str] = &[
    "ADVANCED_BREP_SHAPE_REPRESENTATION",
    "SHAPE_REPRESENTATION",
    "MANIFOLD_SURFACE_SHAPE_REPRESENTATION",
    "FACETED_BREP_SHAPE_REPRESENTATION",
    "REPRESENTATION",
];
const SOLID_TYPES: &[&str] = &["MANIFOLD_SOLID_BREP", "BREP_WITH_VOIDS"];
const RELATIONSHIP_TYPES: &[&str] = &[
    "REPRESENTATION_RELATIONSHIP",
    "SHAPE_REPRESENTATION_RELATIONSHIP",
    "REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION",
];
/// Beziehungen zwischen Produktdefinitionen mit `relating` (Eltern) an Stelle 4 und `related`
/// (Kind) an Stelle 5.
const USAGE_TYPES: &[&str] = &[
    "NEXT_ASSEMBLY_USAGE_OCCURRENCE",
    "ASSEMBLY_COMPONENT_USAGE",
    "PRODUCT_DEFINITION_USAGE",
    "PRODUCT_DEFINITION_RELATIONSHIP",
];
const CTO_3D: &str = "CARTESIAN_TRANSFORMATION_OPERATOR_3D";
const CTO_NON_UNIFORM: &str = "CARTESIAN_TRANSFORMATION_OPERATOR_3D_NON_UNIFORM";

/// Kosinus, bis zu dem gegebene Achsen eines Operators wie in ISO 10303-42 senkrecht projiziert
/// werden (Dateien mit sechsstelligen Richtungen); darüber gelten sie als schief.
const OBLIQUE_TOL: f64 = 1e-4;
/// Kürzer als das ist eine projizierte Achse entartet.
const DEGENERATE: f64 = 1e-9;
/// Abstand von 1, ab dem ein Maßstab eine Skalierung ist (darunter wird genau 1 genommen).
const SCALE_TOL: f64 = 1e-6;

/// Ergebnis einer Lage: Bewegung in mm oder Grund.
type Placed = Result<DMat4, String>;

/// Eine Darstellung.
struct Rep {
    id: u64,
    name: String,
    items: Vec<u64>,
    solids: Vec<u64>,
    context: Option<u64>,
}

/// Woher eine Kante kommt (für das Verwerfen doppelter Vorkommen).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// CDSR ohne Transformation.
    PlainOccurrence,
    Mapped,
    Other,
}

/// Eine Kante zwischen Darstellungen (vor dem Zusammenfassen zu Gruppen).
struct RawEdge {
    parent: u64,
    child: u64,
    /// Ordnung unter Geschwistern: Zeile und Nummer des bestimmenden Eintrags.
    key: (usize, u64),
    kind: Kind,
    transform: Placed,
    /// Richtung angenommen (siehe [`guessed_note`]).
    guessed: bool,
}

/// Eine Kante zwischen Gruppen.
struct Edge {
    child: usize,
    key: (usize, u64),
    transform: Placed,
    guessed: bool,
}

/// Ein Vorkommen eines Körpers vor der Namensvergabe.
struct Hit {
    rep: Option<usize>,
    items: Vec<u64>,
    placement: Placed,
    /// Auf dem Weg wurde eine Richtung angenommen.
    guessed: bool,
}

/// Vorkommen hinter einem CDSR: Eltern- und Kind-Produktdefinition, kein Vorkommen (`None`) oder
/// Grund, weshalb es sich nicht lesen ließ.
type Usage = Result<Option<(u64, u64)>, String>;

/// Alle Körper (Vorkommen) der Datei; `units_of(context)` liefert die (ggf. umgestellten)
/// Einheiten je Kontext.
pub fn find_bodies(
    file: &StepFile,
    units_of: &dyn Fn(Option<u64>) -> Result<Units, StepReadError>,
) -> Result<Vec<FoundBody>, StepReadError> {
    let mut solids: Vec<u64> = SOLID_TYPES
        .iter()
        .flat_map(|t| file.ids_of(t).iter().copied())
        .collect();
    solids.sort_unstable();
    solids.dedup();
    if solids.is_empty() {
        return Err(StepReadError::NoBodies);
    }
    let mut finder = Finder::new(file, units_of, &solids);
    finder.collect()?;
    finder.search()
}

/// Sammelt Darstellungen, Gruppen und Kanten und sucht die Körper.
struct Finder<'a> {
    file: &'a StepFile,
    units_of: &'a dyn Fn(Option<u64>) -> Result<Units, StepReadError>,
    solid_set: HashSet<u64>,
    solids: &'a [u64],
    reps: Vec<Rep>,
    rep_index: HashMap<u64, usize>,
    /// Darstellung → Produktdefinition (erste `SHAPE_DEFINITION_REPRESENTATION`).
    rep_pd: HashMap<u64, u64>,
    /// Produktdefinition → ihre Darstellungen (geordnet, damit alles bestimmt bleibt).
    pd_reps: BTreeMap<u64, Vec<u64>>,
    /// Gleiche Räume (Beziehungen ohne Transformation und ohne Vorkommen).
    links: Vec<(u64, u64)>,
    edges: Vec<RawEdge>,
    /// Noch zu lesende `MAPPED_ITEM`s: (Darstellung, Eintrag).
    pending: Vec<(u64, u64)>,
    units: HashMap<Option<u64>, Result<Units, String>>,
    /// Erster Grund, weshalb sich eine Darstellung nicht lesen ließ (sie könnte freie
    /// Volumen-Einträge enthalten haben).
    broken_rep: Option<String>,
    /// Darstellungen, an denen eine Beziehung mit unlesbarem anderem Ende hängt: Hängen unter
    /// ihnen Körper, wird die Datei mit dem Fehler abgelehnt.
    doubtful: Vec<(u64, StepReadError)>,
}

impl<'a> Finder<'a> {
    fn new(
        file: &'a StepFile,
        units_of: &'a dyn Fn(Option<u64>) -> Result<Units, StepReadError>,
        solids: &'a [u64],
    ) -> Finder<'a> {
        Finder {
            file,
            units_of,
            solid_set: solids.iter().copied().collect(),
            solids,
            reps: Vec::new(),
            rep_index: HashMap::new(),
            rep_pd: HashMap::new(),
            pd_reps: BTreeMap::new(),
            links: Vec::new(),
            edges: Vec::new(),
            pending: Vec::new(),
            units: HashMap::new(),
            broken_rep: None,
            doubtful: Vec::new(),
        }
    }

    fn line(&self, id: u64) -> usize {
        self.file.line(id).unwrap_or(0)
    }

    /// Ordnung in der Datei.
    fn key(&self, id: u64) -> (usize, u64) {
        (self.line(id), id)
    }

    /// Fehler, mit dem die Datei abgelehnt wird, weil `#id` nicht lesbar ist: der Fehler der
    /// Verweisprüfung (etwa `MissingRef`) oder `fallback`.
    fn refusal(&self, id: u64, fallback: impl Into<String>) -> StepReadError {
        match self.file.entity(id) {
            Err(e) => e,
            Ok(_) => self.file.fail(id, fallback),
        }
    }

    /// Liest die Darstellung `#id` (ohne Verweisprüfung): Name, Einträge, Kontext.
    fn read_rep(&self, id: u64) -> Result<Rep, StepReadError> {
        let e = self.file.entity_unchecked(id)?;
        let Some(args) = rep_args(&e) else {
            return Err(self.file.fail(id, tr!("step-r-expected-representation")));
        };
        let name = args[0].text().unwrap_or("").to_string();
        let items = args[1].refs().map_err(|m| self.file.fail(id, m))?;
        let context = match &args[2] {
            Value::Ref(c) => Some(*c),
            _ => None,
        };
        let solids: Vec<u64> = items
            .iter()
            .copied()
            .filter(|i| self.solid_set.contains(i))
            .collect();
        Ok(Rep {
            id,
            name,
            items,
            solids,
            context,
        })
    }

    /// Nimmt eine gelesene Darstellung auf (einmal) und merkt ihre `MAPPED_ITEM`s vor.
    fn register(&mut self, rep: Rep) -> usize {
        if let Some(&i) = self.rep_index.get(&rep.id) {
            return i;
        }
        let mapped = self.file.ids_of("MAPPED_ITEM");
        self.pending.extend(
            rep.items
                .iter()
                .filter(|item| mapped.binary_search(item).is_ok())
                .map(|&item| (rep.id, item)),
        );
        let index = self.reps.len();
        self.rep_index.insert(rep.id, index);
        self.reps.push(rep);
        index
    }

    /// Lädt die Darstellung `#id` (einmal); `Err`, wenn es sie nicht gibt oder sie keine ist.
    fn load_rep(&mut self, id: u64) -> Result<usize, StepReadError> {
        if let Some(&i) = self.rep_index.get(&id) {
            return Ok(i);
        }
        let rep = self.read_rep(id)?;
        Ok(self.register(rep))
    }

    /// Liest alle vorgemerkten `MAPPED_ITEM`s (ohne Rekursion über Ketten von Abbildungen).
    fn drain_mapped(&mut self) -> Result<(), StepReadError> {
        while let Some((parent, item)) = self.pending.pop() {
            self.mapped_item(parent, item)?;
        }
        Ok(())
    }

    /// `MAPPED_ITEM(name, REPRESENTATION_MAP(origin, rep), target)` in der Darstellung `parent`.
    /// Unlesbare Abbildung → ganze Datei abgelehnt (welcher Körper an ihr hängt, ist unbekannt);
    /// fehlt die abgebildete Darstellung, hängt nichts an ihr.
    fn mapped_item(&mut self, parent: u64, item: u64) -> Result<(), StepReadError> {
        const MI: &str = "MAPPED_ITEM";
        const RM: &str = "REPRESENTATION_MAP";
        let refs =
            |e: &Entity, t: &str, i: usize| e.arg(t, i).ok().and_then(|v| v.reference().ok());
        let e = self.file.entity_unchecked(item)?;
        let (Some(source), Some(target)) = (refs(&e, MI, 1), refs(&e, MI, 2)) else {
            return Err(self.refusal(item, tr!("step-r-mapping-unreadable")));
        };
        let map = self
            .file
            .entity_unchecked(source)
            .map_err(|_| self.refusal(item, tr!("step-r-mapping-unreadable")))?;
        let (Some(origin), Some(child)) = (refs(&map, RM, 0), refs(&map, RM, 1)) else {
            return Err(self.refusal(source, tr!("step-r-mapping-unreadable")));
        };
        if self.file.line(child).is_none() {
            return Ok(());
        }
        let Ok(ci) = self.load_rep(child) else {
            // Keine Darstellung: Sie trägt keine bekannten Körper.
            return Ok(());
        };
        let pi = self.rep_index[&parent];
        let (pctx, cctx) = (self.reps[pi].context, self.reps[ci].context);
        let transform = match (
            self.item_matrix(target, pctx),
            self.item_matrix(origin, cctx),
        ) {
            (Ok(t), Ok(o)) => Ok(t * rigid_inverse(o)),
            (Err(r), _) | (_, Err(r)) => Err(r),
        };
        let key = self.key(item);
        self.edges.push(RawEdge {
            parent,
            child,
            key,
            kind: Kind::Mapped,
            transform,
            guessed: false,
        });
        Ok(())
    }

    /// Einheiten eines Kontexts (zwischengespeichert); Fehler als Grund.
    fn units(&mut self, context: Option<u64>) -> Result<Units, String> {
        if let Some(u) = self.units.get(&context) {
            return u.clone();
        }
        let u = (self.units_of)(context).map_err(|e| reason(&e.to_string()));
        self.units.insert(context, u.clone());
        u
    }

    /// Lage `#id` (`AXIS2_PLACEMENT_3D` oder ein Operator) in den Einheiten von `context`, als
    /// Bewegung in mm.
    fn item_matrix(&mut self, id: u64, context: Option<u64>) -> Placed {
        let units = self.units(context)?;
        let e = self.file.entity(id).map_err(|e| reason(&e.to_string()))?;
        let mut res = Resolver::new(self.file, units);
        if e.is("AXIS2_PLACEMENT_3D") {
            let f = res.placement(id).map_err(|Skip(m)| reason(&m))?;
            return Ok(frame_matrix(&f));
        }
        if is_operator(&e) {
            return operator_matrix(self.file, &mut res, id, &e);
        }
        Err(tr!("step-r-unknown-placement", type = type_name(&e)))
    }

    /// Liest Darstellungen mit Körpern oder Abbildungen, Produkte, Vorkommen und Beziehungen.
    fn collect(&mut self) -> Result<(), StepReadError> {
        let mut rep_ids: Vec<u64> = REP_TYPES
            .iter()
            .flat_map(|t| self.file.ids_of(t).iter().copied())
            .collect();
        rep_ids.sort_unstable();
        rep_ids.dedup();
        let mapped = self.file.ids_of("MAPPED_ITEM");
        for id in rep_ids {
            match self.read_rep(id) {
                Ok(rep) => {
                    let relevant = !rep.solids.is_empty()
                        || rep.items.iter().any(|i| mapped.binary_search(i).is_ok());
                    if relevant {
                        self.register(rep);
                    }
                }
                Err(e) => {
                    if self.broken_rep.is_none() {
                        self.broken_rep = Some(reason(&e.to_string()));
                    }
                }
            }
        }
        self.drain_mapped()?;
        self.products();
        // Vorkommen: Beziehung → [(CDSR, Vorkommen)].
        let mut occurrences: HashMap<u64, Vec<(u64, Usage)>> = HashMap::new();
        let mut rels: Vec<u64> = Vec::new();
        for &cdsr in self.file.ids_of("CONTEXT_DEPENDENT_SHAPE_REPRESENTATION") {
            const C: &str = "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION";
            let e = self.file.entity_unchecked(cdsr)?;
            let (Ok(Value::Ref(rr)), Ok(Value::Ref(pds))) = (e.arg(C, 0), e.arg(C, 1)) else {
                return Err(self.refusal(cdsr, tr!("step-r-occurrence-unreadable")));
            };
            let usage = self.usage_of(*pds);
            if self.relation_ends(*rr).is_none() {
                // Ohne lesbare Beziehung: Hat das Kind Körper, läge es still am Ursprung.
                if let Ok(Some((_, child))) = usage {
                    let err = self.refusal(cdsr, tr!("step-r-occurrence-relation"));
                    for rep in self.pd_reps.get(&child).cloned().unwrap_or_default() {
                        self.doubtful.push((rep, err.clone()));
                    }
                }
                continue;
            }
            occurrences.entry(*rr).or_default().push((cdsr, usage));
            rels.push(*rr);
        }
        rels.extend(
            RELATIONSHIP_TYPES
                .iter()
                .flat_map(|t| self.file.ids_of(t).iter().copied()),
        );
        rels.sort_unstable();
        rels.dedup();
        for rr in rels {
            let occ = occurrences.remove(&rr).unwrap_or_default();
            self.relationship(rr, occ)?;
            self.drain_mapped()?;
        }
        Ok(())
    }

    /// Produktdefinitionen der Darstellungen (`SHAPE_DEFINITION_REPRESENTATION`); fehlerhafte
    /// Einträge werden übergangen (sie betreffen nur Namen und die Richtungsprüfung, die dann
    /// „Richtung unklar“ meldet).
    fn products(&mut self) {
        for &sdr in self.file.ids_of("SHAPE_DEFINITION_REPRESENTATION") {
            let Ok(e) = self.file.entity(sdr) else {
                continue;
            };
            const S: &str = "SHAPE_DEFINITION_REPRESENTATION";
            let (Ok(Value::Ref(pds)), Ok(Value::Ref(rep))) = (e.arg(S, 0), e.arg(S, 1)) else {
                continue;
            };
            let Some(pd) = self.definition_of(*pds) else {
                continue;
            };
            self.rep_pd.entry(*rep).or_insert(pd);
            self.pd_reps.entry(pd).or_default().push(*rep);
        }
    }

    /// `definition` eines `PRODUCT_DEFINITION_SHAPE` (oder `PROPERTY_DEFINITION`).
    fn definition_of(&self, pds: u64) -> Option<u64> {
        let e = self.file.entity(pds).ok()?;
        let args = e
            .part("PRODUCT_DEFINITION_SHAPE")
            .or_else(|| e.part("PROPERTY_DEFINITION"))?;
        args.get(2)?.reference().ok()
    }

    /// Eltern- und Kind-Produktdefinition des Vorkommens hinter `pds`; `Ok(None)`, wenn die
    /// Definition kein Vorkommen ist; `Err(Grund)`, wenn sie sich nicht lesen lässt.
    fn usage_of(&self, pds: u64) -> Usage {
        let e = self.file.entity(pds).map_err(|e| reason(&e.to_string()))?;
        let Some(def) = e
            .part("PRODUCT_DEFINITION_SHAPE")
            .or_else(|| e.part("PROPERTY_DEFINITION"))
            .and_then(|a| a.get(2))
            .and_then(|v| v.reference().ok())
        else {
            return Err(reason(
                &self
                    .file
                    .fail(pds, tr!("step-r-definition-missing"))
                    .to_string(),
            ));
        };
        let u = self.file.entity(def).map_err(|e| reason(&e.to_string()))?;
        for t in USAGE_TYPES {
            if let Some(args) = u.part(t)
                && let (Some(Value::Ref(parent)), Some(Value::Ref(child))) =
                    (args.get(3), args.get(4))
            {
                return Ok(Some((*parent, *child)));
            }
        }
        Ok(None)
    }

    /// `rep_1`, `rep_2` und Transformation (falls genannt) der Beziehung `#rr`, ohne
    /// Verweisprüfung; `None`, wenn sich das nicht lesen lässt.
    fn relation_ends(&self, rr: u64) -> Option<(u64, u64, Option<u64>)> {
        let e = self.file.entity_unchecked(rr).ok()?;
        let args = e
            .part("REPRESENTATION_RELATIONSHIP")
            .filter(|a| a.len() >= 4)
            .or_else(|| match &e {
                Entity::Simple(r) if r.args.len() >= 4 => Some(&r.args[..]),
                _ => None,
            })?;
        let rep_1 = args[2].reference().ok()?;
        let rep_2 = args[3].reference().ok()?;
        let op = match e.part("REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION") {
            Some(a) if a.len() == 1 => Some(a[0].reference().ok()?),
            Some(a) if a.len() >= 5 => Some(a[4].reference().ok()?),
            Some(_) => return None,
            None => None,
        };
        Some((rep_1, rep_2, op))
    }

    /// Eine Beziehung zwischen Darstellungen mit ihren Vorkommen (CDSR) `occ`.
    fn relationship(&mut self, rr: u64, occ: Vec<(u64, Usage)>) -> Result<(), StepReadError> {
        let Some((rep_1, rep_2, op)) = self.relation_ends(rr) else {
            // Beide Endpunkte unbekannt: Sie betrifft keinen bekannten Körper … außer sie gehört
            // zu einem Vorkommen (dann wäre sie oben schon aufgefallen).
            return Ok(());
        };
        let end_1 = self.load_rep(rep_1);
        let end_2 = self.load_rep(rep_2);
        match (&end_1, &end_2) {
            (Ok(_), Ok(_)) => {}
            (Ok(_), Err(e)) | (Err(e), Ok(_)) => {
                // Ein Ende fehlt: Hängen unter dem anderen Körper, läge einer still falsch.
                let ok = if end_1.is_ok() { rep_1 } else { rep_2 };
                let err = match self.file.entity(rr) {
                    Err(re) => re,
                    Ok(_) => e.clone(),
                };
                self.doubtful.push((ok, err));
                return Ok(());
            }
            (Err(_), Err(_)) => return Ok(()),
        }
        if occ.is_empty() {
            let Some(op) = op else {
                self.links.push((rep_1, rep_2));
                return Ok(());
            };
            // Ohne Vorkommen: Eltern ist die Darstellung, die als einzige zu einem Produkt
            // gehört, sonst `rep_2` (ISO).
            let swapped = self.rep_pd.contains_key(&rep_1) && !self.rep_pd.contains_key(&rep_2);
            let transform = self.relation_transform(op, rep_1, rep_2, swapped);
            let key = self.key(rr);
            self.push_edge(rep_1, rep_2, swapped, key, Kind::Other, transform, true);
            return Ok(());
        }
        for (cdsr, usage) in occ {
            let key = self.key(cdsr);
            let swapped = match usage {
                Ok(Some((ppd, cpd))) => self.direction(rep_1, rep_2, ppd, cpd).ok_or(unclear()),
                Ok(None) => Err(unclear()),
                Err(why) => Err(format!("{} – {why}", unclear())),
            };
            let swapped = match swapped {
                Ok(s) => s,
                Err(why) => {
                    self.push_edge(rep_1, rep_2, false, key, Kind::Other, Err(why), false);
                    continue;
                }
            };
            let (kind, transform) = match op {
                None => (Kind::PlainOccurrence, Ok(DMat4::IDENTITY)),
                Some(op) => (
                    Kind::Other,
                    self.relation_transform(op, rep_1, rep_2, swapped),
                ),
            };
            self.push_edge(rep_1, rep_2, swapped, key, kind, transform, false);
        }
        Ok(())
    }

    /// Kante der Beziehung (`rep_1`, `rep_2`); `swapped`: `rep_1` ist das Elternteil;
    /// `guessed`: diese Richtung ist angenommen.
    #[allow(clippy::too_many_arguments)]
    fn push_edge(
        &mut self,
        rep_1: u64,
        rep_2: u64,
        swapped: bool,
        key: (usize, u64),
        kind: Kind,
        transform: Placed,
        guessed: bool,
    ) {
        let (parent, child) = if swapped {
            (rep_1, rep_2)
        } else {
            (rep_2, rep_1)
        };
        self.edges.push(RawEdge {
            parent,
            child,
            key,
            kind,
            transform,
            guessed,
        });
    }

    /// Richtung einer Beziehung über das Vorkommen: `Some(false)`: `rep_1` ist das Kind,
    /// `Some(true)`: vertauscht, `None`: unklar.
    fn direction(&self, rep_1: u64, rep_2: u64, parent_pd: u64, child_pd: u64) -> Option<bool> {
        let belongs = |rep: u64, pd: u64| {
            self.pd_reps
                .get(&pd)
                .is_some_and(|reps| reps.contains(&rep))
        };
        let (c1, c2) = (belongs(rep_1, child_pd), belongs(rep_2, child_pd));
        if c1 != c2 {
            return Some(c2);
        }
        let (p1, p2) = (belongs(rep_1, parent_pd), belongs(rep_2, parent_pd));
        if p1 != p2 {
            return Some(p1);
        }
        None
    }

    /// Transformation von Kind- in Elternkoordinaten aus dem Operator `op` der Beziehung
    /// (`rep_1`, `rep_2`); `swapped`: `rep_1` ist das Elternteil.
    fn relation_transform(&mut self, op: u64, rep_1: u64, rep_2: u64, swapped: bool) -> Placed {
        let r1 = self.rep_index[&rep_1];
        let r2 = self.rep_index[&rep_2];
        let (ctx_1, ctx_2) = (self.reps[r1].context, self.reps[r2].context);
        let e = self.file.entity(op).map_err(|e| reason(&e.to_string()))?;
        if let Some(args) = e.part("ITEM_DEFINED_TRANSFORMATION") {
            let (Some(Value::Ref(it_1)), Some(Value::Ref(it_2))) = (args.get(2), args.get(3))
            else {
                return Err(reason(
                    &self
                        .file
                        .fail(op, tr!("step-r-transform-position"))
                        .to_string(),
                ));
            };
            // ISO: `it_1` gehört zu `rep_1`, `it_2` zu `rep_2`; die `items` der Darstellungen
            // entscheiden, wenn sie es anders sagen.
            let (it_1, it_2) =
                match item_order(*it_1, *it_2, &self.reps[r1].items, &self.reps[r2].items) {
                    Some(true) => (*it_1, *it_2),
                    Some(false) => (*it_2, *it_1),
                    None => return Err(unclear()),
                };
            let a_1 = self.item_matrix(it_1, ctx_1)?;
            let a_2 = self.item_matrix(it_2, ctx_2)?;
            let (parent, child) = if swapped { (a_1, a_2) } else { (a_2, a_1) };
            return Ok(parent * rigid_inverse(child));
        }
        if is_operator(&e) {
            // Bildet `rep_1` auf `rep_2` ab; der Ursprung steht in den Einheiten von `rep_2`.
            let m = self.item_matrix(op, ctx_2)?;
            return Ok(if swapped { rigid_inverse(m) } else { m });
        }
        Err(tr!("step-r-unknown-placement", type = type_name(&e)))
    }

    /// Name des Produkts einer Darstellung (P10), getrimmt; leer, wenn keins.
    fn product_name(&self, rep: u64) -> Option<String> {
        let pd = *self.rep_pd.get(&rep)?;
        let pd_e = self.file.entity(pd).ok()?;
        let formation = args_starting(&pd_e, "PRODUCT_DEFINITION")?
            .get(2)?
            .reference()
            .ok()?;
        let f_e = self.file.entity(formation).ok()?;
        let product = args_starting(&f_e, "PRODUCT_DEFINITION_FORMATION")?
            .get(2)?
            .reference()
            .ok()?;
        let p_e = self.file.entity(product).ok()?;
        let name = p_e.part("PRODUCT")?.get(1)?.text().ok()?;
        Some(name.to_string())
    }

    /// Rohname eines Körpers aus der Darstellung `rep` (Gruppe `group`): Produkt der
    /// Darstellung, sonst Produkt der Gruppe, sonst Name der Darstellung, sonst des ersten
    /// Volumen-Eintrags.
    fn raw_name(&self, rep: usize, group_members: &[usize], items: &[u64]) -> String {
        let r = &self.reps[rep];
        // Der Reihe nach, erst bei Bedarf gelesen (eine Gruppe kann viele Darstellungen haben).
        let products = std::iter::once(r.id)
            .chain(group_members.iter().map(|&m| self.reps[m].id))
            .filter(|id| self.rep_pd.contains_key(id))
            .filter_map(|id| self.product_name(id));
        let rest = std::iter::once_with(|| r.name.clone())
            .chain(items.first().map(|&s| self.solid_name(s)));
        products
            .chain(rest)
            .map(|n| clean_name(&n))
            .find(|n| !n.is_empty())
            .unwrap_or_default()
    }

    fn solid_name(&self, solid: u64) -> String {
        self.file
            .entity(solid)
            .ok()
            .and_then(|e| {
                SOLID_TYPES
                    .iter()
                    .find_map(|t| e.part(t))
                    .and_then(|a| a.first())
                    .and_then(|v| v.text().ok())
                    .map(str::to_string)
            })
            .unwrap_or_default()
    }

    /// Legt jede nicht platzierte Darstellung einer Produktdefinition in die Gruppe der ersten
    /// platzierten derselben Definition (gleicher Raum des Teils). Platziert = eine Kante von
    /// außerhalb der Produktdefinition führt hinein.
    fn join_part_spaces(&self, uf: &mut [usize]) {
        let mut placed_roots: HashSet<usize> = HashSet::new();
        for e in &self.edges {
            let same_pd = matches!(
                (self.rep_pd.get(&e.parent), self.rep_pd.get(&e.child)),
                (Some(a), Some(b)) if a == b
            );
            if !same_pd {
                let c = find(uf, self.rep_index[&e.child]);
                placed_roots.insert(c);
            }
        }
        for reps in self.pd_reps.values() {
            let mut roots: Vec<usize> = reps
                .iter()
                .filter_map(|r| self.rep_index.get(r))
                .map(|&i| find(uf, i))
                .collect();
            roots.dedup();
            let Some(&placed) = roots.iter().find(|r| placed_roots.contains(r)) else {
                continue;
            };
            for &r in &roots {
                let (a, b) = (find(uf, r), find(uf, placed));
                // Die platzierte Wurzel bleibt Wurzel (sonst stimmte `placed_roots` nicht mehr).
                if a != b && !placed_roots.contains(&r) {
                    uf[a] = b;
                }
            }
        }
    }

    /// Gruppen, Kreise, Tiefensuche, Namen.
    fn search(&mut self) -> Result<Vec<FoundBody>, StepReadError> {
        let n = self.reps.len();
        // Gruppen gleicher Räume.
        let mut uf: Vec<usize> = (0..n).collect();
        for &(a, b) in &self.links {
            let (a, b) = (
                find(&mut uf, self.rep_index[&a]),
                find(&mut uf, self.rep_index[&b]),
            );
            if a != b {
                uf[a.max(b)] = a.min(b);
            }
        }
        self.join_part_spaces(&mut uf);
        let mut group_of = vec![usize::MAX; n];
        let mut members: Vec<Vec<usize>> = Vec::new();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| self.key(self.reps[i].id));
        let mut root_group: HashMap<usize, usize> = HashMap::new();
        for &i in &order {
            let r = find(&mut uf, i);
            let g = *root_group.entry(r).or_insert_with(|| {
                members.push(Vec::new());
                members.len() - 1
            });
            group_of[i] = g;
            members[g].push(i);
        }
        let groups = members.len();
        // Körper je Gruppe: jeder Volumen-Eintrag einmal (Darstellungen in Dateireihenfolge).
        let bodies: Vec<Vec<(usize, Vec<u64>)>> = members
            .iter()
            .map(|m| {
                let mut seen: HashSet<u64> = HashSet::new();
                m.iter()
                    .filter_map(|&r| {
                        let items: Vec<u64> = self.reps[r]
                            .solids
                            .iter()
                            .copied()
                            .filter(|s| seen.insert(*s))
                            .collect();
                        (!items.is_empty()).then_some((r, items))
                    })
                    .collect()
            })
            .collect();
        // Kanten zwischen Gruppen; ein Vorkommen ohne Transformation entfällt, wenn eine
        // Abbildung dieselben Gruppen verbindet.
        let edges = std::mem::take(&mut self.edges);
        let pair = |e: &RawEdge| {
            (
                group_of[self.rep_index[&e.parent]],
                group_of[self.rep_index[&e.child]],
            )
        };
        let mapped_pairs: HashSet<(usize, usize)> = edges
            .iter()
            .filter(|e| e.kind == Kind::Mapped)
            .map(pair)
            .collect();
        let mut children: Vec<Vec<Edge>> = (0..groups).map(|_| Vec::new()).collect();
        let mut incoming = vec![false; groups];
        for e in edges {
            let (p, c) = pair(&e);
            if e.kind == Kind::PlainOccurrence && mapped_pairs.contains(&(p, c)) {
                continue;
            }
            incoming[c] = true;
            children[p].push(Edge {
                child: c,
                key: e.key,
                transform: e.transform,
                guessed: e.guessed,
            });
        }
        for list in &mut children {
            list.sort_by_key(|e| e.key);
        }
        let succ: Vec<Vec<usize>> = children
            .iter()
            .map(|l| l.iter().map(|e| e.child).collect())
            .collect();
        let cyclic = cyclic_nodes(&succ);
        let own: Vec<usize> = bodies.iter().map(Vec::len).collect();
        let cap = self.file.limits().max_bodies + 1;
        let count = subtree_counts(&succ, &own, &cyclic, cap);
        // Beziehungen mit unlesbarem Ende an Darstellungen, unter denen Körper hängen.
        for (rep, err) in std::mem::take(&mut self.doubtful) {
            if let Some(&i) = self.rep_index.get(&rep) {
                let g = group_of[i];
                if count[g] > 0 || cyclic[g] {
                    return Err(err);
                }
            }
        }

        let mut walk = Walk {
            bodies: &bodies,
            children: &children,
            cyclic: &cyclic,
            count: &count,
            reached: vec![false; groups],
            hits: Vec::new(),
            max: self.file.limits().max_bodies,
        };
        // Wurzeln in Dateireihenfolge (Gruppen sind nach ihrer ersten Darstellung geordnet).
        for g in 0..groups {
            if !incoming[g] && count[g] > 0 {
                walk.visit(g, Ok(DMat4::IDENTITY), false, 0)?;
            }
        }
        // Nicht erreichte Darstellungen: nur über Kreise erreichbar.
        for g in 0..groups {
            if !walk.reached[g] {
                walk.emit(g, &Err(cycle()), false)?;
            }
        }
        // Freie Volumen-Einträge; ließ sich eine Darstellung nicht lesen, könnten sie in ihr
        // gestanden haben (dann ist ihre Lage unbekannt).
        let in_rep: HashSet<u64> = self
            .reps
            .iter()
            .flat_map(|r| r.solids.iter().copied())
            .collect();
        let free: Placed = match &self.broken_rep {
            Some(why) => Err(why.clone()),
            None => Ok(DMat4::IDENTITY),
        };
        for &s in self.solids {
            if !in_rep.contains(&s) {
                walk.push(Hit {
                    rep: None,
                    items: vec![s],
                    placement: free.clone(),
                    guessed: false,
                })?;
            }
        }
        let hits = walk.hits;

        let mut used: HashSet<String> = HashSet::new();
        let mut out = Vec::with_capacity(hits.len());
        for h in hits {
            let (raw, context) = match h.rep {
                Some(r) => (
                    self.raw_name(r, &members[group_of[r]], &h.items),
                    self.reps[r].context,
                ),
                None => (
                    clean_name(
                        &h.items
                            .first()
                            .map_or_else(String::new, |&s| self.solid_name(s)),
                    ),
                    None,
                ),
            };
            let name = unique_name(raw, &mut used);
            let placement_guessed = h.guessed && h.placement.is_ok();
            let (placement, placement_note) = match h.placement {
                Ok(m) => (m, None),
                Err(why) => (DMat4::IDENTITY, Some(placement_note(&name, &why))),
            };
            out.push(FoundBody {
                name,
                items: h.items,
                context,
                placement,
                placement_note,
                placement_guessed,
            });
        }
        Ok(out)
    }
}

/// Zuordnung der Lagen einer `ITEM_DEFINED_TRANSFORMATION` zu `rep_1`/`rep_2` über die `items`
/// der Darstellungen: `Some(true)` wie geschrieben (ISO), `Some(false)` gekreuzt, `None`
/// widersprüchlich. Eine Lage, die in keiner oder in beiden Darstellungen steht, entscheidet
/// nichts.
fn item_order(it_1: u64, it_2: u64, items_1: &[u64], items_2: &[u64]) -> Option<bool> {
    let side = |it: u64| match (items_1.contains(&it), items_2.contains(&it)) {
        (true, false) => Some(1),
        (false, true) => Some(2),
        _ => None,
    };
    let (s1, s2) = (side(it_1), side(it_2));
    let straight = s1 != Some(2) && s2 != Some(1);
    let crossed = s1 != Some(1) && s2 != Some(2);
    if straight {
        Some(true)
    } else if crossed {
        Some(false)
    } else {
        None
    }
}

/// Tiefensuche über die Gruppen.
struct Walk<'w> {
    bodies: &'w [Vec<(usize, Vec<u64>)>],
    children: &'w [Vec<Edge>],
    cyclic: &'w [bool],
    count: &'w [usize],
    reached: Vec<bool>,
    hits: Vec<Hit>,
    max: usize,
}

impl Walk<'_> {
    fn push(&mut self, h: Hit) -> Result<(), StepReadError> {
        self.hits.push(h);
        if self.hits.len() > self.max {
            return Err(StepReadError::TooManyBodies);
        }
        Ok(())
    }

    /// Körper der Gruppe `g` mit der Lage `m`.
    fn emit(&mut self, g: usize, m: &Placed, guessed: bool) -> Result<(), StepReadError> {
        self.reached[g] = true;
        let bodies = self.bodies;
        for (r, items) in &bodies[g] {
            self.push(Hit {
                rep: Some(*r),
                items: items.clone(),
                placement: m.clone(),
                guessed,
            })?;
        }
        Ok(())
    }

    /// Besucht `g` in der Tiefe `depth` (Zahl der Vorkommen ab der Wurzel) mit der Lage `m`;
    /// `guessed`: auf dem Weg wurde eine Richtung angenommen.
    fn visit(
        &mut self,
        g: usize,
        m: Placed,
        guessed: bool,
        depth: usize,
    ) -> Result<(), StepReadError> {
        self.emit(g, &m, guessed)?;
        let children = self.children;
        for e in &children[g] {
            if self.cyclic[e.child] || self.count[e.child] == 0 {
                continue;
            }
            if depth + 1 > MAX_ASSEMBLY_DEPTH {
                self.emit_below(e.child, &too_deep())?;
                continue;
            }
            let next = match (&m, &e.transform) {
                (Ok(a), Ok(t)) => Ok(*a * *t),
                (Err(r), _) | (Ok(_), Err(r)) => Err(r.clone()),
            };
            self.visit(e.child, next, guessed || e.guessed, depth + 1)?;
        }
        Ok(())
    }

    /// Alle Körper ab `g` (ohne Kreise) je einmal mit dem Grund `why`.
    fn emit_below(&mut self, g: usize, why: &str) -> Result<(), StepReadError> {
        let mut seen = HashSet::from([g]);
        let mut todo = vec![g];
        let why: Placed = Err(why.into());
        while let Some(v) = todo.pop() {
            self.emit(v, &why, false)?;
            for e in &self.children[v] {
                if !self.cyclic[e.child] && seen.insert(e.child) {
                    todo.push(e.child);
                }
            }
        }
        Ok(())
    }
}

/// Wurzel in der Union-Find-Struktur (mit Pfadverkürzung, ohne Rekursion).
fn find(uf: &mut [usize], mut i: usize) -> usize {
    while uf[i] != i {
        uf[i] = uf[uf[i]];
        i = uf[i];
    }
    i
}

/// Knoten in einem Kreis (starke Zusammenhangskomponente mit mehr als einem Knoten oder
/// Schleife auf sich selbst); Tarjan ohne Rekursion.
fn cyclic_nodes(succ: &[Vec<usize>]) -> Vec<bool> {
    let n = succ.len();
    let mut index = vec![usize::MAX; n];
    let mut low = vec![0; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut out = vec![false; n];
    let mut next = 0;
    for s in 0..n {
        if index[s] != usize::MAX {
            continue;
        }
        let mut calls: Vec<(usize, usize)> = vec![(s, 0)];
        index[s] = next;
        low[s] = next;
        next += 1;
        stack.push(s);
        on_stack[s] = true;
        while let Some(top) = calls.len().checked_sub(1) {
            let (v, i) = calls[top];
            if i < succ[v].len() {
                calls[top].1 += 1;
                let w = succ[v][i];
                if index[w] == usize::MAX {
                    index[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    calls.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            calls.pop();
            if let Some(&(u, _)) = calls.last() {
                low[u] = low[u].min(low[v]);
            }
            if low[v] == index[v] {
                let mut comp = Vec::new();
                while let Some(w) = stack.pop() {
                    on_stack[w] = false;
                    comp.push(w);
                    if w == v {
                        break;
                    }
                }
                if comp.len() > 1 {
                    for w in comp {
                        out[w] = true;
                    }
                }
            }
        }
    }
    for (v, s) in succ.iter().enumerate() {
        if s.contains(&v) {
            out[v] = true;
        }
    }
    out
}

/// Zahl der Körper-Vorkommen unter jeder Gruppe (ohne Kreise; bei `cap` gekappt). Die Gruppen
/// ohne Kreis bilden einen Graphen ohne Kreis; gezählt wird von unten (ohne Rekursion). Eine
/// Gruppe, die gerade offen ist, wird nicht noch einmal betreten (Schutz, falls doch ein Kreis
/// übrig bliebe: die Suche endet immer).
fn subtree_counts(succ: &[Vec<usize>], own: &[usize], cyclic: &[bool], cap: usize) -> Vec<usize> {
    let n = succ.len();
    let mut count: Vec<Option<usize>> = vec![None; n];
    let mut open = vec![false; n];
    for s in 0..n {
        if cyclic[s] || count[s].is_some() {
            continue;
        }
        let mut calls: Vec<(usize, usize)> = vec![(s, 0)];
        open[s] = true;
        while let Some(top) = calls.len().checked_sub(1) {
            let (v, i) = calls[top];
            if i < succ[v].len() {
                calls[top].1 += 1;
                let w = succ[v][i];
                if !cyclic[w] && count[w].is_none() && !open[w] {
                    open[w] = true;
                    calls.push((w, 0));
                }
                continue;
            }
            calls.pop();
            open[v] = false;
            let total = succ[v]
                .iter()
                .filter(|&&w| !cyclic[w])
                .fold(own[v].min(cap), |acc, &w| {
                    acc.saturating_add(count[w].unwrap_or(0)).min(cap)
                });
            count[v] = Some(total);
        }
    }
    count.into_iter().map(|c| c.unwrap_or(0)).collect()
}

/// Parameter (Name, Einträge, Kontext) einer Darstellung, auch in zusammengesetzten Einträgen.
fn rep_args(e: &Entity) -> Option<&[Value]> {
    let ok = |a: &[Value]| a.len() >= 3 && matches!(a[1], Value::List(_));
    if let Some(a) = e.part("REPRESENTATION").filter(|a| ok(a)) {
        return Some(a);
    }
    e.names()
        .into_iter()
        .filter_map(|n| e.part(n))
        .find(|a| ok(a))
}

/// Parameter des ersten Teils, dessen Typ mit `prefix` beginnt.
fn args_starting<'e>(e: &'e Entity, prefix: &str) -> Option<&'e [Value]> {
    e.names()
        .into_iter()
        .filter(|n| n.starts_with(prefix))
        .find_map(|n| e.part(n))
}

/// Typname für Meldungen.
fn type_name(e: &Entity) -> String {
    e.names()
        .into_iter()
        .find(|n| !matches!(*n, "REPRESENTATION_ITEM" | "GEOMETRIC_REPRESENTATION_ITEM"))
        .unwrap_or("")
        .to_string()
}

/// Grund aus einer Meldung: ohne den Punkt am Ende (er steht in Klammern).
fn reason(msg: &str) -> String {
    msg.trim_end_matches('.').to_string()
}

fn frame_matrix(f: &Frame) -> DMat4 {
    DMat4::from_cols(
        f.x.extend(0.0),
        f.y.extend(0.0),
        f.z.extend(0.0),
        f.origin.extend(1.0),
    )
}

/// Kehrwert einer Bewegung (Drehung transponiert, Verschiebung zurückgedreht).
fn rigid_inverse(m: DMat4) -> DMat4 {
    let x = m.x_axis.truncate();
    let y = m.y_axis.truncate();
    let z = m.z_axis.truncate();
    let t = m.w_axis.truncate();
    DMat4::from_cols(
        DVec3::new(x.x, y.x, z.x).extend(0.0),
        DVec3::new(x.y, y.y, z.y).extend(0.0),
        DVec3::new(x.z, y.z, z.z).extend(0.0),
        DVec3::new(-x.dot(t), -y.dot(t), -z.dot(t)).extend(1.0),
    )
}

/// Ein Operator `CARTESIAN_TRANSFORMATION_OPERATOR_3D` (auch nicht gleichmäßig).
fn is_operator(e: &Entity) -> bool {
    e.is(CTO_3D) || e.is(CTO_NON_UNIFORM)
}

/// Parameter eines Operators: (axis1, axis2, local_origin, scale, axis3, scale2, scale3), die
/// letzten beiden nur beim nicht gleichmäßigen. Einfach (`…_3D(name, [description,] axis1,
/// axis2, origin, scale, axis3)`, nicht gleichmäßig mit `scale2, scale3` dahinter) oder
/// zusammengesetzt (`CARTESIAN_TRANSFORMATION_OPERATOR(axis1, axis2, origin, scale)
/// …_3D(axis3) [ …_NON_UNIFORM(scale2, scale3)]`).
fn operator_args(e: &Entity) -> Option<Vec<&Value>> {
    let unset = &Value::Unset;
    if let Entity::Simple(r) = e {
        let n = r.args.len();
        if e.is(CTO_NON_UNIFORM) && n >= 7 {
            return Some(r.args[n - 7..].iter().collect());
        }
        if e.is(CTO_3D) && n >= 5 {
            return Some(r.args[n - 5..].iter().chain([unset, unset]).collect());
        }
        return None;
    }
    let base = e.part("CARTESIAN_TRANSFORMATION_OPERATOR")?;
    let a3 = e.part(CTO_3D)?;
    if base.len() < 4 || a3.is_empty() {
        return None;
    }
    let mut out: Vec<&Value> = base[base.len() - 4..].iter().collect();
    out.push(&a3[a3.len() - 1]);
    match e.part(CTO_NON_UNIFORM) {
        Some(nu) if nu.len() >= 2 => out.extend(&nu[nu.len() - 2..]),
        Some(_) => return None,
        None => out.extend([unset, unset]),
    }
    Some(out)
}

/// Operator nach ISO 10303-42 (`base_axis`, `first_proj_axis`, `second_proj_axis`):
/// `z` = axis3 (Vorgabe (0, 0, 1)); `x` = axis1 senkrecht zu `z` gemacht (Vorgabe (1, 0, 0),
/// liegt `z` auf ±(1, 0, 0): (0, 1, 0)); `y` = axis2 (Vorgabe (0, 1, 0)!) senkrecht zu `z` und
/// `x` gemacht – ISO bildet `y` nicht als `z × x`, das System kann also linkshändig werden;
/// Bild `p ↦ origin + scale · (p.x x + p.y y + p.z z)` mit den Achsen nach ISO 10303-42
/// (`base_axis`: z = axis3 oder (0, 0, 1); x = `first_proj_axis` (axis1, ohne sie (1, 0, 0), bei
/// z = ±x (0, 1, 0)), senkrecht zu z projiziert; y = `second_proj_axis` (axis2, ohne sie
/// (0, 1, 0)), senkrecht zu z und x projiziert; kein Kreuzprodukt, das System darf linkshändig
/// sein). Gegen die veröffentlichte EXPRESS-Fassung geprüft (Schlussprüfung). Angenommen werden
/// nur Bewegungen; Abweichungen sind vorsichtig (Meldung, Körper am Ursprung, nie still anders):
/// Determinante < 0 → Spiegelung; Maßstab (auch scale2, scale3, Vorgabe = scale) weiter als
/// 1e-6 von 1 → Skalierung; gegebene Achsen bis zum Kosinus [`OBLIQUE_TOL`] (1e-4) nicht
/// senkrecht werden wie bei ISO projiziert, darüber oder entartet (ISO lässt das Ergebnis dann
/// offen) → schiefe Achsen. Ein negativer Maßstab bei linkshändigen Achsen ergibt zwar eine
/// Drehung, ist nach ISO (`scale > 0`) aber ungültig → Skalierung. Maßstäbe innerhalb 1e-6
/// werden als genau 1 genommen. Wo die ISO-Vorgabe linkshändig wird (etwa axis1 = (−1, 0, 0)
/// ohne axis2), melden wir „Spiegelung“ (andere Programme nehmen dort teils eine Drehung).
fn operator_matrix(file: &StepFile, res: &mut Resolver, id: u64, e: &Entity) -> Placed {
    let fail = |m: String| reason(&file.fail(id, m).to_string());
    let Some(args) = operator_args(e) else {
        return Err(fail(tr!("step-r-operator-params")));
    };
    let mut dir = |v: &Value| -> Result<Option<DVec3>, String> {
        if v.is_unset() {
            return Ok(None);
        }
        let r = v.reference().map_err(fail)?;
        res.direction(r).map(Some).map_err(|Skip(m)| reason(&m))
    };
    let axis1 = dir(args[0])?;
    let axis2 = dir(args[1])?;
    let axis3 = dir(args[4])?;
    let origin = args[2].reference().map_err(fail)?;
    let origin = res.point(origin).map_err(|Skip(m)| reason(&m))?;
    let number = |v: &Value, default: f64| -> Result<f64, String> {
        if v.is_unset() {
            Ok(default)
        } else {
            v.real().map_err(fail)
        }
    };
    let scale = number(args[3], 1.0)?;
    let scale2 = number(args[5], scale)?;
    let scale3 = number(args[6], scale)?;
    // base_axis: z, dann x (first_proj_axis), dann y (second_proj_axis).
    let z = axis3.unwrap_or(DVec3::Z);
    let v = match axis1 {
        Some(a) => {
            if a.dot(z).abs() > OBLIQUE_TOL {
                return Err(oblique());
            }
            a
        }
        None if (z - DVec3::X).length() <= DEGENERATE || (z + DVec3::X).length() <= DEGENERATE => {
            DVec3::Y
        }
        None => DVec3::X,
    };
    let x = v - z * v.dot(z);
    if x.length() <= DEGENERATE {
        return Err(oblique());
    }
    let x = x.normalize();
    let w = match axis2 {
        Some(a) => {
            if a.dot(z).abs() > OBLIQUE_TOL || a.dot(x).abs() > OBLIQUE_TOL {
                return Err(oblique());
            }
            a
        }
        None => DVec3::Y,
    };
    let y = w - z * w.dot(z) - x * w.dot(x);
    if y.length() <= DEGENERATE {
        return Err(oblique());
    }
    let y = y.normalize();
    let handed = x.cross(y).dot(z);
    let det = scale * scale2 * scale3 * handed;
    if !(det.is_finite() && origin.is_finite()) {
        return Err(scaling());
    }
    if det < 0.0 {
        return Err(mirror());
    }
    if [scale, scale2, scale3]
        .iter()
        .any(|s| (s - 1.0).abs() > SCALE_TOL)
    {
        return Err(scaling());
    }
    Ok(DMat4::from_cols(
        x.extend(0.0),
        y.extend(0.0),
        z.extend(0.0),
        origin.extend(1.0),
    ))
}

/// Name nach P10: Steuerzeichen → Leerzeichen, getrimmt, höchstens
/// [`MAX_BODY_NAME_CHARS`] Zeichen.
fn clean_name(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let s: String = s.trim().chars().take(MAX_BODY_NAME_CHARS).collect();
    s.trim_end().to_string()
}

/// Eindeutiger Name: der erste bleibt, weitere bekommen „ (2)“, „ (3)“ … (der Stamm wird so
/// gekürzt, dass alles höchstens [`MAX_BODY_NAME_CHARS`] Zeichen hat). Leere Namen bleiben leer.
fn unique_name(name: String, used: &mut HashSet<String>) -> String {
    if name.is_empty() {
        return name;
    }
    if used.insert(name.clone()) {
        return name;
    }
    let mut k = 2usize;
    loop {
        let suffix = format!(" ({k})");
        let room = MAX_BODY_NAME_CHARS.saturating_sub(suffix.chars().count());
        let stem: String = name.chars().take(room).collect();
        let candidate = format!("{}{suffix}", stem.trim_end());
        if used.insert(candidate.clone()) {
            return candidate;
        }
        k += 1;
    }
}
