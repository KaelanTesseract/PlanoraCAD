//! Zustand je Platz (Konfiguration) und im egui-Kontext.

use super::*;

/// Welche Plätze abgedockt sind und wo; `dirty`, wenn seit dem letzten Speichern etwas
/// geändert wurde.
#[derive(Clone, Debug, Default)]
pub struct DetachState {
    windows: BTreeMap<String, DetachedWindow>,
    dirty: bool,
    /// Der Zustand ist gerade an den egui-Kontext verliehen ([`install`]); dies ist nur der leere
    /// Platzhalter.
    lent: bool,
}

impl DetachState {
    pub fn from_config(c: &Config) -> Self {
        DetachState {
            windows: c.detached.clone(),
            dirty: false,
            lent: false,
        }
    }

    pub fn get(&self, p: Place) -> Option<DetachedWindow> {
        self.windows.get(&p.key()).copied()
    }

    /// Dockt den Platz wieder an.
    pub fn undock(&mut self, p: Place) {
        if self.windows.remove(&p.key()).is_some() {
            self.dirty = true;
        }
    }

    /// Merkt Lage und Größe des abgedockten Platzes.
    pub fn set(&mut self, p: Place, w: DetachedWindow) {
        if self.windows.insert(p.key(), w) != Some(w) {
            self.dirty = true;
        }
    }

    /// `true`, wenn seit dem letzten Aufruf etwas geändert wurde (setzt zurück).
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// Die abgedockten Fenster für `config.json`; `None`, solange der Zustand an den
    /// egui-Kontext verliehen ist ([`install`], während eines Bilds).
    pub fn windows(&self) -> Option<&BTreeMap<String, DetachedWindow>> {
        (!self.lent).then_some(&self.windows)
    }

    /// Merkt wieder „geändert“, etwa wenn das Speichern nach [`DetachState::take_dirty`]
    /// scheiterte; der nächste Versuch schreibt den Stand dann erneut.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Schreibt den Stand, falls er sich geändert hat. Scheitert das Schreiben, bleibt die
    /// Änderung vorgemerkt (der nächste Aufruf versucht es erneut) statt verloren zu gehen.
    pub fn save_if_dirty(&mut self, config_path: &Path) {
        if self.take_dirty() && !self.save(config_path) {
            self.mark_dirty();
        }
    }

    /// Schreibt nur das Feld `detached` in die Konfigurationsdatei; `true`, wenn geschrieben
    /// wurde. Ein Fehler lässt die Datei unangetastet und bricht nichts ab.
    pub fn save(&self, config_path: &Path) -> bool {
        let windows = self.windows.clone();
        Config::update_at(config_path, |c| c.detached = windows).is_ok()
    }
}

// ---------------------------------------------------------------------------------------------
// Zustand im egui-Kontext
// ---------------------------------------------------------------------------------------------

/// Ablage des Abdock-Zustands in `ctx.data` während eines Bilds.
pub(super) fn state_id() -> egui::Id {
    egui::Id::unique("planora_detach_state")
}

/// Legt den Abdock-Zustand für dieses Bild in `ctx.data` ab, damit `Dialog` ihn erreicht, ohne
/// dass alle Aufrufe ihn durchreichen. `state` ist danach leer, bis [`uninstall`] ihn
/// zurückholt (am Ende von `PlanoraApp::ui`).
pub fn install(ctx: &egui::Context, state: &mut DetachState) {
    let s = std::mem::take(state);
    state.lent = true;
    ctx.data_mut(|d| d.insert_temp(state_id(), s));
}

/// Holt den mit [`install`] abgelegten Zustand nach `state` zurück. Wurde der Platzhalter
/// inzwischen als geändert markiert ([`DetachState::mark_dirty`], etwa nach dem Ersatz einer
/// unlesbaren `config.json`), bleibt das erhalten: der Stand wird dann neu geschrieben.
pub fn uninstall(ctx: &egui::Context, state: &mut DetachState) {
    let back = ctx.data_mut(|d| d.remove_temp::<DetachState>(state_id()));
    debug_assert!(back.is_some(), "uninstall without install");
    if let Some(back) = back {
        let resave = state.dirty;
        *state = back;
        state.dirty |= resave;
    }
}

/// Ist ein Zustand abgelegt (und gerade nicht von [`with_state`] ausgeliehen)?
pub fn is_installed(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<DetachState>(state_id()).is_some())
}

/// Führt `f` mit dem abgelegten Zustand aus; `None`, wenn keiner abgelegt ist. Während `f`
/// läuft, ist der Zustand ausgeliehen: Ein darin gezeigter Dialog (etwa im abgedockten Fenster)
/// findet keinen und bleibt wie heute angedockt.
pub fn with_state<R>(ctx: &egui::Context, f: impl FnOnce(&mut DetachState) -> R) -> Option<R> {
    let mut state = ctx.data_mut(|d| d.remove_temp::<DetachState>(state_id()))?;
    let r = f(&mut state);
    ctx.data_mut(|d| d.insert_temp(state_id(), state));
    Some(r)
}
