//! Dialog-Rahmen im Graphit-Stil: schwebendes Fenster im Rahmen der Felder ([`theme::panel_frame`]),
//! drei feste Breiten, mittig im Fenster, eigene Kopfzeile mit Titel und Schließen-Symbol. Der
//! Dialog ist so hoch wie sein Inhalt; nur zu hoher Inhalt scrollt (unter der stehenden
//! Kopfzeile). Der Dialog bleibt ganz im Fenster.

use crate::detach::{self, Place, WindowEvent, WindowSpec};
use crate::icons::{self, Icon};
use crate::shell::SHELL_INSET;
use crate::theme::{self, SPACE_2};
use egui::{Align, Align2, Layout};
use planora_i18n::tr;

/// Name des Schließen-Symbols (Tooltip und Bedienhilfe).
pub fn close_label() -> String {
    tr!("dialog-close")
}

/// Steht der Dialog, in dessen Inhalt `ui` zeichnet, in einem eigenen Fenster (abgedockt oder
/// als Unterdialog über einem abgedockten Feld)?
pub fn in_own_window(ui: &egui::Ui) -> bool {
    ui.ctx().viewport_id() != egui::ViewportId::ROOT
}

/// Esc für einen Befehlsdialog (wie „Abbrechen“), im Inhalt des Dialogs aufgerufen, damit es die
/// Eingabe des Fensters liest, in dem der Dialog steht: Esc dort, solange dort kein Feld tippt.
/// Steht der Dialog in einem eigenen Fenster, zählt auch Esc im Hauptfenster, das dort niemand
/// verbraucht hat und während dort kein Feld tippt (wie Esc in Fusions Zeichenfläche den Befehl
/// abbricht); Enter des Hauptfensters zählt nie. `consume`: das Esc verbrauchen (es wirkt dann
/// nur hier).
pub fn escape_pressed(ui: &egui::Ui, consume: bool) -> bool {
    let ctx = ui.ctx();
    let esc = |i: &egui::InputState| i.key_pressed(egui::Key::Escape);
    let take = |i: &mut egui::InputState| {
        if consume {
            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
        }
    };
    if !crate::viewport::typing(ctx) && ctx.input(esc) {
        ctx.input_mut(take);
        return true;
    }
    let root = egui::ViewportId::ROOT;
    if in_own_window(ui) && !crate::viewport::root_typing(ctx) && ctx.input_for(root, esc) {
        ctx.input_mut_for(root, take);
        return true;
    }
    false
}

/// Größe der Symbole (Schließen, An-/Abdocken) in der Kopfzeile.
const HEADER_ICON_SIZE: f32 = 16.0;

/// Symbol „Andocken“ bzw. „Abdocken“ in der Kopfzeile (links neben dem Schließen-Symbol).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DockButton {
    /// Abgedocktes Fenster: zurück ins Hauptfenster.
    Dock,
    /// Angedockt: als eigenes Fenster herauslösen.
    Undock,
}

impl DockButton {
    fn icon(self) -> Icon {
        match self {
            DockButton::Dock => Icon::Dock,
            DockButton::Undock => Icon::Undock,
        }
    }

    /// Name des Symbols (Tooltip und Bedienhilfe).
    pub(crate) fn label(self) -> String {
        match self {
            DockButton::Dock => tr!("dialog-dock"),
            DockButton::Undock => tr!("dialog-undock"),
        }
    }
}

/// Welche Symbole der Kopfzeile angeklickt wurden.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(crate) struct HeaderClicks {
    pub close: bool,
    pub dock: bool,
}

/// Breitenklassen der Dialoge (Außenbreite in Punkten).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DialogSize {
    /// Rückfragen und kurze Hinweise.
    Prompt,
    /// Dialoge mit einigen Feldern.
    Normal,
    /// Dialoge mit Karten oder Listen nebeneinander.
    Wide,
    /// Fenster mit Dateiauswahl (Exportieren, Netz und STEP importieren): mindestens so breit
    /// wie [`DialogSize::Wide`], in großen Fenstern breiter (siehe [`limits`]).
    File,
}

impl DialogSize {
    /// Außenbreite der Klasse.
    pub const fn width(self) -> f32 {
        match self {
            DialogSize::Prompt => 440.0,
            DialogSize::Normal => 480.0,
            DialogSize::Wide | DialogSize::File => 720.0,
        }
    }
}

/// Anteil der Fensterbreite, den ein Dateifenster einnimmt.
pub const FILE_WIDTH_SHARE: f32 = 0.55;
/// Größte Breite eines Dateifensters.
pub const FILE_MAX_WIDTH: f32 = 1200.0;

/// Rand zur Monitorhöhe, den ein abgedockter Dialog frei lässt (Taskleiste, Luft); höher wird
/// er nicht, darüber rollt sein Inhalt.
pub const DETACHED_MONITOR_MARGIN: f32 = 80.0;

/// Außenbreite und größte Außenhöhe im Fenster `content` (Rand [`SHELL_INSET`] rundum). Ein
/// Dateifenster ist [`FILE_WIDTH_SHARE`] der Fensterbreite breit, mindestens 720 und höchstens
/// [`FILE_MAX_WIDTH`] px.
pub fn limits(content: egui::Rect, size: DialogSize) -> (f32, f32) {
    let free = content.shrink(SHELL_INSET);
    let wanted = match size {
        DialogSize::File => {
            (FILE_WIDTH_SHARE * content.width()).clamp(DialogSize::Wide.width(), FILE_MAX_WIDTH)
        }
        other => other.width(),
    };
    (wanted.min(free.width()), free.height())
}

/// Ein Dialog; Aufbau wie bei `egui::Window`.
pub struct Dialog<'a> {
    title: String,
    id: egui::Id,
    size: DialogSize,
    open: Option<&'a mut bool>,
    /// Ohne Schließen-Symbol: meldet das Schließen des eigenen Fensters durch das
    /// Betriebssystem ([`Dialog::close_request`]).
    close_request: Option<&'a mut bool>,
    fixed: bool,
    center_on_open: bool,
    /// Fest unten rechts in diesem Bereich statt mittig ([`Dialog::bottom_right_in`]).
    dock: Option<egui::Rect>,
    /// Platz zum Abdocken ([`Dialog::dock_key`]).
    dock_key: Option<&'static str>,
}

/// Ob ein abdockbarer Dialog in diesem Öffnen abgedockt steht (Bild der letzten Anzeige,
/// abgedockt). Entschieden beim Öffnen (Monitorprüfung), danach durch die Symbole.
#[derive(Clone, Copy, Debug)]
struct DetachMemo {
    frame: u64,
    detached: bool,
}

/// Stand der Dialog mit der Id `id` im vorigen (oder diesem) Bild abgedockt im eigenen Fenster?
/// Für Tasten, die außerhalb des Dialogs vor den Feldern gelesen werden (Esc der Übersicht):
/// abgedockt gehören sie dem Fenster des Dialogs, nicht dem Hauptfenster.
pub fn shown_detached(ctx: &egui::Context, id: egui::Id) -> bool {
    let frame = ctx.cumulative_frame_nr();
    ctx.data(|d| d.get_temp::<DetachMemo>(id.with("dialog_abgedockt")))
        .is_some_and(|m| m.detached && m.frame + 1 >= frame)
}

impl<'a> Dialog<'a> {
    /// Neuer Dialog; die Id hängt wie bei `egui::Window::new` am Titel.
    pub fn new(title: impl Into<String>, size: DialogSize) -> Self {
        let title = title.into();
        Dialog {
            id: egui::Id::new(&title),
            title,
            size,
            open: None,
            close_request: None,
            fixed: false,
            center_on_open: false,
            dock: None,
            dock_key: None,
        }
    }

    /// Abdockbar unter dem festen, sprachunabhängigen Schlüssel `key` (aus
    /// [`detach::DIALOG_KEYS`]): Mit eigenen Fenstern zeigt die Kopfzeile links vom
    /// Schließen-Symbol das Symbol „Abdocken“; abgedockt steht der Dialog als eigenes Fenster an
    /// der gemerkten Lage. Ohne Schlüssel, bei festen Dialogen und ohne abgelegten
    /// Abdock-Zustand ([`detach::install`]) wie bisher.
    pub fn dock_key(mut self, key: &'static str) -> Self {
        debug_assert!(
            detach::DIALOG_KEYS.contains(&key),
            "dialog key `{key}` is missing from detach::DIALOG_KEYS"
        );
        self.dock_key = Some(key);
        self
    }

    /// Steht fest unten rechts im Bereich `area` (nicht verschiebbar) und bleibt darin; höher
    /// als `area` wird er nie (zu hoher Inhalt rollt). So liegt er neben mittigen Dialogen statt
    /// unter ihnen.
    pub fn bottom_right_in(mut self, area: egui::Rect) -> Self {
        self.dock = Some(area);
        self
    }

    /// Steht beim Öffnen immer mittig im Fenster, auch wenn er vorher woanders stand oder das
    /// Fenster seither größer oder kleiner geworden ist (danach verschiebbar wie sonst).
    pub fn center_on_open(mut self) -> Self {
        self.center_on_open = true;
        self
    }

    /// Eigene Id (wenn der Titel wechselt oder nicht eindeutig ist).
    pub fn id(mut self, id: egui::Id) -> Self {
        self.id = id;
        self
    }

    /// Zeigt das Schließen-Symbol; ein Klick setzt `*open = false`. Bei `false` erscheint der
    /// Dialog nicht.
    pub fn open(mut self, open: &'a mut bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Für Dialoge ohne Schließen-Symbol (feste Rückfragen): Will das Betriebssystem das eigene
    /// Fenster des Dialogs schließen (Alt+F4 auf einem Unterdialog über einem abgedockten Feld),
    /// wird `*requested = true`; der Aufrufer behandelt das wie „Abbrechen“. Im Hauptfenster
    /// ändert sich nichts (kein Symbol).
    pub fn close_request(mut self, requested: &'a mut bool) -> Self {
        self.close_request = Some(requested);
        self
    }

    /// Fest in der Mitte verankert, nicht verschiebbar (Rückfragen).
    pub fn fixed(mut self) -> Self {
        self.fixed = true;
        self
    }

    /// Zeigt den Dialog; `None`, wenn er nicht (mehr) offen ist. `add_contents` kann in einem
    /// Bild mehrmals laufen (abgedockt wiederholt egui verworfene Durchgänge des eigenen
    /// Fensters); geliefert wird das Ergebnis des letzten Laufs. Abgedockt beschreibt die
    /// Antwort die Fläche des Inhalts im eigenen Fenster (dessen Punkte).
    pub fn show<R>(
        self,
        ctx: &egui::Context,
        add_contents: impl FnMut(&mut egui::Ui) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        self.show_inner(ctx, add_contents, None::<fn(&mut egui::Ui)>)
    }

    /// Wie [`Dialog::show`], dazu eine Fußzeile `footer` unter dem rollenden Inhalt (etwa
    /// „Übernehmen“ und „Abbrechen“): sie bleibt immer zu sehen, nur der Inhalt darüber rollt.
    pub fn show_with_footer<R>(
        self,
        ctx: &egui::Context,
        add_contents: impl FnMut(&mut egui::Ui) -> R,
        footer: impl FnMut(&mut egui::Ui),
    ) -> Option<egui::InnerResponse<Option<R>>> {
        self.show_inner(ctx, add_contents, Some(footer))
    }

    fn show_inner<R>(
        self,
        ctx: &egui::Context,
        mut add_contents: impl FnMut(&mut egui::Ui) -> R,
        footer: Option<impl FnMut(&mut egui::Ui)>,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        let Dialog {
            title,
            id,
            size,
            open,
            close_request,
            fixed,
            center_on_open,
            dock,
            dock_key,
        } = self;
        if matches!(open.as_deref(), Some(false)) {
            return None;
        }
        // Aus einem abgedockten Feld geöffnet (Rückfragen und Umbenennen des Baums, Rückfragen
        // der Versionen): eigenes kleines Fenster mittig über dem Feldfenster, nicht im (zu
        // schmalen) Feldfenster; ohne „Abdocken“, nicht gemerkt; feste Rückfragen ebenso.
        if let (Some(host), Some(host_rect)) =
            (detach::host_place(ctx), detach::host_viewport_rect(ctx))
        {
            let width = match size {
                DialogSize::File => DialogSize::Wide.width(),
                other => other.width(),
            };
            let max_height = ctx
                .input(|i| i.viewport().monitor_size)
                .filter(|m| m.y > DETACHED_MONITOR_MARGIN)
                .map_or(host_rect.height(), |m| m.y - DETACHED_MONITOR_MARGIN);
            return show_own_window(
                ctx,
                OwnDialog {
                    kind: OwnKind::Sub { host, host_rect },
                    title: &title,
                    id,
                    width,
                    max_height,
                },
                (open, close_request),
                add_contents,
                footer,
            );
        }
        let content = ctx.content_rect();
        // Abdockbar: mit Schlüssel, nicht fest, nicht unten rechts, mit eigenen Fenstern und
        // abgelegtem Zustand. Sonst bleibt alles wie bisher (auch in allen übrigen Tests).
        let place = dock_key
            .filter(|_| dock.is_none() && !fixed)
            .map(Place::Dialog)
            .filter(|_| detach::can_detach(ctx) && detach::is_installed(ctx));
        let memo_id = id.with("dialog_abgedockt");
        if let Some(place) = place {
            let frame = ctx.cumulative_frame_nr();
            let detached = match ctx.data(|d| d.get_temp::<DetachMemo>(memo_id)) {
                // Im vorigen (oder diesem) Bild gezeigt: wie dort.
                Some(m) if m.frame + 1 >= frame => m.detached,
                // Frisch geöffnet: abgedockt, wenn gemerkt und sein Monitor da ist; fehlt der
                // Monitor, steht er angedockt und der Eintrag bleibt.
                _ => detach::with_state(ctx, |s| {
                    detach::should_detach(s.get(place), detach::on_screen).is_some()
                })
                .unwrap_or(false),
            };
            ctx.data_mut(|d| d.insert_temp(memo_id, DetachMemo { frame, detached }));
            if detached {
                let (width, max_height) = limits(content, size);
                return show_own_window(
                    ctx,
                    OwnDialog {
                        kind: OwnKind::Detached { place, memo_id },
                        title: &title,
                        id,
                        width,
                        max_height,
                    },
                    (open, close_request),
                    add_contents,
                    footer,
                );
            }
        }
        let dark = ctx.global_style().visuals.dark_mode;
        let (width, height) = match dock {
            Some(area) => (size.width().min(area.width()), area.height()),
            None => limits(content, size),
        };
        let frame = theme::panel_frame(dark);
        // Platz über und unter dem Inhalt (Innenrand und Strich).
        let frame_h = frame.total_margin().sum().y;
        // egui merkt sich die Größe eines Fensters über das Schließen hinaus. Öffnet der Dialog
        // wieder (im vorigen Bild nicht gezeigt), stünde er im ersten Bild mit der alten Höhe
        // da, ein fester Dialog also nicht mittig. Dieser Durchgang misst nur und wird verworfen.
        let layer = egui::LayerId::new(egui::Order::Foreground, id);
        let reopened =
            ctx.memory(|m| m.area_rect(id).is_some() && !m.areas().visible_last_frame(&layer));
        if reopened {
            ctx.request_discard("Dialog: opened again");
        }
        // Größe vor diesem Durchgang: egui stellt das Fenster mit der Größe des vorigen Bilds
        // hin. Ändert sich die Höhe des Inhalts, stünde der Dialog ein Bild lang versetzt (ein
        // fester ragte beim Wachsen sogar aus dem Fenster).
        let before = ctx.memory(|m| m.area_rect(id)).map(|r| r.size());
        // Höhe: so hoch wie der Inhalt. Die Fläche für den Inhalt ist stets die größte Höhe
        // (min = max), gezeichnet wird aber nur, was der Inhalt belegt. Erst wenn er nicht
        // passt, scrollt er (unter der Kopfzeile, die stehen bleibt).
        let mut window = egui::Window::new(title.as_str())
            .id(id)
            .title_bar(false)
            .frame(frame)
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .fade_in(false)
            .fade_out(false)
            .min_width(width)
            .max_width(width)
            .min_height(height)
            .max_height(height)
            .constrain_to(dock.unwrap_or(content));
        window = if let Some(area) = dock {
            window
                .movable(false)
                .pivot(Align2::RIGHT_BOTTOM)
                .current_pos(area.right_bottom())
        } else if fixed {
            window.anchor(Align2::CENTER_CENTER, [0.0, 0.0])
        } else if center_on_open && reopened {
            // Wieder geöffnet: nicht an der alten Stelle (aus einer anderen Fenstergröße).
            window
                .pivot(Align2::CENTER_CENTER)
                .current_pos(content.center())
        } else {
            window
                .pivot(Align2::CENTER_CENTER)
                .default_pos(content.center())
        };
        let closed = window.show(ctx, |ui| {
            // egui zeigt ein neues Fenster im ersten Durchgang unsichtbar, um seine Größe zu
            // bestimmen. Dieser Durchgang wird verworfen und sofort wiederholt, damit der Dialog
            // schon im ersten Bild sichtbar und an seiner Stelle steht.
            if ui.is_sizing_pass() {
                ui.ctx().request_discard("Dialog: first sizing pass");
            }
            let top = ui.min_rect().top();
            let clicks = header(
                ui,
                &title,
                open.is_some(),
                place.map(|_| DockButton::Undock),
            );
            let used = ui.cursor().top() - top;
            // Höhe der Fußzeile aus dem vorigen Bild (ändert sie sich, wird das Bild unten
            // verworfen und wiederholt).
            let footer_id = id.with("dialog_fusszeile");
            let footer_h = match footer {
                Some(_) => ui
                    .ctx()
                    .data(|d| d.get_temp::<f32>(footer_id))
                    .unwrap_or(0.0),
                None => 0.0,
            };
            let r = egui::ScrollArea::vertical()
                .id_salt("dialog_inhalt")
                .auto_shrink([false, true])
                .max_height((height - frame_h - used - footer_h).max(0.0))
                .show(ui, &mut add_contents)
                .inner;
            if let Some(mut footer) = footer {
                // Ab der Unterkante des Inhalts: mit dem Abstand zur Fußzeile.
                let start = ui.min_rect().bottom();
                footer(ui);
                let h = ui.min_rect().bottom() - start;
                if (h - footer_h).abs() > 0.5 {
                    ui.ctx().data_mut(|d| d.insert_temp(footer_id, h));
                    ui.ctx().request_discard("Dialog: footer measured");
                }
            }
            (clicks, r)
        });
        // Größe geändert: dieses Bild verwerfen und mit der neuen Größe wiederholen (egui
        // begrenzt das auf `max_passes`, eine Schleife ist ausgeschlossen).
        if let (Some(before), Some(response)) = (before, closed.as_ref())
            && !reopened
            && (response.response.rect.size() - before).abs().max_elem() > 0.5
        {
            ctx.request_discard("Dialog: height changed");
        }
        // „Abdocken“: an der jetzigen Stelle (Bildschirmkoordinaten, physische Pixel) als eigenes
        // Fenster, ab dem nächsten Bild.
        if let (Some(place), Some(response)) = (place, closed.as_ref())
            && response
                .inner
                .as_ref()
                .is_some_and(|(clicks, _)| clicks.dock)
        {
            let rect = response.response.rect;
            let origin = ctx
                .input(|i| i.viewport().inner_rect)
                .map_or(egui::Vec2::ZERO, |r| r.min.to_vec2());
            let ppp = ctx.pixels_per_point();
            detach::with_state(ctx, |s| {
                s.set(place, detach::to_physical(rect.translate(origin), ppp));
            });
            let frame = ctx.cumulative_frame_nr();
            ctx.data_mut(|d| {
                d.insert_temp(
                    memo_id,
                    DetachMemo {
                        frame,
                        detached: true,
                    },
                );
                d.insert_temp(id.with("dialog_abgedockt_hoehe"), rect.height());
            });
            ctx.request_repaint();
        }
        closed.map(|response| {
            let inner = response.inner.map(|(clicks, r)| {
                if clicks.close
                    && let Some(open) = open
                {
                    *open = false;
                }
                r
            });
            egui::InnerResponse::new(inner, response.response)
        })
    }
}

/// Wo ein Dialog im eigenen Fenster steht.
#[derive(Clone, Copy)]
enum OwnKind {
    /// Abgedockt an der gemerkten Lage seines Platzes ([`detach::show_window`]).
    Detached { place: Place, memo_id: egui::Id },
    /// Unterdialog eines abgedockten Felds: mittig über dessen Fenster `host_rect` (Punkte),
    /// nicht gemerkt ([`detach::show_sub_window`]).
    Sub { host: Place, host_rect: egui::Rect },
}

/// Angaben eines Dialogs im eigenen Fenster für [`show_own_window`].
struct OwnDialog<'t> {
    kind: OwnKind,
    title: &'t str,
    id: egui::Id,
    /// Außenbreite wie angedockt (Punkte).
    width: f32,
    /// Größte Höhe, wenn der Monitor des Fensters unbekannt ist.
    max_height: f32,
}

/// Erste Höhe eines Unterdialogs, bevor sein Inhalt gemessen ist (Punkte).
const SUB_FIRST_HEIGHT: f32 = 160.0;

/// Zeigt den Dialog als eigenes Fenster: abgedockt ([`detach::show_window`], Kopfzeile mit
/// „Andocken“) oder als Unterdialog über einem abgedockten Feld ([`detach::show_sub_window`],
/// ohne „Andocken“); dazu (bei `open`) das Schließen-Symbol, darunter der rollende Inhalt und
/// die Fußzeile wie angedockt. Die Fensterhöhe folgt dem Inhalt (gemessen im Fenster, angewandt
/// im nächsten Bild), höchstens bis zur Monitorhöhe abzüglich [`DETACHED_MONITOR_MARGIN`].
fn show_own_window<R>(
    ctx: &egui::Context,
    d: OwnDialog,
    (open, close_request): (Option<&mut bool>, Option<&mut bool>),
    mut add_contents: impl FnMut(&mut egui::Ui) -> R,
    mut footer: Option<impl FnMut(&mut egui::Ui)>,
) -> Option<egui::InnerResponse<Option<R>>> {
    let (height_id, footer_id) = match d.kind {
        OwnKind::Detached { .. } => (
            d.id.with("dialog_abgedockt_hoehe"),
            d.id.with("dialog_abgedockt_fusszeile"),
        ),
        OwnKind::Sub { .. } => (
            d.id.with("dialog_unter_hoehe"),
            d.id.with("dialog_unter_fusszeile"),
        ),
    };
    let ppp = ctx.pixels_per_point();
    let height = ctx
        .data(|m| m.get_temp::<f32>(height_id))
        .or_else(|| match d.kind {
            OwnKind::Detached { place, .. } => detach::with_state(ctx, |s| s.get(place))
                .flatten()
                .map(|w| detach::to_points(w, ppp).height()),
            OwnKind::Sub { .. } => Some(SUB_FIRST_HEIGHT.min(d.max_height)),
        })
        .unwrap_or(d.max_height)
        .max(1.0);
    let max_height = d.max_height;
    let mut wanted = None;
    let mut body = |ui: &mut egui::Ui| {
        let child = ui.ctx().clone();
        let frame_h = {
            let dark = ui.visuals().dark_mode;
            theme::panel_frame(dark).total_margin().sum().y
        };
        // Gemessen am Cursor: die Fläche des Felds (`min_rect`) reicht immer bis zum
        // Fensterrand. Oben beginnt sie an der Kopfzeile.
        let top = ui.max_rect().top();
        let spacing = ui.spacing().item_spacing.y;
        let footer_h = match footer {
            Some(_) => child.data(|m| m.get_temp::<f32>(footer_id)).unwrap_or(0.0),
            None => 0.0,
        };
        let max_h = (ui.available_height() - footer_h).max(0.0);
        let out = egui::ScrollArea::vertical()
            .id_salt("dialog_inhalt")
            .auto_shrink([false, true])
            .max_height(max_h)
            .show(ui, &mut add_contents);
        if let Some(footer) = footer.as_mut() {
            let start = ui.cursor().top();
            footer(ui);
            let h = ui.cursor().top() - start;
            if (h - footer_h).abs() > 0.5 {
                child.data_mut(|m| m.insert_temp(footer_id, h));
                child.request_discard("Dialog: footer measured");
            }
        }
        // Höhe, die das Fenster ohne Rollen bräuchte: alles Gezeichnete (ohne den Abstand
        // nach dem letzten Element), mit dem Inhalt in voller statt in sichtbarer Höhe.
        let used = ui.cursor().top() - spacing - top;
        let content_h = out.content_size.y;
        let needed = used - content_h.min(max_h) + content_h + frame_h;
        let limit = child
            .input(|i| i.viewport().monitor_size)
            .filter(|m| m.y > DETACHED_MONITOR_MARGIN)
            .map_or(max_height, |m| m.y - DETACHED_MONITOR_MARGIN);
        wanted = Some(needed.min(limit).ceil());
        let area = ui.interact(
            ui.min_rect(),
            ui.id().with("dialog_abgedockt_flaeche"),
            egui::Sense::hover(),
        );
        (out.inner, area)
    };
    let size = egui::vec2(d.width, height);
    let shown = match d.kind {
        OwnKind::Detached { place, .. } => {
            let spec = WindowSpec {
                place,
                title: d.title,
                inner_size: size,
                resizable: false,
                focus_on_open: true,
                closable: open.is_some(),
                head: detach::Head::Title,
            };
            detach::with_state(ctx, |state| {
                detach::show_window(ctx, state, spec, &mut body)
            })
        }
        OwnKind::Sub { host, host_rect } => Some(detach::show_sub_window(
            ctx,
            detach::SubWindow {
                host,
                dialog: d.id,
                title: d.title,
                inner_size: size,
                position: host_rect.center() - size / 2.0,
                closable: open.is_some(),
            },
            &mut body,
        )),
    };
    if let Some(h) = wanted
        && (h - height).abs() > 0.5
    {
        // Neue Höhe ab dem nächsten Bild (der Builder schickt sie als `InnerSize`).
        ctx.data_mut(|m| m.insert_temp(height_id, h));
        ctx.request_repaint();
    }
    let Some((Some((r, area)), event)) = shown else {
        return None;
    };
    match (event, d.kind) {
        (WindowEvent::None, _) | (WindowEvent::DockClicked, OwnKind::Sub { .. }) => {}
        (WindowEvent::DockClicked, OwnKind::Detached { place, memo_id }) => {
            detach::with_state(ctx, |s| s.undock(place));
            let frame = ctx.cumulative_frame_nr();
            ctx.data_mut(|m| {
                m.insert_temp(
                    memo_id,
                    DetachMemo {
                        frame,
                        detached: false,
                    },
                );
            });
            ctx.request_repaint();
        }
        (WindowEvent::CloseRequested, _) => {
            if let Some(open) = open {
                *open = false;
            } else if let Some(requested) = close_request {
                *requested = true;
            }
        }
    }
    Some(egui::InnerResponse::new(Some(r), area))
}

/// Kopfzeile: Titel links, rechts (falls `closable`) das Schließen-Symbol und links daneben
/// (falls `dock`) das Symbol „Andocken“/„Abdocken“; liefert, was angeklickt wurde. Darunter
/// `SPACE_2` Abstand, kein Trennstrich.
pub(crate) fn header(
    ui: &mut egui::Ui,
    title: &str,
    closable: bool,
    dock: Option<DockButton>,
) -> HeaderClicks {
    let dark = ui.visuals().dark_mode;
    let mut clicks = HeaderClicks::default();
    ui.horizontal(|ui| {
        // Feste Zeilenhöhe: egui gibt der Reihe im ersten, messenden Durchgang sonst nur die
        // Texthöhe; der Dialog stünde dann im ersten Bild um diese Differenz versetzt.
        ui.set_min_height(ui.spacing().interact_size.y);
        if closable || dock.is_some() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                theme::frameless(ui.style_mut());
                if closable {
                    clicks.close = header_icon(ui, Icon::Close, close_label());
                }
                if let Some(dock) = dock {
                    clicks.dock = header_icon(ui, dock.icon(), dock.label());
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    title_label(ui, title, dark)
                });
            });
        } else {
            title_label(ui, title, dark);
        }
    });
    ui.add_space(SPACE_2);
    clicks
}

/// Symbol „Abdocken“/„Andocken“ eines Felds: rechts oben in `area`, senkrecht mittig im
/// Streifen der Höhe `band` (dem Kopf des Felds), über dem Inhalt; `true`, wenn angeklickt.
/// Nimmt im Feld keinen Platz ein (eigene Fläche).
pub(crate) fn corner_dock_icon(
    ui: &mut egui::Ui,
    area: egui::Rect,
    band: f32,
    button: DockButton,
) -> bool {
    let strip = egui::Rect::from_min_size(area.min, egui::vec2(area.width(), band));
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(strip)
            .layout(Layout::right_to_left(Align::Center)),
    );
    theme::frameless(child.style_mut());
    header_icon(&mut child, button.icon(), button.label())
}

/// Breite, die [`corner_dock_icon`] rechts im Kopf eines Felds belegt (mit Abstand).
pub(crate) fn corner_dock_width(ui: &egui::Ui) -> f32 {
    HEADER_ICON_SIZE + 2.0 * ui.spacing().button_padding.x + ui.spacing().item_spacing.x
}

/// Ein Symbolknopf der Kopfzeile mit Tooltip `label`; `true`, wenn angeklickt.
fn header_icon(ui: &mut egui::Ui, icon: Icon, label: String) -> bool {
    let response = ui
        .add(egui::Button::new(icons::text(
            icon,
            HEADER_ICON_SIZE,
            egui::Color32::PLACEHOLDER,
        )))
        .on_hover_text(&label);
    // Der Knopf trägt nur ein Symbol aus der Symbolschrift; sein eigener Name wäre ein
    // unlesbares Zeichen. Diese zweite Angabe ersetzt ihn durch den Namen (etwa „Schließen“)
    // für Bildschirmleser.
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
    theme::focus_ring(ui, &response);
    response.clicked()
}

fn title_label(ui: &mut egui::Ui, title: &str, dark: bool) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(title)
                .font(theme::heading_font())
                .color(theme::palette(dark).text_primary),
        )
        .truncate()
        // Nicht markierbar: Ziehen am Titel verschiebt den Dialog, statt Text zu markieren.
        .selectable(false),
    );
}
