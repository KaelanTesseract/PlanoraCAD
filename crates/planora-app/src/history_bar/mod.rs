//! Verlaufsleiste: die Schritte des gewählten Körpers mit einem Marker je Körper.
//!
//! - `model`: das Modell ohne Oberfläche (Schritte, Markerstelle, Bewegen, Farbe, Sperre); es
//!   ist ohne Fenster testbar. Der globale Marker (`browser::roll_to_end`, Menü „Bearbeiten“)
//!   bleibt daneben unverändert.
//! - `view`: die Oberfläche ([`show_with`]), ein schwebendes Feld zwischen den Spalten über dem
//!   Befehlsfeld (Lage aus [`crate::shell::layout_in_mode`]) mit Abspielknöpfen, einem Symbol je
//!   Schritt in der Körperfarbe, dem Marker als senkrechter Linie mit Griff und rechts dem Stand.
//!
//! Der Marker eines Körpers steht hinter dem letzten aktiven Schritt von vorn: Position `p`
//! heißt, die ersten `p` Schritte der Kette sind aktiv, der Schritt an `p` trägt die
//! Rückrollstelle. `p == Anzahl der Schritte` heißt, der Körper hat keine Stelle.

mod model;
mod view;

pub use model::*;
pub use view::*;
