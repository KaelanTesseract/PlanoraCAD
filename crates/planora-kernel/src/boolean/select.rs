//! Auswahl der Flächenstücke für das Ergebnis (Spec §6).

use super::classify::Class;
use super::error::BoolOp;
use super::seg::{Seg, reverse_loop};
use super::split::Region;
use super::uv::FaceGeom;
use crate::geometry::{Surface, UvRect};

/// Herkunft eines Flächenstücks: Körper A oder Körper B.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    A,
    B,
}

/// Ein ausgewähltes Flächenstück für den Ergebniskörper.
#[derive(Clone, Debug)]
pub struct Piece {
    pub surface: Surface,
    pub same_sense: bool,
    pub domain: Option<UvRect>,
    pub outer: Vec<Seg>,
    pub holes: Vec<Vec<Seg>>,
}

/// Auswahltabelle (siehe Spec §6). Bündige Stücke werden nur von Körper A übernommen.
pub fn select_piece(
    op: BoolOp,
    side: Side,
    class: Class,
    geom: &FaceGeom,
    region: Region,
) -> Option<Piece> {
    use BoolOp::*;
    use Class::*;
    use Side::*;
    // (behalten, umdrehen)
    let (keep, flip) = match (op, side, class) {
        (Union, _, Outside) => (true, false),
        (Union, A, OnSame) => (true, false),
        (Union, B, OnSame) => (false, false),
        (Union, _, OnOpposite) => (false, false),
        (Union, _, Inside) => (false, false),

        (Difference, A, Outside) => (true, false),
        (Difference, A, OnOpposite) => (true, false),
        (Difference, A, OnSame) => (false, false),
        (Difference, A, Inside) => (false, false),
        (Difference, B, Inside) => (true, true),
        (Difference, B, _) => (false, false),

        (Intersection, _, Inside) => (true, false),
        (Intersection, A, OnSame) => (true, false),
        (Intersection, B, OnSame) => (false, false),
        (Intersection, _, OnOpposite) => (false, false),
        (Intersection, _, Outside) => (false, false),
    };
    if !keep {
        return None;
    }
    let (outer, holes) = if flip {
        (
            reverse_loop(&region.outer),
            region.holes.iter().map(|h| reverse_loop(h)).collect(),
        )
    } else {
        (region.outer, region.holes)
    };
    Some(Piece {
        surface: geom.surface.clone(),
        same_sense: geom.same_sense != flip,
        domain: geom.domain,
        outer,
        holes,
    })
}
