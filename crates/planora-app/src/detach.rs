//! Abgedockte Fenster: Plätze, Zustand je Platz, Monitorprüfung und Umrechnung zwischen
//! Punkten (egui) und physischen Pixeln (config.json).

use crate::theme;
use planora_project::config::{Config, DetachedWindow};
use std::collections::BTreeMap;
use std::path::Path;

/// Ein Platz, der sich als eigenes Fenster abdocken lässt.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Place {
    Tree,
    Right,
    SketchSide,
    Dialog(&'static str),
}

/// Alle Dialogschlüssel (wird mit den Dialogen in Aufgabe 5 gefüllt).
pub const DIALOG_KEYS: &[&str] = &[];

impl Place {
    /// Schlüssel in `config.json` (`detached`).
    pub fn key(self) -> String {
        match self {
            Place::Tree => "tree".to_string(),
            Place::Right => "right".to_string(),
            Place::SketchSide => "sketch_side".to_string(),
            Place::Dialog(k) => format!("dialog:{k}"),
        }
    }
}

/// Welche Plätze abgedockt sind und wo; `dirty`, wenn seit dem letzten Speichern etwas
/// geändert wurde.
#[derive(Clone, Debug, Default)]
pub struct DetachState {
    windows: BTreeMap<String, DetachedWindow>,
    dirty: bool,
}

impl DetachState {
    pub fn from_config(c: &Config) -> Self {
        DetachState {
            windows: c.detached.clone(),
            dirty: false,
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

/// Prüft, ob der Kopfstreifen des Fensters (physische Pixel) auf einem Monitor liegt.
/// `None`, wenn die Plattform das nicht beantworten kann.
#[cfg(windows)]
pub fn on_screen(rect_px: egui::Rect) -> Option<bool> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONULL, MonitorFromRect};
    let head = egui::Rect::from_min_size(rect_px.min, egui::vec2(64.0, 24.0));
    let r = RECT {
        left: head.min.x.round() as i32,
        top: head.min.y.round() as i32,
        right: head.max.x.round() as i32,
        bottom: head.max.y.round() as i32,
    };
    // SAFETY: `r` ist ein gültiger Zeiger auf ein lokales RECT.
    let monitor = unsafe { MonitorFromRect(&r, MONITOR_DEFAULTTONULL) };
    Some(!monitor.is_null())
}

#[cfg(not(windows))]
pub fn on_screen(_rect_px: egui::Rect) -> Option<bool> {
    None
}

/// Liegt der Platz `p` im eigenen Fenster? Nur wenn echte Fenster möglich sind (nicht
/// eingebettet) und sein gespeicherter Platz auf einem Bildschirm liegt.
pub fn is_undocked(ctx: &egui::Context, state: &DetachState, p: Place) -> bool {
    !ctx.embed_viewports() && should_detach(state.get(p), on_screen).is_some()
}

/// Das gespeicherte Fenster, falls es abgedockt starten soll: `None`, wenn nichts gespeichert
/// ist oder der Monitor fehlt (dann bleibt der Platz angedockt).
pub fn should_detach(
    saved: Option<DetachedWindow>,
    on_screen: impl Fn(egui::Rect) -> Option<bool>,
) -> Option<DetachedWindow> {
    let w = saved?;
    let rect = egui::Rect::from_min_size(egui::pos2(w.x, w.y), egui::vec2(w.width, w.height));
    (on_screen(rect) != Some(false)).then_some(w)
}

/// Rechteck in Punkten nach physischen Pixeln.
pub fn to_physical(rect_points: egui::Rect, pixels_per_point: f32) -> DetachedWindow {
    DetachedWindow {
        x: rect_points.min.x * pixels_per_point,
        y: rect_points.min.y * pixels_per_point,
        width: rect_points.width() * pixels_per_point,
        height: rect_points.height() * pixels_per_point,
    }
}

/// Physische Pixel nach Punkten.
pub fn to_points(w: DetachedWindow, pixels_per_point: f32) -> egui::Rect {
    egui::Rect::from_min_size(
        egui::pos2(w.x / pixels_per_point, w.y / pixels_per_point),
        egui::vec2(w.width / pixels_per_point, w.height / pixels_per_point),
    )
}

// ---------------------------------------------------------------------------------------------
// Eigenes Fenster
// ---------------------------------------------------------------------------------------------

/// Wie ein abgedocktes Fenster aussieht und sich verhält.
#[derive(Clone, Copy, Debug)]
pub struct WindowSpec<'a> {
    pub place: Place,
    /// Titel in der Kopfzeile (das Betriebssystem bekommt „PlanoraCAD – {title}“).
    pub title: &'a str,
    /// Innere Größe in Punkten.
    pub inner_size: egui::Vec2,
    /// Ränder zum Ändern der Größe (Felder ja, Dialoge nein).
    pub resizable: bool,
    /// Beim Öffnen den Fokus nehmen (Dialoge ja, beim Start erscheinende Felder nein).
    pub focus_on_open: bool,
}

/// Was im abgedockten Fenster in diesem Bild geschah.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowEvent {
    None,
    /// Symbol „Andocken“ oder Doppelklick auf die Kopfzeile.
    DockClicked,
    /// Das Betriebssystem will das Fenster schließen (Alt+F4); das Schließen ist schon
    /// abgebrochen, der Aufrufer entscheidet (Feld andocken, Dialog abbrechen).
    CloseRequested,
}

/// Kann diese Oberfläche eigene Fenster öffnen? Nicht bei eingebetteten Viewports (Tests,
/// Plattformen ohne mehrere Fenster); dann gibt es kein Symbol „Abdocken“.
pub fn can_detach(ctx: &egui::Context) -> bool {
    !ctx.embed_viewports()
}

/// Kennung des Betriebssystemfensters für den Platz `place` (fest, sprachunabhängig).
pub fn viewport_id(place: Place) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(place.key())
}

/// Breite der Randstreifen zum Ändern der Größe (Punkte).
const RESIZE_EDGE: f32 = 6.0;
/// Größte Abweichung der Lage (physische Pixel), die ohne Korrektur bleibt.
const POSITION_TOLERANCE_PX: f32 = 2.0;

/// Gedächtnis eines offenen Fensters zwischen den Bildern.
#[derive(Clone, Copy, Debug)]
struct Opened {
    /// Bild des Aufrufers, in dem das Fenster zuletzt gezeigt wurde. Fehlt ein Bild, war das
    /// Fenster zu (egui entfernt ein Kindfenster, das in einem Bild nicht gezeigt wird).
    parent_frame: u64,
    /// Lage im Builder (Punkte des Aufrufers). Bleibt fest, solange das Fenster offen ist:
    /// egui schickt bei jeder Änderung des Builders die Lage erneut, winit rechnete sie mit dem
    /// Faktor des Fensters um und das Fenster spränge.
    position: Option<egui::Pos2>,
    /// Gewollte physische Lage beim Öffnen (für die Korrektur im zweiten Bild).
    wanted: Option<egui::Pos2>,
    /// Äußeres Rechteck (Punkte des Fensters) beim Beginn eines Zieh- oder Größenvorgangs.
    moving: Option<egui::Rect>,
}

/// Zeigt `add_contents` als eigenes Betriebssystemfenster für den Platz `spec.place`: ohne
/// Rahmen des Betriebssystems und ohne Taskleisten-Eintrag, mit der Graphit-Kopfzeile (Titel,
/// Symbol „Andocken“), Ziehen an der Kopfzeile und, falls `resizable`, Rändern zum Ändern der
/// Größe. Lage aus `state`; nach dem Ende eines Zieh- oder Größenvorgangs wird die neue Lage
/// (physische Pixel) in `state` gemerkt.
///
/// Der einzige Ort, der ein abgedocktes Fenster erzeugt. `add_contents` ist `FnMut`, weil egui
/// den Inhalt eines Kindfensters in einem Bild mehrmals ausführen kann (verworfene Durchgänge,
/// `request_discard`); geliefert wird das Ergebnis des letzten Durchgangs.
///
/// Bei eingebetteten Viewports ([`can_detach`] ist `false`) öffnet die Funktion nie ein
/// Fenster und liefert `(None, WindowEvent::None)`; Aufrufer prüfen das vorher.
pub fn show_window<R>(
    ctx: &egui::Context,
    state: &mut DetachState,
    spec: WindowSpec,
    mut add_contents: impl FnMut(&mut egui::Ui) -> R,
) -> (Option<R>, WindowEvent) {
    if !can_detach(ctx) {
        debug_assert!(
            false,
            "show_window ohne eigene Fenster ({:?}); vorher can_detach prüfen",
            spec.place
        );
        return (None, WindowEvent::None);
    }
    let id = viewport_id(spec.place);
    let memory = egui::Id::new(("detach_fenster", spec.place.key()));
    let frame = ctx.cumulative_frame_nr();
    // Punkte des Aufrufers: der Builder gibt die Lage logisch an winit weiter.
    let ppp = ctx.pixels_per_point();
    let mut opened = ctx
        .data(|d| d.get_temp::<Opened>(memory))
        .filter(|o| o.parent_frame + 1 >= frame)
        .unwrap_or_else(|| {
            let saved = state.get(spec.place);
            Opened {
                parent_frame: frame,
                position: saved.map(|w| to_points(w, ppp).min),
                wanted: saved.map(|w| egui::pos2(w.x, w.y)),
                moving: None,
            }
        });
    opened.parent_frame = frame;

    let mut builder = egui::ViewportBuilder::default()
        .with_title(format!("PlanoraCAD – {}", spec.title))
        .with_decorations(false)
        .with_taskbar(false)
        .with_inner_size(spec.inner_size)
        .with_resizable(spec.resizable)
        .with_active(spec.focus_on_open);
    if let Some(p) = opened.position {
        builder = builder.with_position(p);
    }
    let out = ctx.show_viewport_immediate(id, builder, |ui, _class| {
        window_contents(ui, state, &mut opened, &spec, &mut add_contents)
    });
    ctx.data_mut(|d| d.insert_temp(memory, opened));
    (Some(out.0), out.1)
}

/// Inhalt des Kindfensters (läuft mit dessen Eingabe und Faktor).
fn window_contents<R>(
    ui: &mut egui::Ui,
    state: &mut DetachState,
    opened: &mut Opened,
    spec: &WindowSpec,
    add_contents: &mut impl FnMut(&mut egui::Ui) -> R,
) -> (R, WindowEvent) {
    let ctx = ui.ctx().clone();
    let ppp = ctx.pixels_per_point();
    let outer = ctx.input(|i| i.viewport().outer_rect);
    let mut event = WindowEvent::None;

    // Lage-Korrektur: winit rechnet die logische Lage des Builders mit dem Faktor um, den das
    // neue Fenster gerade hat, nicht mit dem des Zielmonitors. Im zweiten Bild steht fest, wo
    // das Fenster gelandet ist; weicht es ab, einmal physisch genau hinsetzen (egui-winit
    // rechnet `OuterPosition` mit dem aktuellen Faktor des Fensters in Pixel um).
    if ctx.cumulative_frame_nr() == 1
        && let Some(wanted) = opened.wanted.take()
        && let Some(outer) = outer
        && (outer.min.to_vec2() * ppp - wanted.to_vec2())
            .abs()
            .max_elem()
            > POSITION_TOLERANCE_PX
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(wanted / ppp));
    }

    if ctx.input(|i| i.viewport().close_requested()) {
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        event = WindowEvent::CloseRequested;
    }

    let dark = ui.visuals().dark_mode;
    // Rahmen der Felder, aber eckig: das Fenster selbst ist rechteckig, gerundete Ecken
    // zeigten dahinter die leere Fensterfläche.
    let frame = theme::panel_frame(dark).corner_radius(egui::CornerRadius::ZERO);
    let r = egui::CentralPanel::default()
        .frame(frame)
        .show(ui, |ui| {
            // Fläche der Kopfzeile vor deren Symbolen: die Symbole liegen darüber und behalten
            // ihre Klicks, der Rest der Zeile zieht das Fenster bzw. dockt per Doppelklick an.
            let head = egui::Rect::from_min_size(
                ui.cursor().min,
                egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
            );
            let drag = ui.interact(
                head,
                ui.id().with("detach_kopfzeile"),
                egui::Sense::click_and_drag(),
            );
            let clicks =
                crate::dialog::header(ui, spec.title, false, Some(crate::dialog::DockButton::Dock));
            if clicks.dock || drag.double_clicked() {
                event = WindowEvent::DockClicked;
            } else if drag.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                opened.moving = outer;
            }
            add_contents(ui)
        })
        .inner;

    if spec.resizable {
        resize_edges(ui, opened, outer);
    }

    // Ende eines Zieh- oder Größenvorgangs: Maus losgelassen und Lage oder Größe geändert.
    if let Some(start) = opened.moving
        && !ctx.input(|i| i.pointer.any_down())
    {
        opened.moving = None;
        if let Some(now) = outer
            && now != start
        {
            state.set(spec.place, to_physical(now, ppp));
        }
    }
    (r, event)
}

/// Randstreifen ([`RESIZE_EDGE`]) zum Ändern der Größe über das Betriebssystem. Nach dem
/// Inhalt angelegt, damit sie über ihm liegen.
fn resize_edges(ui: &mut egui::Ui, opened: &mut Opened, outer: Option<egui::Rect>) {
    use egui::viewport::ResizeDirection as D;
    let r = ui.max_rect();
    let e = RESIZE_EDGE;
    let (x0, x1, y0, y1) = (r.left(), r.right(), r.top(), r.bottom());
    let rect = |a: egui::Pos2, b: egui::Pos2| egui::Rect::from_two_pos(a, b);
    // Ecken zuletzt: sie liegen über den Kanten.
    let strips = [
        (D::North, rect(egui::pos2(x0, y0), egui::pos2(x1, y0 + e))),
        (D::South, rect(egui::pos2(x0, y1 - e), egui::pos2(x1, y1))),
        (D::West, rect(egui::pos2(x0, y0), egui::pos2(x0 + e, y1))),
        (D::East, rect(egui::pos2(x1 - e, y0), egui::pos2(x1, y1))),
        (
            D::NorthWest,
            rect(egui::pos2(x0, y0), egui::pos2(x0 + e, y0 + e)),
        ),
        (
            D::NorthEast,
            rect(egui::pos2(x1 - e, y0), egui::pos2(x1, y0 + e)),
        ),
        (
            D::SouthWest,
            rect(egui::pos2(x0, y1 - e), egui::pos2(x0 + e, y1)),
        ),
        (
            D::SouthEast,
            rect(egui::pos2(x1 - e, y1 - e), egui::pos2(x1, y1)),
        ),
    ];
    for (dir, strip) in strips {
        let response = ui.interact(
            strip,
            egui::Id::new(("detach_rand", format!("{dir:?}"))),
            egui::Sense::drag(),
        );
        if response.hovered() || response.dragged() {
            ui.ctx().set_cursor_icon(match dir {
                D::North | D::South => egui::CursorIcon::ResizeVertical,
                D::West | D::East => egui::CursorIcon::ResizeHorizontal,
                D::NorthWest | D::SouthEast => egui::CursorIcon::ResizeNwSe,
                D::NorthEast | D::SouthWest => egui::CursorIcon::ResizeNeSw,
            });
        }
        if response.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
            opened.moving = outer;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win() -> DetachedWindow {
        DetachedWindow {
            x: -1500.0,
            y: 40.0,
            width: 400.0,
            height: 600.0,
        }
    }

    #[test]
    fn place_keys() {
        assert_eq!(Place::Tree.key(), "tree");
        assert_eq!(Place::Right.key(), "right");
        assert_eq!(Place::SketchSide.key(), "sketch_side");
        assert_eq!(Place::Dialog("blend").key(), "dialog:blend");
    }

    #[test]
    fn should_detach_missing_monitor_keeps_docked() {
        assert_eq!(should_detach(Some(win()), |_| Some(false)), None);
        assert_eq!(should_detach(None, |_| Some(true)), None);
    }

    #[test]
    fn should_detach_unknown_platform_trusts_saved() {
        assert_eq!(should_detach(Some(win()), |_| None), Some(win()));
        assert_eq!(should_detach(Some(win()), |_| Some(true)), Some(win()));
    }

    #[test]
    fn physical_round_trip_125() {
        let r = egui::Rect::from_min_size(egui::pos2(-120.3, 33.7), egui::vec2(320.0, 480.5));
        let back = to_points(to_physical(r, 1.25), 1.25);
        assert!((back.min - r.min).length() < 0.01);
        assert!((back.max - r.max).length() < 0.01);
    }

    #[test]
    fn state_undock_marks_dirty() {
        let mut s = DetachState::default();
        assert!(!s.take_dirty());
        s.set(Place::Tree, win());
        assert_eq!(s.get(Place::Tree), Some(win()));
        assert!(s.take_dirty());
        assert!(!s.take_dirty());
        s.set(Place::Tree, win());
        assert!(!s.take_dirty(), "gleicher Wert ist keine Änderung");
        s.undock(Place::Tree);
        assert_eq!(s.get(Place::Tree), None);
        assert!(s.take_dirty());
        s.undock(Place::Tree);
        assert!(!s.take_dirty());
    }

    #[test]
    fn save_writes_only_detached_field() {
        let dir = std::env::temp_dir().join(format!("planora-detach-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        let mut s = DetachState::default();
        s.set(Place::Right, win());
        assert!(s.save(&path));
        let c = Config::load_from(&path);
        assert_eq!(DetachState::from_config(&c).get(Place::Right), Some(win()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn failed_save_keeps_change_pending() {
        let dir = std::env::temp_dir().join(format!("planora-detach-dirty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Elternpfad ist eine Datei: Schreiben muss scheitern.
        let blocker = dir.join("datei");
        std::fs::write(&blocker, b"x").unwrap();
        let bad = blocker.join("config.json");
        let mut s = DetachState::default();
        s.set(Place::Tree, win());
        s.save_if_dirty(&bad);
        assert!(s.take_dirty(), "gescheitertes Speichern bleibt vorgemerkt");
        s.mark_dirty();
        let good = dir.join("config.json");
        s.save_if_dirty(&good);
        assert!(!s.take_dirty(), "gespeichert");
        let c = Config::load_from(&good);
        assert_eq!(DetachState::from_config(&c).get(Place::Tree), Some(win()));
        std::fs::remove_dir_all(&dir).ok();
    }
}
