//! Körper, Namen und Baugruppen-Lagen (STEP-Import, Aufgabe 8).
//!
//! **Was ein Körper ist (P2):** Je Vorkommen einer Darstellung mit mindestens einem
//! `MANIFOLD_SOLID_BREP`/`BREP_WITH_VOIDS` ein Körper; alle Volumen-Einträge der Darstellung
//! werden Schalen dieses einen Körpers. Volumen-Einträge, die in keiner Darstellung stehen, sind
//! je ein eigener Körper. Ein Teil, das eine Baugruppe zweimal benutzt, ergibt zwei Körper.
//!
//! **Lagen (P3):** Die Baugruppe ist ein Graph aus Darstellungen. Kanten (Eltern → Kind, mit der
//! Transformation von Kind- in Elternkoordinaten):
//! - `CONTEXT_DEPENDENT_SHAPE_REPRESENTATION`: Die Beziehung (`rep_1`, `rep_2`) ist nach ISO
//!   `rep_1` = Kind; die Richtung wird aber über das zugehörige `NEXT_ASSEMBLY_USAGE_OCCURRENCE`
//!   bestimmt (Elternteil = `relating_product_definition`): Gehört `rep_1` zum Kind, gilt sie
//!   so, gehört `rep_2` zum Kind, ist sie vertauscht, sonst „Richtung unklar“.
//!   `ITEM_DEFINED_TRANSFORMATION(it1, it2)`: `it1` gehört zu `rep_1`, `it2` zu `rep_2`;
//!   `T = A(Eltern-Lage) · A(Kind-Lage)⁻¹`. Ein `CARTESIAN_TRANSFORMATION_OPERATOR_3D` bildet
//!   `rep_1` auf `rep_2` ab (vertauscht: sein Kehrwert). Ohne Transformation: Einheitslage.
//! - `MAPPED_ITEM` in einer Darstellung: Eltern = diese Darstellung, Kind = die abgebildete;
//!   `T = A(mapping_target) · A(mapping_origin)⁻¹`.
//! - Beziehung ohne `CONTEXT_DEPENDENT_SHAPE_REPRESENTATION` und ohne Transformation (etwa
//!   `SHAPE_REPRESENTATION` ↔ `ADVANCED_BREP_SHAPE_REPRESENTATION` eines Teils): beide
//!   Darstellungen liegen im selben Raum (eine Gruppe).
//! - Beziehung mit Transformation, aber ohne Vorkommen: Eltern ist die Darstellung, die als
//!   einzige zu einem Produkt gehört, sonst `rep_2` (ISO).
//!
//! Jede Lage wird in den Einheiten der Darstellung gelesen, zu der sie gehört (`units_of`), und
//! ist danach eine Bewegung in mm. Verschachtelte Lagen werden bis zur Wurzel multipliziert
//! (`M = T_Wurzel · … · T_Kind`). Spiegelung (negative Determinante), Skalierung (Maßstab ≠ 1
//! oder schiefe Achsen), unbekannte Arten von Lagen und unklare Richtungen ergeben eine Meldung
//! ([`placement_note`]); der Körper und alles darunter liegt dann am Ursprung. Darstellungen in
//! einem Kreis (und alles, was nur über einen Kreis erreichbar ist) erscheinen je einmal mit
//! „Verweise im Kreis“; tiefer als [`MAX_ASSEMBLY_DEPTH`] Ebenen: „zu tief verschachtelt“.
//! Verweise, die fehlen oder nicht zu einer Darstellung führen, lehnen die ganze Datei ab (sonst
//! läge ein Teil still an der falschen Stelle).
//!
//! **Reihenfolge:** Tiefensuche ab den Wurzeln (Gruppen ohne eingehende Kante) in
//! Dateireihenfolge, Kinder in der Reihenfolge ihrer Einträge; dann nicht erreichte
//! Darstellungen; dann freie Volumen-Einträge. Teilbäume ohne Körper werden nicht betreten
//! (vorab gezählt), so bleibt die Suche auch bei vielen leeren Verzweigungen linear.
//!
//! **Namen (P10):** Produktname, sonst Name der Darstellung, sonst Name des ersten
//! Volumen-Eintrags; Steuerzeichen → Leerzeichen, getrimmt, höchstens
//! [`MAX_BODY_NAME_CHARS`] Zeichen; Doppelte „Name (2)“, „Name (3)“ … (auch dann höchstens 64
//! Zeichen); leere Namen bleiben leer (die App vergibt „STEP-Körper n“).

use crate::read::error::StepReadError;
use crate::read::geom::{Resolver, Skip};
use crate::read::index::StepFile;
use crate::read::units::Units;
use crate::read::value::{Entity, Value};
use planora_math::{DMat4, DVec3, Frame};
use std::collections::{HashMap, HashSet};

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
}

/// Meldung zu einer Lage, die sich nicht bestimmen ließ.
pub fn placement_note(name: &str, reason: &str) -> String {
    format!(
        "Die Lage von „{name}“ ließ sich nicht bestimmen ({reason}); der Körper liegt am Ursprung."
    )
}

/// Gründe (wörtlich, Plan Aufgabe 8).
const MIRROR: &str = "Spiegelung";
const SCALING: &str = "Skalierung";
const CYCLE: &str = "Verweise im Kreis";
const TOO_DEEP: &str = "zu tief verschachtelt";
const UNCLEAR: &str = "Richtung unklar";

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

/// Abstand, ab dem gegebene Achsen eines Operators als schief gelten (Kosinus).
const OBLIQUE_TOL: f64 = 1e-6;
/// Abstand, ab dem ein Maßstab nicht 1 ist.
const SCALE_TOL: f64 = 1e-9;

/// Ergebnis einer Lage: Bewegung in mm oder Grund.
type Placed = Result<DMat4, String>;

/// Eine Darstellung.
struct Rep {
    id: u64,
    name: String,
    solids: Vec<u64>,
    context: Option<u64>,
}

/// Eine Kante zwischen Darstellungen (vor dem Zusammenfassen zu Gruppen).
struct RawEdge {
    parent: u64,
    child: u64,
    /// Ordnung unter Geschwistern: Zeile und Nummer des bestimmenden Eintrags.
    key: (usize, u64),
    transform: Placed,
}

/// Eine Kante zwischen Gruppen.
struct Edge {
    child: usize,
    key: (usize, u64),
    transform: Placed,
}

/// Ein Vorkommen eines Körpers vor der Namensvergabe.
struct Hit {
    rep: Option<usize>,
    solid: Option<u64>,
    placement: Placed,
}

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
    /// Produktdefinition → ihre Darstellungen.
    pd_reps: HashMap<u64, Vec<u64>>,
    /// Gleiche Räume (Beziehungen ohne Transformation und ohne Vorkommen).
    links: Vec<(u64, u64)>,
    edges: Vec<RawEdge>,
    /// Noch zu lesende `MAPPED_ITEM`s: (Darstellung, Eintrag).
    pending: Vec<(u64, u64)>,
    units: HashMap<Option<u64>, Result<Units, String>>,
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
            pd_reps: HashMap::new(),
            links: Vec::new(),
            edges: Vec::new(),
            pending: Vec::new(),
            units: HashMap::new(),
        }
    }

    fn line(&self, id: u64) -> usize {
        self.file.line(id).unwrap_or(0)
    }

    /// Ordnung in der Datei.
    fn key(&self, id: u64) -> (usize, u64) {
        (self.line(id), id)
    }

    fn refer(&self, id: u64, v: &Value) -> Result<u64, StepReadError> {
        v.reference().map_err(|m| self.file.fail(id, m))
    }

    /// Lädt die Darstellung `#id` (einmal) samt ihrer `MAPPED_ITEM`s; liefert ihren Index.
    fn load_rep(&mut self, id: u64) -> Result<usize, StepReadError> {
        if let Some(&i) = self.rep_index.get(&id) {
            return Ok(i);
        }
        let e = self.file.entity(id)?;
        let Some(args) = rep_args(&e) else {
            return Err(self.file.fail(id, "Erwartet wird eine Darstellung."));
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
        let index = self.reps.len();
        self.reps.push(Rep {
            id,
            name,
            solids,
            context,
        });
        self.rep_index.insert(id, index);
        // `MAPPED_ITEM`s erst später (ohne Rekursion über Ketten von Abbildungen).
        let mapped = self.file.ids_of("MAPPED_ITEM");
        self.pending.extend(
            items
                .into_iter()
                .filter(|item| mapped.binary_search(item).is_ok())
                .map(|item| (id, item)),
        );
        Ok(index)
    }

    /// Lädt die Darstellung `#id` und alle über `MAPPED_ITEM` von ihr erreichten.
    fn load_rep_all(&mut self, id: u64) -> Result<usize, StepReadError> {
        let index = self.load_rep(id)?;
        while let Some((parent, item)) = self.pending.pop() {
            self.mapped_item(parent, item)?;
        }
        Ok(index)
    }

    /// `MAPPED_ITEM(name, REPRESENTATION_MAP(origin, rep), target)` in der Darstellung `parent`.
    fn mapped_item(&mut self, parent: u64, item: u64) -> Result<(), StepReadError> {
        let e = self.file.entity(item)?;
        let source = self.refer(
            item,
            e.arg("MAPPED_ITEM", 1)
                .map_err(|m| self.file.fail(item, m))?,
        )?;
        let target = self.refer(
            item,
            e.arg("MAPPED_ITEM", 2)
                .map_err(|m| self.file.fail(item, m))?,
        )?;
        let map = self.file.entity(source)?;
        let origin = self.refer(
            source,
            map.arg("REPRESENTATION_MAP", 0)
                .map_err(|m| self.file.fail(source, m))?,
        )?;
        let child = self.refer(
            source,
            map.arg("REPRESENTATION_MAP", 1)
                .map_err(|m| self.file.fail(source, m))?,
        )?;
        let ci = self.load_rep(child)?;
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
            transform,
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

    /// Lage `#id` (`AXIS2_PLACEMENT_3D` oder `CARTESIAN_TRANSFORMATION_OPERATOR_3D`) in den
    /// Einheiten von `context`, als Bewegung in mm.
    fn item_matrix(&mut self, id: u64, context: Option<u64>) -> Placed {
        let units = self.units(context)?;
        let e = self.file.entity(id).map_err(|e| reason(&e.to_string()))?;
        let mut res = Resolver::new(self.file, units);
        if e.is("AXIS2_PLACEMENT_3D") {
            let f = res.placement(id).map_err(|Skip(m)| reason(&m))?;
            return Ok(frame_matrix(&f));
        }
        if e.is("CARTESIAN_TRANSFORMATION_OPERATOR_3D") {
            return operator_matrix(self.file, &mut res, id, &e);
        }
        Err(format!("unbekannte Art {}", type_name(&e)))
    }

    /// Liest alle Darstellungen, Produkte, Vorkommen und Beziehungen.
    fn collect(&mut self) -> Result<(), StepReadError> {
        let mut rep_ids: Vec<u64> = REP_TYPES
            .iter()
            .flat_map(|t| self.file.ids_of(t).iter().copied())
            .collect();
        rep_ids.sort_unstable();
        rep_ids.dedup();
        for id in rep_ids {
            self.load_rep_all(id)?;
        }
        self.products();
        // Vorkommen: Beziehung → (CDSR, Eltern-PD, Kind-PD).
        let mut occurrence: HashMap<u64, (u64, Option<(u64, u64)>)> = HashMap::new();
        let mut rels: Vec<u64> = Vec::new();
        for &cdsr in self.file.ids_of("CONTEXT_DEPENDENT_SHAPE_REPRESENTATION") {
            let e = self.file.entity(cdsr)?;
            const C: &str = "CONTEXT_DEPENDENT_SHAPE_REPRESENTATION";
            let rr = self.refer(cdsr, e.arg(C, 0).map_err(|m| self.file.fail(cdsr, m))?)?;
            let pds = self.refer(cdsr, e.arg(C, 1).map_err(|m| self.file.fail(cdsr, m))?)?;
            let usage = self.usage_of(pds)?;
            occurrence.entry(rr).or_insert((cdsr, usage));
            rels.push(rr);
        }
        rels.extend(
            RELATIONSHIP_TYPES
                .iter()
                .flat_map(|t| self.file.ids_of(t).iter().copied()),
        );
        rels.sort_unstable();
        rels.dedup();
        for rr in rels {
            let occ = occurrence.get(&rr).copied();
            self.relationship(rr, occ)?;
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

    /// Eltern- und Kind-Produktdefinition des Vorkommens hinter `pds`; `None`, wenn die
    /// Definition kein Vorkommen ist.
    fn usage_of(&self, pds: u64) -> Result<Option<(u64, u64)>, StepReadError> {
        let e = self.file.entity(pds)?;
        let Some(def) = e
            .part("PRODUCT_DEFINITION_SHAPE")
            .or_else(|| e.part("PROPERTY_DEFINITION"))
            .and_then(|a| a.get(2))
            .and_then(|v| v.reference().ok())
        else {
            return Ok(None);
        };
        let u = self.file.entity(def)?;
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

    /// Eine Beziehung zwischen Darstellungen; `occ`: ihr Vorkommen (CDSR) mit Eltern- und
    /// Kind-Produktdefinition.
    fn relationship(
        &mut self,
        rr: u64,
        occ: Option<(u64, Option<(u64, u64)>)>,
    ) -> Result<(), StepReadError> {
        let e = self.file.entity(rr)?;
        let Some(args) = e
            .part("REPRESENTATION_RELATIONSHIP")
            .filter(|a| a.len() >= 4)
            .or_else(|| match &e {
                Entity::Simple(r) if r.args.len() >= 4 => Some(&r.args[..]),
                _ => None,
            })
        else {
            return Err(self
                .file
                .fail(rr, "Erwartet wird eine Beziehung zweier Darstellungen."));
        };
        let rep_1 = self.refer(rr, &args[2])?;
        let rep_2 = self.refer(rr, &args[3])?;
        let op = match e.part("REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION") {
            Some(a) if a.len() == 1 => Some(self.refer(rr, &a[0])?),
            Some(a) if a.len() >= 5 => Some(self.refer(rr, &a[4])?),
            Some(_) => return Err(self.file.fail(rr, "Die Transformation fehlt.")),
            None => None,
        };
        self.load_rep_all(rep_1)?;
        self.load_rep_all(rep_2)?;
        let (swapped, key) = match occ {
            Some((cdsr, usage)) => {
                let swapped = match usage {
                    Some((ppd, cpd)) => self.direction(rep_1, rep_2, ppd, cpd),
                    None => None,
                };
                let key = self.key(cdsr);
                let Some(swapped) = swapped else {
                    self.edges.push(RawEdge {
                        parent: rep_2,
                        child: rep_1,
                        key,
                        transform: Err(UNCLEAR.into()),
                    });
                    return Ok(());
                };
                (swapped, key)
            }
            None => {
                if op.is_none() {
                    self.links.push((rep_1, rep_2));
                    return Ok(());
                }
                // Ohne Vorkommen: Eltern ist die Darstellung, die als einzige zu einem Produkt
                // gehört, sonst `rep_2` (ISO).
                let swapped = self.rep_pd.contains_key(&rep_1) && !self.rep_pd.contains_key(&rep_2);
                (swapped, self.key(rr))
            }
        };
        let (parent, child) = if swapped {
            (rep_1, rep_2)
        } else {
            (rep_2, rep_1)
        };
        let transform = match op {
            None => Ok(DMat4::IDENTITY),
            Some(op) => self.relation_transform(op, rep_1, rep_2, swapped),
        };
        self.edges.push(RawEdge {
            parent,
            child,
            key,
            transform,
        });
        Ok(())
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
        let ctx_1 = self.reps[self.rep_index[&rep_1]].context;
        let ctx_2 = self.reps[self.rep_index[&rep_2]].context;
        let e = self.file.entity(op).map_err(|e| reason(&e.to_string()))?;
        if let Some(args) = e.part("ITEM_DEFINED_TRANSFORMATION") {
            let (Some(Value::Ref(it_1)), Some(Value::Ref(it_2))) = (args.get(2), args.get(3))
            else {
                return Err(reason(
                    &self
                        .file
                        .fail(op, "Eine Lage der Transformation fehlt.")
                        .to_string(),
                ));
            };
            // `it_1` gehört zu `rep_1`, `it_2` zu `rep_2`.
            let a_1 = self.item_matrix(*it_1, ctx_1)?;
            let a_2 = self.item_matrix(*it_2, ctx_2)?;
            let (parent, child) = if swapped { (a_1, a_2) } else { (a_2, a_1) };
            return Ok(parent * rigid_inverse(child));
        }
        if e.is("CARTESIAN_TRANSFORMATION_OPERATOR_3D") {
            // Bildet `rep_1` auf `rep_2` ab; der Ursprung steht in den Einheiten von `rep_2`.
            let m = self.item_matrix(op, ctx_2)?;
            return Ok(if swapped { rigid_inverse(m) } else { m });
        }
        Err(format!("unbekannte Art {}", type_name(&e)))
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
    fn raw_name(&self, rep: usize, group_members: &[usize]) -> String {
        let r = &self.reps[rep];
        // Der Reihe nach, erst bei Bedarf gelesen (eine Gruppe kann viele Darstellungen haben).
        let products = std::iter::once(r.id)
            .chain(group_members.iter().map(|&m| self.reps[m].id))
            .filter(|id| self.rep_pd.contains_key(id))
            .filter_map(|id| self.product_name(id));
        let rest = std::iter::once_with(|| r.name.clone())
            .chain(r.solids.first().map(|&s| self.solid_name(s)));
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
        // Kanten zwischen Gruppen, Geschwister in Dateireihenfolge.
        let mut children: Vec<Vec<Edge>> = (0..groups).map(|_| Vec::new()).collect();
        let mut incoming = vec![false; groups];
        for e in std::mem::take(&mut self.edges) {
            let p = group_of[self.rep_index[&e.parent]];
            let c = group_of[self.rep_index[&e.child]];
            incoming[c] = true;
            children[p].push(Edge {
                child: c,
                key: e.key,
                transform: e.transform,
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
        let own: Vec<usize> = members
            .iter()
            .map(|m| {
                m.iter()
                    .filter(|&&r| !self.reps[r].solids.is_empty())
                    .count()
            })
            .collect();
        let cap = self.file.limits().max_bodies + 1;
        let count = subtree_counts(&succ, &own, &cyclic, cap);

        let mut walk = Walk {
            members: &members,
            reps: &self.reps,
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
                walk.visit(g, Ok(DMat4::IDENTITY), 0)?;
            }
        }
        // Nicht erreichte Darstellungen: nur über Kreise erreichbar.
        for g in 0..groups {
            if !walk.reached[g] {
                walk.emit(g, &Err(CYCLE.into()))?;
            }
        }
        // Freie Volumen-Einträge.
        let in_rep: HashSet<u64> = self
            .reps
            .iter()
            .flat_map(|r| r.solids.iter().copied())
            .collect();
        for &s in self.solids {
            if !in_rep.contains(&s) {
                walk.push(Hit {
                    rep: None,
                    solid: Some(s),
                    placement: Ok(DMat4::IDENTITY),
                })?;
            }
        }
        let hits = walk.hits;

        let mut used: HashSet<String> = HashSet::new();
        let mut out = Vec::with_capacity(hits.len());
        for h in hits {
            let (raw, items, context) = match (h.rep, h.solid) {
                (Some(r), _) => {
                    let g = group_of[r];
                    let rep = &self.reps[r];
                    (
                        self.raw_name(r, &members[g]),
                        rep.solids.clone(),
                        rep.context,
                    )
                }
                (None, Some(s)) => (clean_name(&self.solid_name(s)), vec![s], None),
                (None, None) => continue,
            };
            let name = unique_name(raw, &mut used);
            let (placement, placement_note) = match h.placement {
                Ok(m) => (m, None),
                Err(why) => (DMat4::IDENTITY, Some(placement_note(&name, &why))),
            };
            out.push(FoundBody {
                name,
                items,
                context,
                placement,
                placement_note,
            });
        }
        Ok(out)
    }
}

/// Tiefensuche über die Gruppen.
struct Walk<'w> {
    members: &'w [Vec<usize>],
    reps: &'w [Rep],
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
    fn emit(&mut self, g: usize, m: &Placed) -> Result<(), StepReadError> {
        self.reached[g] = true;
        for &r in &self.members[g] {
            if !self.reps[r].solids.is_empty() {
                self.push(Hit {
                    rep: Some(r),
                    solid: None,
                    placement: m.clone(),
                })?;
            }
        }
        Ok(())
    }

    /// Besucht `g` in der Tiefe `depth` (Zahl der Vorkommen ab der Wurzel) mit der Lage `m`.
    fn visit(&mut self, g: usize, m: Placed, depth: usize) -> Result<(), StepReadError> {
        self.emit(g, &m)?;
        let children = self.children;
        for e in &children[g] {
            if self.cyclic[e.child] || self.count[e.child] == 0 {
                continue;
            }
            if depth + 1 > MAX_ASSEMBLY_DEPTH {
                self.emit_below(e.child, TOO_DEEP)?;
                continue;
            }
            let next = match (&m, &e.transform) {
                (Ok(a), Ok(t)) => Ok(*a * *t),
                (Err(r), _) | (Ok(_), Err(r)) => Err(r.clone()),
            };
            self.visit(e.child, next, depth + 1)?;
        }
        Ok(())
    }

    /// Alle Körper ab `g` (ohne Kreise) je einmal mit dem Grund `why`.
    fn emit_below(&mut self, g: usize, why: &str) -> Result<(), StepReadError> {
        let mut seen = HashSet::from([g]);
        let mut todo = vec![g];
        let why: Placed = Err(why.into());
        while let Some(v) = todo.pop() {
            self.emit(v, &why)?;
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

/// `CARTESIAN_TRANSFORMATION_OPERATOR_3D(…, axis1, axis2, local_origin, scale, axis3)` (die
/// letzten fünf Parameter) nach ISO 10303-42 (`base_axis`): `D3` = axis3 (Vorgabe z), `D1` =
/// axis1 senkrecht zu `D3`, `D2` = axis2 senkrecht zu beiden (Vorgabe `D3 × D1`); Bild
/// `p ↦ origin + scale · (p.x D1 + p.y D2 + p.z D3)`. Nur Bewegungen werden angenommen:
/// negative Determinante → Spiegelung; Maßstab ≠ 1 oder gegebene Achsen, die nicht senkrecht
/// aufeinander stehen → Skalierung.
fn operator_matrix(file: &StepFile, res: &mut Resolver, id: u64, e: &Entity) -> Placed {
    let fail = |m: String| reason(&file.fail(id, m).to_string());
    let args: Vec<&Value> = match e.part("CARTESIAN_TRANSFORMATION_OPERATOR_3D") {
        Some(a) if a.len() >= 5 => a[a.len() - 5..].iter().collect(),
        Some(a3) if a3.len() == 1 => match e.part("CARTESIAN_TRANSFORMATION_OPERATOR") {
            Some(a) if a.len() >= 4 => a[a.len() - 4..].iter().chain(a3).collect(),
            _ => return Err(fail("Parameter des Operators fehlen.".into())),
        },
        _ => return Err(fail("Parameter des Operators fehlen.".into())),
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
    let scale = if args[3].is_unset() {
        1.0
    } else {
        args[3].real().map_err(fail)?
    };
    let d3 = axis3.unwrap_or(DVec3::Z);
    let v1 = match axis1 {
        Some(a) => {
            if a.dot(d3).abs() > OBLIQUE_TOL {
                return Err(SCALING.into());
            }
            a
        }
        None if d3.cross(DVec3::X).length() > 1e-9 => DVec3::X,
        None => DVec3::Z,
    };
    let d1 = (v1 - d3 * v1.dot(d3)).normalize();
    let d2 = match axis2 {
        Some(a) => {
            if a.dot(d3).abs() > OBLIQUE_TOL || a.dot(d1).abs() > OBLIQUE_TOL {
                return Err(SCALING.into());
            }
            (a - d3 * a.dot(d3) - d1 * a.dot(d1)).normalize()
        }
        None => d3.cross(d1),
    };
    let handed = d1.cross(d2).dot(d3);
    let det = scale * scale * scale * handed;
    if !(det.is_finite() && origin.is_finite()) {
        return Err(SCALING.into());
    }
    if det < 0.0 {
        return Err(MIRROR.into());
    }
    if (scale - 1.0).abs() > SCALE_TOL {
        return Err(SCALING.into());
    }
    Ok(DMat4::from_cols(
        d1.extend(0.0),
        d2.extend(0.0),
        d3.extend(0.0),
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
