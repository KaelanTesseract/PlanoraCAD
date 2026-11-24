//! Abgedockte Fenster: Plätze, Zustand je Platz, Monitorprüfung und Umrechnung zwischen
//! Punkten (egui) und physischen Pixeln (config.json).

use crate::theme;
use planora_project::config::{Config, DetachedWindow};
use std::collections::BTreeMap;
use std::path::Path;

mod contents;
mod decide;
mod keys;
mod monitor;
mod place;
mod state;
mod strip;
mod window;

use contents::*;
pub use decide::*;
pub use keys::*;
pub use monitor::*;
pub use place::*;
pub use state::*;
use strip::*;
pub use window::*;

#[cfg(test)]
mod tests;
