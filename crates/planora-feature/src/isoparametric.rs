//! Isoparametrische Kurve (Skizzen 11b, Spec 6): Richtung der Kurve auf der Fläche. Die Herkunft
//! ist [`crate::CopySource::Isoparametric`]; in Task 7 nur die Typen, die Rechnung kommt in
//! Task 15.

/// Richtung der Isokurve: `U` ist die Kurve `u = const`, `V` die Kurve `v = const`
/// (**Annahme**, in `offene-punkte.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsoDirection {
    U,
    V,
}
