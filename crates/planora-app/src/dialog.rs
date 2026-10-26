//! Dialog-Rahmen im Graphit-Stil: schwebendes Fenster im Rahmen der Felder ([`theme::panel_frame`]),
//! drei feste Breiten, mittig im Fenster, eigene Kopfzeile mit Titel und Schließen-Symbol. Der
//! Dialog ist so hoch wie sein Inhalt; nur zu hoher Inhalt scrollt (unter der stehenden
//! Kopfzeile). Der Dialog bleibt ganz im Fenster.

use crate::icons::{self, Icon};
use crate::shell::SHELL_INSET;
use crate::theme::{self, SPACE_2};
use egui::{Align, Align2, Layout};

/// Name des Schließen-Symbols (Tooltip und Bedienhilfe).
pub const CLOSE_LABEL: &str = "Schließen";

/// Größe des Schließen-Symbols in der Kopfzeile.
const CLOSE_ICON_SIZE: f32 = 16.0;

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
    fixed: bool,
    center_on_open: bool,
    /// Fest unten rechts in diesem Bereich statt mittig ([`Dialog::bottom_right_in`]).
    dock: Option<egui::Rect>,
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
            fixed: false,
            center_on_open: false,
            dock: None,
        }
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

    /// Fest in der Mitte verankert, nicht verschiebbar (Rückfragen).
    pub fn fixed(mut self) -> Self {
        self.fixed = true;
        self
    }

    /// Zeigt den Dialog; `None`, wenn er nicht (mehr) offen ist.
    pub fn show<R>(
        self,
        ctx: &egui::Context,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        self.show_inner(ctx, add_contents, None::<fn(&mut egui::Ui)>)
    }

    /// Wie [`Dialog::show`], dazu eine Fußzeile `footer` unter dem rollenden Inhalt (etwa
    /// „Übernehmen“ und „Abbrechen“): sie bleibt immer zu sehen, nur der Inhalt darüber rollt.
    pub fn show_with_footer<R>(
        self,
        ctx: &egui::Context,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
        footer: impl FnOnce(&mut egui::Ui),
    ) -> Option<egui::InnerResponse<Option<R>>> {
        self.show_inner(ctx, add_contents, Some(footer))
    }

    fn show_inner<R>(
        self,
        ctx: &egui::Context,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
        footer: Option<impl FnOnce(&mut egui::Ui)>,
    ) -> Option<egui::InnerResponse<Option<R>>> {
        let Dialog {
            title,
            id,
            size,
            open,
            fixed,
            center_on_open,
            dock,
        } = self;
        if matches!(open.as_deref(), Some(false)) {
            return None;
        }
        let dark = ctx.global_style().visuals.dark_mode;
        let content = ctx.content_rect();
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
            ctx.request_discard("Dialog: wieder geöffnet");
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
                ui.ctx().request_discard("Dialog: erste Größenbestimmung");
            }
            let top = ui.min_rect().top();
            let close = header(ui, &title, open.is_some());
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
                .show(ui, add_contents)
                .inner;
            if let Some(footer) = footer {
                // Ab der Unterkante des Inhalts: mit dem Abstand zur Fußzeile.
                let start = ui.min_rect().bottom();
                footer(ui);
                let h = ui.min_rect().bottom() - start;
                if (h - footer_h).abs() > 0.5 {
                    ui.ctx().data_mut(|d| d.insert_temp(footer_id, h));
                    ui.ctx().request_discard("Dialog: Fußzeile vermessen");
                }
            }
            (close, r)
        });
        // Größe geändert: dieses Bild verwerfen und mit der neuen Größe wiederholen (egui
        // begrenzt das auf `max_passes`, eine Schleife ist ausgeschlossen).
        if let (Some(before), Some(response)) = (before, closed.as_ref())
            && !reopened
            && (response.response.rect.size() - before).abs().max_elem() > 0.5
        {
            ctx.request_discard("Dialog: Höhe geändert");
        }
        closed.map(|response| {
            let inner = response.inner.map(|(close, r)| {
                if close && let Some(open) = open {
                    *open = false;
                }
                r
            });
            egui::InnerResponse::new(inner, response.response)
        })
    }
}

/// Kopfzeile: Titel links, rechts (falls `closable`) das Schließen-Symbol; liefert, ob es
/// angeklickt wurde. Darunter `SPACE_2` Abstand, kein Trennstrich.
fn header(ui: &mut egui::Ui, title: &str, closable: bool) -> bool {
    let dark = ui.visuals().dark_mode;
    let mut close = false;
    ui.horizontal(|ui| {
        // Feste Zeilenhöhe: egui gibt der Reihe im ersten, messenden Durchgang sonst nur die
        // Texthöhe; der Dialog stünde dann im ersten Bild um diese Differenz versetzt.
        ui.set_min_height(ui.spacing().interact_size.y);
        if closable {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                theme::frameless(ui.style_mut());
                let response = ui
                    .add(egui::Button::new(icons::text(
                        Icon::Close,
                        CLOSE_ICON_SIZE,
                        egui::Color32::PLACEHOLDER,
                    )))
                    .on_hover_text(CLOSE_LABEL);
                // Der Knopf trägt nur ein Symbol aus der Symbolschrift; sein eigener Name wäre ein
                // unlesbares Zeichen. Diese zweite Angabe ersetzt ihn durch „Schließen“ für
                // Bildschirmleser.
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, CLOSE_LABEL)
                });
                theme::focus_ring(ui, &response);
                close = response.clicked();
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    title_label(ui, title, dark)
                });
            });
        } else {
            title_label(ui, title, dark);
        }
    });
    ui.add_space(SPACE_2);
    close
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
