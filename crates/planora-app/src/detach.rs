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

/// Alle Dialogschlüssel (`Dialog::dock_key`): fest und sprachunabhängig, je Dialog bzw. je
/// Feature-Art einer. Der Test `tests/dialog_dock_keys.rs` hält die Liste und die Aufrufe
/// gleich.
pub const DIALOG_KEYS: &[&str] = &[
    "about",
    "boolean",
    // Erzeugen (`create_dialog`), je Art
    "box",
    "cylinder",
    "sphere",
    "cone",
    "extrude",
    "revolve",
    // Verrunden und Fase (`blend_dialog`)
    "fillet",
    "chamfer",
    // Bearbeiten eines Features (`feature_dialog`), je Art; eigene Plätze, weil das Fenster
    // „Bearbeiten“ neben dem Erzeugen-Fenster derselben Art offen sein kann
    "edit_box",
    "edit_cylinder",
    "edit_sphere",
    "edit_cone",
    "edit_extrude",
    "edit_revolve",
    "edit_boolean",
    "edit_move",
    "edit_fillet",
    "edit_chamfer",
    "edit_feature",
    // Konstruktionsebenen (`plane_dialog`), je Befehl
    "plane_offset",
    "plane_angle",
    "plane_tangent",
    "plane_mid",
    "plane_three_points",
    // Einfügen (Skizzen 9)
    "insert_dxf",
    "insert_svg",
    "canvas",
    // Auf Fläche projizieren und Schnittkurve (Skizzen 10)
    "sketch_project_to_surface",
    "sketch_intersection_curve",
    // Krümmungsanzeige einrichten (Skizzen 11a)
    "curvature_display",
    // Verlorene Projektionen verwalten (Skizzen 11a)
    "lost_projections",
    // Baum und Zeichnung
    "tree_rename",
    "tree_delete",
    "drawing_rename",
    "drawing_delete",
    "dimension_delete",
    "dimension_edit",
    "drawing_new",
    "sheet_info",
    // Austausch
    "export",
    "mesh_import",
    "step_import",
    // Fenster
    "parameters",
    "projects",
    "settings",
    "shortcuts",
    "feedback",
    "update_done",
];

/// Die abdockbaren Felder (alle Plätze außer den Dialogen).
pub const FIELDS: [Place; 3] = [Place::Tree, Place::Right, Place::SketchSide];

impl Place {
    /// Ein Feld (Baum, rechte Spalte, Skizzenfeld), kein Dialog?
    pub fn is_field(self) -> bool {
        !matches!(self, Place::Dialog(_))
    }

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
fn state_id() -> egui::Id {
    egui::Id::new("planora_detach_state")
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

thread_local! {
    /// Nachgebildete Monitore (physische Pixel) für Tests, je Thread; `None`: echte Prüfung.
    static TEST_MONITORS: std::cell::RefCell<Option<Vec<egui::Rect>>> =
        const { std::cell::RefCell::new(None) };
}

/// Setzt für diesen Thread nachgebildete Monitore (für Tests): [`on_screen`] fragt dann nicht
/// das System, sondern prüft, ob der Kopfstreifen einen davon schneidet. So hängen die Tests
/// nicht vom echten Bildschirm des Rechners ab. `None` stellt die echte Prüfung wieder her.
#[doc(hidden)]
pub fn set_test_monitors(monitors: Option<Vec<egui::Rect>>) {
    TEST_MONITORS.with(|m| *m.borrow_mut() = monitors);
}

/// Prüft, ob der Kopfstreifen des Fensters (physische Pixel) auf einem Monitor liegt.
/// `None`, wenn die Plattform das nicht beantworten kann.
pub fn on_screen(rect_px: egui::Rect) -> Option<bool> {
    let head = egui::Rect::from_min_size(rect_px.min, egui::vec2(64.0, 24.0));
    // Wie `MonitorFromRect` mit `MONITOR_DEFAULTTONULL`: Ein Schnitt reicht.
    if let Some(hit) = TEST_MONITORS.with(|m| {
        m.borrow()
            .as_ref()
            .map(|ms| ms.iter().any(|s| s.intersects(head)))
    }) {
        return Some(hit);
    }
    system_on_screen(head)
}

#[cfg(windows)]
fn system_on_screen(head: egui::Rect) -> Option<bool> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONULL, MonitorFromRect};
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
fn system_on_screen(_head: egui::Rect) -> Option<bool> {
    None
}

/// Liegt der Platz `p` im eigenen Fenster? Nur wenn echte Fenster möglich sind (nicht
/// eingebettet) und sein gespeicherter Platz auf einem Bildschirm liegt.
///
/// Bei Feldern wird der Monitor nicht laufend geprüft (Spec „Monitor fehlt“), sondern nur, wenn
/// der Platz erscheint: beim Start und beim Wechsel in einen Modus, der ihn zeigt (die Aufrufer
/// fragen nur nach Plätzen, die der Modus zeigt; wer im vorigen Bild nicht gefragt wurde, wird
/// neu geprüft), und beim Abdocken. Ein Fenster, das danach außerhalb aller Monitore landet
/// (gezogen, Monitor abgezogen und von Windows verschoben), bleibt abgedockt.
pub fn is_undocked(ctx: &egui::Context, state: &DetachState, p: Place) -> bool {
    if ctx.embed_viewports() {
        return false;
    }
    let saved = state.get(p);
    if !p.is_field() {
        return should_detach(saved, on_screen).is_some();
    }
    let frame = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT);
    let id = field_memo_id(p);
    let memo = ctx
        .data(|d| d.get_temp::<FieldMemo>(id))
        .filter(|m| m.frame + 1 >= frame);
    let detached = field_detached(memo.map(|m| (m.saved, m.detached)), saved, on_screen);
    ctx.data_mut(|d| {
        d.insert_temp(
            id,
            FieldMemo {
                frame,
                saved,
                detached,
            },
        )
    });
    detached
}

/// Gedächtnis von [`is_undocked`] je Feld: Bild der letzten Frage (Hauptfenster), gemerkte Lage
/// und Antwort.
#[derive(Clone, Copy, Debug)]
struct FieldMemo {
    frame: u64,
    saved: Option<DetachedWindow>,
    detached: bool,
}

fn field_memo_id(p: Place) -> egui::Id {
    egui::Id::new(("detach_feld_entschieden", p.key()))
}

/// Entscheidung für ein Feld. `memo`: gemerkte Lage und Antwort, falls der Platz im vorigen Bild
/// schon erschien. Ohne Eintrag angedockt. Stand es abgedockt, bleibt es das (auch wenn sich die
/// Lage ändert). Stand es trotz Eintrag angedockt (Monitor fehlt), bleibt es angedockt, bis sich
/// der Eintrag ändert (Klick auf „Abdocken“). Sonst entscheidet der Monitor.
pub fn field_detached(
    memo: Option<(Option<DetachedWindow>, bool)>,
    saved: Option<DetachedWindow>,
    on_screen: impl Fn(egui::Rect) -> Option<bool>,
) -> bool {
    let Some(w) = saved else {
        return false;
    };
    match memo {
        Some((_, true)) => true,
        Some((s, false)) if s == Some(w) => false,
        _ => should_detach(Some(w), on_screen).is_some(),
    }
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

/// Weicht die gemeldete Lage (bei `with_size` auch die Größe) um mehr als
/// [`POSITION_TOLERANCE_PX`] von der gemerkten ab (physische Pixel)?
pub fn moved_by_system(saved: DetachedWindow, now: DetachedWindow, with_size: bool) -> bool {
    let far = |a: f32, b: f32| (a - b).abs() > POSITION_TOLERANCE_PX;
    far(saved.x, now.x)
        || far(saved.y, now.y)
        || (with_size && (far(saved.width, now.width) || far(saved.height, now.height)))
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
    /// Schließen-Symbol in der Kopfzeile (Dialoge mit `.open(..)`); ein Klick meldet
    /// [`WindowEvent::CloseRequested`].
    pub closable: bool,
    /// Art der Kopfzeile.
    pub head: Head,
}

/// Kopfzeile eines abgedockten Fensters.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Head {
    /// Graphit-Kopfzeile mit Titel, Symbol „Andocken“ und (bei `closable`) Schließen-Symbol;
    /// Doppelklick dockt an (Dialoge).
    Title,
    /// Keine eigene Zeile: Der Kopf des Inhalts (etwa „Feature-Baum“) ist die Kopfzeile. Oben
    /// rechts darüber steht „Andocken“ an derselben Stelle wie angedockt „Abdocken“; ein
    /// Streifen der Höhe `band` über dem Inhalt zieht das Fenster (Felder, siehe
    /// [`dock_band`]).
    Overlay { band: f32 },
    /// Graphit-Kopfzeile mit Titel, ohne Symbol „Andocken“ (Unterdialoge über einem Feld).
    TitleOnly,
}

/// Was im abgedockten Fenster in diesem Bild geschah.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowEvent {
    None,
    /// Symbol „Andocken“ oder Doppelklick auf die Kopfzeile.
    DockClicked,
    /// Schließen-Symbol (nur bei [`WindowSpec::closable`]) oder das Betriebssystem will das
    /// Fenster schließen (Alt+F4); das Schließen durch das Betriebssystem ist schon abgebrochen,
    /// der Aufrufer entscheidet (Feld andocken, Dialog abbrechen).
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
    /// Innere Größe im Builder (Punkte des Aufrufers) bei Fenstern mit Größenänderung: fest,
    /// solange das Fenster offen ist. Jede Änderung schickte egui als `InnerSize`, und winit
    /// rechnete sie mit dem Faktor des Kindfensters um; auf einem anders skalierten Monitor
    /// wüchse das Feld dann nach jedem Ziehen.
    size: egui::Vec2,
    /// Gewollte physische Größe beim Öffnen (für die Korrektur im zweiten Bild; nur Fenster mit
    /// Größenänderung und gemerkter Lage).
    wanted_size: Option<egui::Vec2>,
    /// Gewollte physische Lage beim Öffnen (für die Korrektur im zweiten Bild).
    wanted: Option<egui::Pos2>,
    /// Eine Korrektur der Lage bzw. Größe ist unterwegs (gewollte physische Werte): Bis das
    /// Fenster dort gemeldet wird, übernimmt [`window_contents`] seine Lage nicht in den Zustand
    /// (sie wäre die falsche vor der Korrektur).
    correcting: Option<egui::Pos2>,
    correcting_size: Option<egui::Vec2>,
    /// Äußeres Rechteck (Punkte des Fensters) beim Beginn eines Zieh- oder Größenvorgangs.
    moving: Option<egui::Rect>,
    /// Bild des Aufrufers, in dem das Schließen zuletzt abgebrochen bzw. ein Zieh- oder
    /// Größenvorgang begonnen wurde: Läuft das Kindfenster in einem Bild mit mehreren
    /// Durchgängen (`request_discard`) mehrmals, geht jeder Befehl nur einmal ans Fenster.
    close_frame: Option<u64>,
    drag_frame: Option<u64>,
}

/// Zeigt `add_contents` als eigenes Betriebssystemfenster für den Platz `spec.place`: ohne
/// Rahmen des Betriebssystems und ohne Taskleisten-Eintrag, mit der Kopfzeile nach `spec.head`
/// (Symbol „Andocken“), Ziehen an der Kopfzeile und, falls `resizable`, Rändern zum Ändern der
/// Größe. Lage aus `state`; nach dem Ende eines Zieh- oder Größenvorgangs wird die neue Lage
/// (physische Pixel) in `state` gemerkt.
///
/// Mit [`show_sub_window`] der einzige Ort, der ein eigenes Fenster erzeugt. `add_contents` ist
/// `FnMut`, weil egui
/// den Inhalt eines Kindfensters in einem Bild mehrmals ausführen kann (verworfene Durchgänge,
/// `request_discard`); geliefert wird das Ergebnis des letzten Durchgangs.
///
/// Bei eingebetteten Viewports ([`can_detach`] ist `false`) öffnet die Funktion nie ein
/// Fenster und liefert `(None, WindowEvent::None)`; Aufrufer prüfen das vorher.
pub fn show_window<R>(
    ctx: &egui::Context,
    state: &mut DetachState,
    spec: WindowSpec,
    add_contents: impl FnMut(&mut egui::Ui) -> R,
) -> (Option<R>, WindowEvent) {
    if !can_detach(ctx) {
        debug_assert!(
            false,
            "show_window without native windows ({:?}); check can_detach first",
            spec.place
        );
        return (None, WindowEvent::None);
    }
    let saved = state.get(spec.place);
    let memory = egui::Id::new(("detach_fenster", spec.place.key()));
    show_core(
        ctx,
        Some(state),
        (viewport_id(spec.place), memory),
        Placement::Saved(saved),
        spec,
        add_contents,
    )
}

/// Wo ein Fenster steht.
#[derive(Clone, Copy)]
enum Placement {
    /// Gemerkte Lage (physische Pixel) beim Öffnen; danach fest, bis es zu ist.
    Saved(Option<DetachedWindow>),
    /// Linke obere Ecke in Punkten des Aufrufers, in jedem Bild neu (Unterdialoge: mittig über
    /// ihrem Feld, auch wenn ihre Höhe sich ändert).
    Follow(egui::Pos2),
}

/// Gemeinsamer Teil von [`show_window`] und [`show_sub_window`]; `state` nur bei gemerkten
/// Plätzen.
fn show_core<R>(
    ctx: &egui::Context,
    mut state: Option<&mut DetachState>,
    (id, memory): (egui::ViewportId, egui::Id),
    placement: Placement,
    spec: WindowSpec,
    mut add_contents: impl FnMut(&mut egui::Ui) -> R,
) -> (Option<R>, WindowEvent) {
    let frame = ctx.cumulative_frame_nr();
    // Punkte des Aufrufers: der Builder gibt die Lage logisch an winit weiter.
    let ppp = ctx.pixels_per_point();
    let mut opened = ctx
        .data(|d| d.get_temp::<Opened>(memory))
        .filter(|o| o.parent_frame + 1 >= frame)
        .unwrap_or_else(|| {
            let (position, wanted) = match placement {
                Placement::Saved(saved) => (
                    saved.map(|w| to_points(w, ppp).min),
                    saved.map(|w| egui::pos2(w.x, w.y)),
                ),
                Placement::Follow(p) => (Some(p), Some((p.to_vec2() * ppp).to_pos2())),
            };
            Opened {
                parent_frame: frame,
                position,
                wanted,
                size: spec.inner_size,
                wanted_size: match placement {
                    Placement::Saved(Some(w)) if spec.resizable => {
                        Some(egui::vec2(w.width, w.height))
                    }
                    _ => None,
                },
                correcting: None,
                correcting_size: None,
                moving: None,
                close_frame: None,
                drag_frame: None,
            }
        });
    opened.parent_frame = frame;
    if let Placement::Follow(p) = placement {
        opened.position = Some(p);
    }

    let mut builder = egui::ViewportBuilder::default()
        .with_title(format!("PlanoraCAD – {}", spec.title))
        .with_decorations(false)
        .with_taskbar(false)
        .with_inner_size(if spec.resizable {
            opened.size
        } else {
            spec.inner_size
        })
        .with_resizable(spec.resizable)
        .with_active(spec.focus_on_open);
    if let Some(p) = opened.position {
        builder = builder.with_position(p);
    }
    let out = ctx.show_viewport_immediate(id, builder, |ui, _class| {
        window_contents(
            ui,
            state.as_deref_mut(),
            &mut opened,
            &spec,
            &mut add_contents,
        )
    });
    ctx.data_mut(|d| d.insert_temp(memory, opened));
    (Some(out.0), out.1)
}

/// Kennung des Fensters eines Unterdialogs (Dialog-Id `dialog`) über dem Feld `host`.
pub fn sub_viewport_id(host: Place, dialog: egui::Id) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(("detach_unterdialog", host.key(), dialog))
}

/// Angaben eines Unterdialogs für [`show_sub_window`].
#[derive(Clone, Copy, Debug)]
pub struct SubWindow<'a> {
    /// Feld, über dem der Unterdialog steht.
    pub host: Place,
    /// Id des Dialogs (für die Kennung des Fensters).
    pub dialog: egui::Id,
    pub title: &'a str,
    /// Innere Größe in Punkten.
    pub inner_size: egui::Vec2,
    /// Linke obere Ecke in Punkten des Feldfensters, in jedem Bild neu.
    pub position: egui::Pos2,
    /// Schließen-Symbol in der Kopfzeile.
    pub closable: bool,
}

/// Zeigt einen Unterdialog (Rückfrage, Umbenennen) eines abgedockten Felds als eigenes kleines
/// Fenster: Kopfzeile mit Titel und (bei `closable`) Schließen-Symbol, ohne „Andocken“, ohne
/// Größenänderung; nichts wird gemerkt. Solange er offen ist, ist das Feld gesperrt
/// ([`sub_dialog_open`]). Bei eingebetteten Viewports `(None, WindowEvent::None)`.
pub fn show_sub_window<R>(
    ctx: &egui::Context,
    sub: SubWindow,
    add_contents: impl FnMut(&mut egui::Ui) -> R,
) -> (Option<R>, WindowEvent) {
    if !can_detach(ctx) {
        return (None, WindowEvent::None);
    }
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| d.insert_temp(sub_open_id(sub.host), frame));
    let spec = WindowSpec {
        place: sub.host,
        title: sub.title,
        inner_size: sub.inner_size,
        resizable: false,
        focus_on_open: true,
        closable: sub.closable,
        head: Head::TitleOnly,
    };
    let memory = egui::Id::new(("detach_unterfenster", sub.host.key(), sub.dialog));
    show_core(
        ctx,
        None,
        (sub_viewport_id(sub.host, sub.dialog), memory),
        Placement::Follow(sub.position),
        spec,
        add_contents,
    )
}

fn sub_open_id(host: Place) -> egui::Id {
    egui::Id::new(("detach_unterdialog_offen", host.key()))
}

/// Ist über dem Feld `host` ein Unterdialog offen (im vorigen oder diesem Bild des
/// Feldfensters gezeigt)? Dann ist das Feld gesperrt wie der Hintergrund einer Rückfrage.
pub fn sub_dialog_open(ctx: &egui::Context, host: Place) -> bool {
    let frame = ctx.cumulative_frame_nr_for(viewport_id(host));
    ctx.data(|d| d.get_temp::<u64>(sub_open_id(host)))
        .is_some_and(|f| f + 1 >= frame)
}

/// Höhe des Streifens oben im Feld, in dem das Symbol „Abdocken“/„Andocken“ steht: die Höhe
/// des Kopfs, den das Feld oben zeigt (Skizzenfeld: Zeile mit Name und „Profil anzeigen“).
pub fn dock_band(place: Place) -> f32 {
    match place {
        Place::SketchSide => theme::ROW_H,
        _ => theme::HEADER_H,
    }
}

/// Das abgedockte Feld, in dessen Fenster gerade gezeichnet wird.
pub fn host_place(ctx: &egui::Context) -> Option<Place> {
    if !can_detach(ctx) {
        return None;
    }
    let current = ctx.viewport_id();
    FIELDS.into_iter().find(|p| viewport_id(*p) == current)
}

fn host_rect_id(place: Place) -> egui::Id {
    egui::Id::new(("detach_feldfenster", place.key()))
}

/// Bildschirm-Rechteck (Punkte) des Fensters, in dem gerade gezeichnet wird, wenn es ein
/// abgedocktes **Feld** ist; sonst `None` (Hauptfenster, abgedockter Dialog, eingebettet). Meldet
/// das Betriebssystem die Lage nicht (Wayland, Tests): Lage beim Öffnen, Größe des Builders.
pub fn host_viewport_rect(ctx: &egui::Context) -> Option<egui::Rect> {
    let place = host_place(ctx)?;
    ctx.input(|i| i.viewport().outer_rect)
        .or_else(|| ctx.data(|d| d.get_temp::<egui::Rect>(host_rect_id(place))))
}

/// Inhalt des Kindfensters (läuft mit dessen Eingabe und Faktor).
fn window_contents<R>(
    ui: &mut egui::Ui,
    mut state: Option<&mut DetachState>,
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
        opened.correcting = Some(wanted);
    }
    // Ebenso die Größe eines Felds: der Builder gibt sie in Punkten des Aufrufers an, winit
    // nimmt sie mit dem Faktor des Kindfensters. Einmal physisch genau setzen.
    if ctx.cumulative_frame_nr() == 1
        && let Some(wanted) = opened.wanted_size.take()
        && let Some(outer) = outer
        && (outer.size() * ppp - wanted).abs().max_elem() > POSITION_TOLERANCE_PX
    {
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(wanted / ppp));
        opened.correcting_size = Some(wanted);
    }
    // Angekommen: die Korrektur ist erledigt.
    if let Some(outer) = outer {
        let near = |a: egui::Vec2, b: egui::Vec2| (a - b).abs().max_elem() <= POSITION_TOLERANCE_PX;
        if opened
            .correcting
            .is_some_and(|w| near(outer.min.to_vec2() * ppp, w.to_vec2()))
        {
            opened.correcting = None;
        }
        if opened
            .correcting_size
            .is_some_and(|w| near(outer.size() * ppp, w))
        {
            opened.correcting_size = None;
        }
    }

    let parent_frame = opened.parent_frame;
    if ctx.input(|i| i.viewport().close_requested()) && opened.close_frame != Some(parent_frame) {
        opened.close_frame = Some(parent_frame);
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        event = WindowEvent::CloseRequested;
    }

    if spec.place.is_field() && spec.head != Head::TitleOnly {
        // Für Unterdialoge des Felds ([`host_viewport_rect`]), falls das Betriebssystem die Lage
        // nicht meldet.
        let rect =
            egui::Rect::from_min_size(opened.position.unwrap_or(egui::Pos2::ZERO), spec.inner_size);
        ctx.data_mut(|d| d.insert_temp(host_rect_id(spec.place), rect));
    }

    let dark = ui.visuals().dark_mode;
    // Rahmen der Felder, aber eckig: das Fenster selbst ist rechteckig, gerundete Ecken
    // zeigten dahinter die leere Fensterfläche.
    let frame = theme::panel_frame(dark).corner_radius(egui::CornerRadius::ZERO);
    let r = egui::CentralPanel::default()
        .frame(frame)
        .show(ui, |ui| {
            let mut head_event =
                |clicks: crate::dialog::HeaderClicks, double: bool, drag: &egui::Response| {
                    if clicks.close {
                        event = WindowEvent::CloseRequested;
                    } else if clicks.dock || double {
                        event = WindowEvent::DockClicked;
                    } else if drag.drag_started_by(egui::PointerButton::Primary)
                        && opened.drag_frame != Some(parent_frame)
                    {
                        opened.drag_frame = Some(parent_frame);
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        opened.moving = outer;
                    }
                };
            match spec.head {
                Head::Title | Head::TitleOnly => {
                    // Fläche der Kopfzeile vor deren Symbolen: die Symbole liegen darüber und
                    // behalten ihre Klicks, der Rest der Zeile zieht das Fenster bzw. dockt per
                    // Doppelklick an.
                    let head = egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
                    );
                    let drag = ui.interact(
                        head,
                        ui.id().with("detach_kopfzeile"),
                        egui::Sense::click_and_drag(),
                    );
                    let dock =
                        (spec.head == Head::Title).then_some(crate::dialog::DockButton::Dock);
                    let clicks = crate::dialog::header(ui, spec.title, spec.closable, dock);
                    head_event(clicks, dock.is_some() && drag.double_clicked(), &drag);
                    add_contents(ui)
                }
                Head::Overlay { band } => {
                    // Gesperrt, solange ein Unterdialog des Felds offen ist.
                    let locked = sub_dialog_open(&ctx, spec.place);
                    let area = ui.max_rect();
                    let r = ui.add_enabled_ui(!locked, |ui| add_contents(ui)).inner;
                    // Nach dem Inhalt angelegt, damit der Streifen über ihm liegt: Ziehen im Kopf
                    // des Inhalts zieht das Fenster (statt den Inhalt zu rollen); Klicks behalten
                    // die Knöpfe darunter, weil der Streifen nur Ziehen annimmt.
                    let strip = egui::Rect::from_min_size(area.min, egui::vec2(area.width(), band));
                    let drag = ui.interact(
                        strip,
                        ui.id().with("detach_kopfstreifen"),
                        egui::Sense::drag(),
                    );
                    let dock = ui
                        .add_enabled_ui(!locked, |ui| {
                            crate::dialog::corner_dock_icon(
                                ui,
                                area,
                                band,
                                crate::dialog::DockButton::Dock,
                            )
                        })
                        .inner;
                    let clicks = crate::dialog::HeaderClicks { close: false, dock };
                    head_event(clicks, false, &drag);
                    r
                }
            }
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
            && let Some(state) = state.as_deref_mut()
        {
            state.set(spec.place, to_physical(now, ppp));
        }
    }
    // Lage oder Größe ohne Ziehen an Kopfzeile oder Rand geändert (Win+Umschalt+Pfeil,
    // Einrasten, anderer DPI-Faktor): ab dem dritten Bild des Fensters, ohne gedrückte Maustaste
    // und ohne unterwegs befindliche Korrektur übernehmen, was das Betriebssystem meldet. Nicht,
    // wenn der Monitor der gemerkten Lage fehlt (Windows hat das Fenster dann verschoben; der
    // Eintrag bleibt, Spec Ziel 3). Bei Dialogen zählt nur die Lage (die Höhe folgt dem Inhalt).
    if opened.moving.is_none()
        && opened.correcting.is_none()
        && opened.correcting_size.is_none()
        && ctx.cumulative_frame_nr() >= 2
        && !ctx.input(|i| i.pointer.any_down())
        && let Some(now) = outer
        && let Some(state) = state
        && let Some(saved) = state.get(spec.place)
    {
        let now_px = to_physical(now, ppp);
        if moved_by_system(saved, now_px, spec.resizable)
            && should_detach(Some(saved), on_screen).is_some()
        {
            state.set(spec.place, now_px);
        }
    }
    // Tastenkürzel: was der Inhalt nicht verbraucht hat, führt die App am Ende des Bilds aus
    // ([`take_child_keys`]); danach merkt sich das Fenster seinen Tastaturfokus für das nächste
    // Bild (Esc nimmt egui den Fokus schon zu Beginn des Bilds).
    let dialog = !spec.place.is_field() || spec.head == Head::TitleOnly;
    collect_keys(&ctx, dialog);
    crate::viewport::remember_keyboard_focus(&ctx);
    (r, event)
}

/// Unverbrauchte Tastenereignisse eines abgedockten Fensters aus einem Bild, für die
/// Tastenkürzel der App (`PlanoraApp::shortcuts_from_children`). Jedes Ereignis gehört nur dem
/// Fenster, das es empfangen hat: Es wird dort herausgenommen und genau einmal ausgeführt.
#[derive(Clone, Debug)]
pub struct ChildKeys {
    /// Bild des Hauptfensters, in dem die Tasten kamen (ältere verfallen).
    pub frame: u64,
    /// Fenster, das die Tasten empfing (etwa das Baumfenster: dessen Rückfragen erscheinen
    /// über ihm).
    pub viewport: egui::ViewportId,
    /// Ein Dialog oder Unterdialog (nur Kürzel mit Bereich „Überall“), kein Feld.
    pub dialog: bool,
    /// Ein Eingabefeld im Fenster hat die Tastatur (dann gehören ihm alle Tasten).
    pub typing: bool,
    /// Ein Bedienelement, das Tasten liest, hat dort den Fokus (die Leertaste bleibt ihm).
    pub focus: bool,
    /// Eine Maustaste ist im Fenster gedrückt (Rückgängig wartet wie im Hauptfenster).
    pub pointer_down: bool,
    /// Die gedrückten Tasten (`Event::Key` mit `pressed`).
    pub events: Vec<egui::Event>,
}

fn child_keys_id() -> egui::Id {
    egui::Id::new("detach_kind_tasten")
}

/// Nimmt die unverbrauchten gedrückten Tasten aus der Eingabe des Fensters, in dem gerade
/// gezeichnet wird, und legt sie für [`take_child_keys`] ab. Ereignisse kommen nur im ersten
/// Durchgang eines Bilds; herausgenommen zählt jedes höchstens einmal, auch wenn der Inhalt in
/// einem Bild mehrmals läuft.
fn collect_keys(ctx: &egui::Context, dialog: bool) {
    // Eingabe im Kindfenster: Das Hauptfenster hat seine 3D-Ansicht in diesem Bild schon
    // gezeichnet (Dialoge laufen danach). Ein weiteres Bild gibt den geänderten Wert (Vorschau,
    // Griffe, Hervorhebung) ohne Mausbewegung im Hauptfenster weiter; mehrfaches Anfordern ist
    // unschädlich.
    if ctx.input(|i| !i.events.is_empty()) {
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
    // Klick oder Taste im Fenster: Das Hauptfenster gibt seinen Tastaturfokus ab, wie es
    // angedockt ein Klick daneben täte (egui zählt einen Klick in einem anderen Fenster nicht
    // als „daneben“); sonst behielte ein Feld dort alle weitergereichten Tasten.
    let touched = ctx.input(|i| {
        i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key { pressed: true, .. }
                    | egui::Event::PointerButton { pressed: true, .. }
            )
        })
    });
    if touched {
        ctx.data_mut(|d| d.insert_temp(child_touched_id(), true));
    }
    let events: Vec<egui::Event> = ctx.input_mut(|i| {
        let (keys, rest) = std::mem::take(&mut i.events)
            .into_iter()
            .partition(|e| matches!(e, egui::Event::Key { pressed: true, .. }));
        i.events = rest;
        keys
    });
    if events.is_empty() {
        return;
    }
    let keys = ChildKeys {
        frame: ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT),
        viewport: ctx.viewport_id(),
        dialog,
        typing: crate::viewport::typing(ctx),
        focus: crate::viewport::keyboard_taken(ctx),
        pointer_down: ctx.input(|i| i.pointer.any_down()),
        events,
    };
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Vec<ChildKeys>>(child_keys_id())
            .push(keys);
    });
}

fn child_touched_id() -> egui::Id {
    egui::Id::new("detach_kind_beruehrt")
}

/// Wurde seit dem letzten Aufruf in einem abgedockten Fenster geklickt oder eine Taste gedrückt?
/// Setzt zurück.
pub fn take_child_touched(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.remove_temp::<bool>(child_touched_id()))
        .unwrap_or(false)
}

/// Holt die in diesem Bild abgelegten Tasten der abgedockten Fenster (in der Reihenfolge der
/// Fenster) und leert die Ablage; ältere Einträge (aus einem Bild, das sie nicht abholte)
/// verfallen.
pub fn take_child_keys(ctx: &egui::Context) -> Vec<ChildKeys> {
    let frame = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT);
    let mut list = ctx
        .data_mut(|d| d.remove_temp::<Vec<ChildKeys>>(child_keys_id()))
        .unwrap_or_default();
    list.retain(|k| k.frame == frame);
    list
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
        if response.drag_started_by(egui::PointerButton::Primary)
            && opened.drag_frame != Some(opened.parent_frame)
        {
            opened.drag_frame = Some(opened.parent_frame);
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
    fn keys_per_kind_are_listed() {
        for c in crate::plane_dialog::PlaneCommand::ALL {
            assert!(DIALOG_KEYS.contains(&c.dock_key()), "{c:?}");
        }
    }

    #[test]
    fn with_state_lends_installed_state() {
        let ctx = egui::Context::default();
        assert!(!is_installed(&ctx));
        assert_eq!(with_state(&ctx, |_| 1), None, "ohne install nichts");
        let mut s = DetachState::default();
        s.set(Place::Tree, win());
        install(&ctx, &mut s);
        assert_eq!(s.get(Place::Tree), None, "abgelegt");
        assert!(is_installed(&ctx));
        let inner = with_state(&ctx, |st| {
            st.undock(Place::Tree);
            // Ausgeliehen: ein verschachtelter Aufruf findet keinen Zustand.
            (is_installed(&ctx), with_state(&ctx, |_| ()))
        });
        assert_eq!(inner, Some((false, None)));
        uninstall(&ctx, &mut s);
        assert_eq!(s.get(Place::Tree), None, "Änderung zurück");
        assert!(s.take_dirty());
        assert!(!is_installed(&ctx));
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
    fn field_decision_checks_monitor_only_when_place_appears() {
        let off = |_| Some(false);
        let on = |_| Some(true);
        // Kein Eintrag: angedockt.
        assert!(!field_detached(Some((None, true)), None, on));
        // Erscheint (kein Gedächtnis): der Monitor entscheidet.
        assert!(field_detached(None, Some(win()), on));
        assert!(!field_detached(None, Some(win()), off));
        // Stand abgedockt: bleibt es, auch wenn die neue Lage außerhalb liegt.
        let moved = DetachedWindow { x: -9e4, ..win() };
        assert!(field_detached(Some((Some(win()), true)), Some(moved), off));
        // Stand angedockt trotz Eintrag (Monitor fehlt): bleibt, bis sich der Eintrag ändert.
        assert!(!field_detached(Some((Some(win()), false)), Some(win()), on));
        assert!(field_detached(Some((Some(win()), false)), Some(moved), on));
        assert!(field_detached(Some((None, false)), Some(win()), on));
    }

    #[test]
    fn moved_by_system_tolerance() {
        let w = win();
        assert!(!moved_by_system(
            w,
            DetachedWindow { x: w.x + 1.5, ..w },
            true
        ));
        assert!(moved_by_system(
            w,
            DetachedWindow { x: w.x + 3.0, ..w },
            false
        ));
        let taller = DetachedWindow {
            height: w.height + 40.0,
            ..w
        };
        assert!(!moved_by_system(w, taller, false), "Dialoge: nur die Lage");
        assert!(moved_by_system(w, taller, true));
    }

    #[test]
    fn uninstall_keeps_dirty_mark_of_placeholder() {
        let ctx = egui::Context::default();
        let mut s = DetachState::default();
        install(&ctx, &mut s);
        assert!(s.windows().is_none(), "verliehen");
        s.mark_dirty();
        uninstall(&ctx, &mut s);
        assert!(s.windows().is_some());
        assert!(s.take_dirty(), "neu schreiben");
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
