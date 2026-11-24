//! Eigenes Fenster und Unterfenster.

use super::*;

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
pub(super) const RESIZE_EDGE: f32 = 6.0;
/// Größte Abweichung der Lage (physische Pixel), die ohne Korrektur bleibt.
pub(super) const POSITION_TOLERANCE_PX: f32 = 2.0;

/// Gedächtnis eines offenen Fensters zwischen den Bildern.
#[derive(Clone, Copy, Debug)]
pub(super) struct Opened {
    /// Bild des Aufrufers, in dem das Fenster zuletzt gezeigt wurde. Fehlt ein Bild, war das
    /// Fenster zu (egui entfernt ein Kindfenster, das in einem Bild nicht gezeigt wird).
    pub(super) parent_frame: u64,
    /// Lage im Builder (Punkte des Aufrufers). Bleibt fest, solange das Fenster offen ist:
    /// egui schickt bei jeder Änderung des Builders die Lage erneut, winit rechnete sie mit dem
    /// Faktor des Fensters um und das Fenster spränge.
    pub(super) position: Option<egui::Pos2>,
    /// Innere Größe im Builder (Punkte des Aufrufers) bei Fenstern mit Größenänderung: fest,
    /// solange das Fenster offen ist. Jede Änderung schickte egui als `InnerSize`, und winit
    /// rechnete sie mit dem Faktor des Kindfensters um; auf einem anders skalierten Monitor
    /// wüchse das Feld dann nach jedem Ziehen.
    pub(super) size: egui::Vec2,
    /// Gewollte physische Größe beim Öffnen (für die Korrektur im zweiten Bild; nur Fenster mit
    /// Größenänderung und gemerkter Lage).
    pub(super) wanted_size: Option<egui::Vec2>,
    /// Gewollte physische Lage beim Öffnen (für die Korrektur im zweiten Bild).
    pub(super) wanted: Option<egui::Pos2>,
    /// Eine Korrektur der Lage bzw. Größe ist unterwegs (gewollte physische Werte): Bis das
    /// Fenster dort gemeldet wird, übernimmt [`window_contents`] seine Lage nicht in den Zustand
    /// (sie wäre die falsche vor der Korrektur).
    pub(super) correcting: Option<egui::Pos2>,
    pub(super) correcting_size: Option<egui::Vec2>,
    /// Äußeres Rechteck (Punkte des Fensters) beim Beginn eines Zieh- oder Größenvorgangs.
    pub(super) moving: Option<egui::Rect>,
    /// Bild des Aufrufers, in dem das Schließen zuletzt abgebrochen bzw. ein Zieh- oder
    /// Größenvorgang begonnen wurde: Läuft das Kindfenster in einem Bild mit mehreren
    /// Durchgängen (`request_discard`) mehrmals, geht jeder Befehl nur einmal ans Fenster.
    pub(super) close_frame: Option<u64>,
    pub(super) drag_frame: Option<u64>,
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
    let memory = egui::Id::unique(("detach_fenster", spec.place.key()));
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
pub(super) enum Placement {
    /// Gemerkte Lage (physische Pixel) beim Öffnen; danach fest, bis es zu ist.
    Saved(Option<DetachedWindow>),
    /// Linke obere Ecke in Punkten des Aufrufers, in jedem Bild neu (Unterdialoge: mittig über
    /// ihrem Feld, auch wenn ihre Höhe sich ändert).
    Follow(egui::Pos2),
}

/// Gemeinsamer Teil von [`show_window`] und [`show_sub_window`]; `state` nur bei gemerkten
/// Plätzen.
pub(super) fn show_core<R>(
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
    let memory = egui::Id::unique(("detach_unterfenster", sub.host.key(), sub.dialog));
    show_core(
        ctx,
        None,
        (sub_viewport_id(sub.host, sub.dialog), memory),
        Placement::Follow(sub.position),
        spec,
        add_contents,
    )
}

pub(super) fn sub_open_id(host: Place) -> egui::Id {
    egui::Id::unique(("detach_unterdialog_offen", host.key()))
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

pub(super) fn host_rect_id(place: Place) -> egui::Id {
    egui::Id::unique(("detach_feldfenster", place.key()))
}

/// Bildschirm-Rechteck (Punkte) des Fensters, in dem gerade gezeichnet wird, wenn es ein
/// abgedocktes **Feld** ist; sonst `None` (Hauptfenster, abgedockter Dialog, eingebettet). Meldet
/// das Betriebssystem die Lage nicht (Wayland, Tests): Lage beim Öffnen, Größe des Builders.
pub fn host_viewport_rect(ctx: &egui::Context) -> Option<egui::Rect> {
    let place = host_place(ctx)?;
    ctx.input(|i| i.viewport().outer_rect)
        .or_else(|| ctx.data(|d| d.get_temp::<egui::Rect>(host_rect_id(place))))
}
