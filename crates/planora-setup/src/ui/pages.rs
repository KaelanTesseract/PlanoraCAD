//! Rechtes Feld: Inhalt der Seiten und die Knöpfe unten rechts. Die Seiten ändern nur die
//! Auswahl im [`Wizard`]; alles mit Wirkung nach außen (Motor starten, Ordnerdialog, App
//! starten, Fenster schließen) melden sie als [`Action`] an `ui::SetupApp`.

use super::assets::{Assets, Picture};
use super::steps::step_name;
use super::style::{self, RADIUS_MD, SPACE_1, SPACE_2, SPACE_3, SPACE_4};
use crate::progress::{Progress, Step};
use crate::system::default_dir;
use crate::wizard::{Hint, Page, Wizard};
use egui::{Align, Layout, Rect, RichText, Sense, Stroke, StrokeKind, UiBuilder, pos2, vec2};
use planora_i18n::tr;
use planora_style::{Palette, fonts};

/// Lizenztext (GPL-3.0) aus dem Wurzelordner des Repos.
const LICENSE: &str = include_str!("../../../../LICENSE");
/// Phosphor „warning“, wie `Icon::Warning` der App.
const ICON_WARNING: char = '\u{e4e0}';
/// Phosphor „check-circle“, wie `Icon::EndSketch` der App.
const ICON_CHECK_CIRCLE: char = '\u{e184}';
/// Höhe der Knopfzeile unten.
const BUTTONS_H: f32 = 30.0;
/// Höhe der beiden Karten auf der Ordner-Seite (gleich hoch).
const MODE_CARD_H: f32 = 92.0;
/// Kantenlänge des Logos neben dem Schriftzug.
const LOGO_SIZE: f32 = 26.0;
/// Rundung der Bilder.
const SHOT_RADIUS: u8 = 7;

/// Was ein Klick auslöst.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Fenster schließen, ohne etwas zu tun.
    Cancel,
    Back,
    Next,
    /// „Installieren“ auf der Ordner-Seite.
    Install,
    /// „Erneut prüfen“, ob PlanoraCAD noch läuft.
    Recheck,
    /// Ordnerdialog öffnen.
    Browse,
    OpenLog,
    /// Fertig-Seite: ggf. App starten, dann schließen.
    Finish,
    /// Fehlerseite: aufräumen und schließen.
    Close,
}

/// Ein Knopf der Knopfzeile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonSpec {
    pub action: Action,
    /// Hauptknopf (Fläche `accent`).
    pub primary: bool,
    pub enabled: bool,
}

/// Knöpfe der Seite von links nach rechts; der Hauptknopf steht ganz rechts.
pub fn buttons(wizard: &Wizard, log_exists: bool) -> Vec<ButtonSpec> {
    let b = |action, primary, enabled| ButtonSpec {
        action,
        primary,
        enabled,
    };
    match &wizard.page {
        Page::Welcome => vec![b(Action::Cancel, false, true), b(Action::Next, true, true)],
        Page::License => vec![
            b(Action::Cancel, false, true),
            b(Action::Back, false, true),
            b(Action::Next, true, wizard.can_next()),
        ],
        // „Installieren“ bleibt drückbar: Ein ungültiger Ordner zeigt dann den Hinweis.
        Page::Folder => vec![
            b(Action::Cancel, false, true),
            b(Action::Back, false, true),
            b(Action::Install, true, true),
        ],
        // Abbrechen ist während der Installation gesperrt.
        Page::Installing => vec![b(Action::Cancel, false, false)],
        Page::Done => vec![b(Action::Finish, true, true)],
        Page::Failed { .. } => vec![
            b(Action::OpenLog, false, log_exists),
            b(Action::Close, true, true),
        ],
    }
}

/// Beschriftung eines Knopfs.
pub fn label(action: Action) -> String {
    match action {
        Action::Cancel => tr!("setup-cancel"),
        Action::Back => tr!("setup-back"),
        Action::Next => tr!("setup-next"),
        Action::Install => tr!("setup-install"),
        Action::Recheck => tr!("setup-hint-recheck"),
        Action::Browse => tr!("setup-folder-browse"),
        Action::OpenLog => tr!("setup-open-log"),
        Action::Finish => tr!("setup-finish"),
        Action::Close => tr!("setup-close"),
    }
}

/// Der Schritt des Motors in Worten.
pub fn step_text(step: Step) -> String {
    match step {
        Step::Prepare => tr!("setup-progress-prepare"),
        Step::Install => tr!("setup-progress-install"),
        Step::PostInstall => tr!("setup-progress-postinstall"),
        Step::Done => tr!("setup-progress-done"),
    }
}

/// Zeile unter dem Balken: Schritt in Worten und Prozent.
pub fn progress_line(p: Progress) -> String {
    tr!(
        "setup-progress-line",
        step = step_text(p.step),
        percent = u32::from(p.percent)
    )
}

/// Willkommenssatz: Neuinstallation oder Update auf `version`.
pub fn welcome_text(wizard: &Wizard, version: &str) -> String {
    if wizard.existing.is_some() {
        tr!("setup-welcome-update", version = version)
    } else {
        tr!("setup-welcome-text", version = version)
    }
}

/// Ausschnitt (UV) eines Bilds mit Seitenverhältnis `image`, das eine Fläche mit Seitenverhältnis
/// `target` ganz füllt (wie CSS `background-size: cover`, mittig beschnitten).
pub fn cover_uv(target: f32, image: f32) -> Rect {
    if !(target > 0.0 && image > 0.0) {
        return Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    }
    if target > image {
        // Fläche breiter als das Bild: oben und unten abschneiden.
        let h = image / target;
        Rect::from_min_max(pos2(0.0, (1.0 - h) / 2.0), pos2(1.0, (1.0 + h) / 2.0))
    } else {
        let w = target / image;
        Rect::from_min_max(pos2((1.0 - w) / 2.0, 0.0), pos2((1.0 + w) / 2.0, 1.0))
    }
}

/// Breite und Abstand der drei gleich hohen Karten auf der Installieren-Seite.
pub fn teaser_columns(width: f32, gap: f32) -> f32 {
    ((width - 2.0 * gap) / 3.0).max(0.0)
}

/// Bildhöhe der Karten auf der Installieren-Seite: Das Bild füllt die Fläche der Höhe `area`
/// bis auf die Unterschrift (`caption`) und den Abstand darüber (`gap`), wie `flex: 1` in
/// Entwurf C; das Bild wird mittig beschnitten (`cover_uv`).
pub fn teaser_shot_height(area: f32, caption: f32, gap: f32) -> f32 {
    (area - caption - gap).max(0.0)
}

/// Daten, die die Seiten zum Zeichnen brauchen, aber nicht ändern.
pub struct View<'a> {
    pub p: &'a Palette,
    pub assets: &'a Assets,
    pub progress: Progress,
    pub version: &'a str,
    pub log_exists: bool,
}

/// Zeichnet den Inhalt des rechten Felds in `rect`; liefert die ausgelöste Aktion.
pub fn show(ui: &mut egui::Ui, rect: Rect, wizard: &mut Wizard, view: &View) -> Option<Action> {
    let content = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - BUTTONS_H));
    let bar = Rect::from_min_max(pos2(rect.left(), rect.bottom() - BUTTONS_H), rect.max);
    let mut action = None;
    {
        let mut ui = ui.new_child(
            UiBuilder::new()
                .max_rect(content)
                .layout(Layout::top_down(Align::Min)),
        );
        ui.set_clip_rect(content.expand(1.0).intersect(ui.clip_rect()));
        let page = wizard.page.clone();
        let a = match page {
            Page::Welcome => {
                welcome(&mut ui, wizard, view);
                None
            }
            Page::License => {
                license(&mut ui, wizard, view);
                None
            }
            Page::Folder => folder(&mut ui, wizard, view),
            Page::Installing => {
                installing(&mut ui, view);
                None
            }
            Page::Done => {
                done(&mut ui, wizard, view);
                None
            }
            Page::Failed { log } => {
                failed(&mut ui, &log, view);
                None
            }
        };
        action = action.or(a);
    }
    let mut ui = ui.new_child(
        UiBuilder::new()
            .max_rect(bar)
            .layout(Layout::right_to_left(Align::Center)),
    );
    for spec in buttons(wizard, view.log_exists).iter().rev() {
        let text = label(spec.action);
        let button = if spec.primary {
            style::primary_button(text, view.p)
        } else {
            egui::Button::new(text)
        };
        if ui.add_enabled(spec.enabled, button).clicked() {
            action = Some(spec.action);
        }
    }
    action
}

/// Seitentitel (19 px halbfett).
fn title(ui: &mut egui::Ui, text: String, p: &Palette) {
    ui.label(
        RichText::new(text)
            .font(style::title_font())
            .color(p.text_primary),
    );
    ui.add_space(SPACE_1 + 2.0);
}

/// Fließtext in `text_secondary`.
fn body(ui: &mut egui::Ui, text: String, p: &Palette) {
    ui.add(
        egui::Label::new(
            RichText::new(text)
                .font(style::body_font())
                .color(p.text_secondary),
        )
        .wrap(),
    );
}

/// Bild füllt `rect` (mittig beschnitten), mit Rundung und 1-px-Rand.
fn picture(ui: &egui::Ui, rect: Rect, pic: Option<&Picture>, p: &Palette) {
    let radius = egui::CornerRadius::same(SHOT_RADIUS);
    match pic {
        Some(pic) => {
            egui::Image::from_texture(&pic.texture)
                .uv(cover_uv(rect.width() / rect.height().max(1.0), pic.aspect))
                .corner_radius(radius)
                .paint_at(ui, rect);
        }
        None => {
            ui.painter().rect_filled(rect, radius, p.bg_inset);
        }
    }
    ui.painter().rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, p.border_subtle),
        StrokeKind::Inside,
    );
}

fn welcome(ui: &mut egui::Ui, wizard: &Wizard, view: &View) {
    let p = view.p;
    ui.horizontal(|ui| {
        if let Some(logo) = &view.assets.logo {
            ui.add(
                egui::Image::from_texture(&logo.texture)
                    .fit_to_exact_size(vec2(LOGO_SIZE, LOGO_SIZE)),
            );
        }
        ui.label(
            RichText::new("PlanoraCAD")
                .font(style::brand_font())
                .color(p.text_primary),
        );
    });
    ui.add_space(SPACE_3);
    title(ui, step_name(&Page::Welcome), p);
    body(ui, welcome_text(wizard, view.version), p);
    ui.add_space(SPACE_4);
    // Der Screenshot füllt den Rest bis kurz über die Knöpfe.
    let avail = ui.available_rect_before_wrap();
    let rect = Rect::from_min_max(avail.min, pos2(avail.right(), avail.bottom() - SPACE_4));
    if rect.height() > 40.0 {
        ui.allocate_rect(rect, Sense::hover());
        picture(ui, rect, view.assets.welcome.as_ref(), p);
    }
}

fn license(ui: &mut egui::Ui, wizard: &mut Wizard, view: &View) {
    let p = view.p;
    title(ui, step_name(&Page::License), p);
    ui.add_space(SPACE_2);
    let avail = ui.available_rect_before_wrap();
    // Unten Platz für den Haken und den Abstand zu den Knöpfen.
    let field = Rect::from_min_max(avail.min, pos2(avail.right(), avail.bottom() - 52.0));
    ui.allocate_rect(field, Sense::hover());
    ui.painter().rect(
        field,
        RADIUS_MD,
        p.bg_inset,
        Stroke::new(1.0, p.border_subtle),
        StrokeKind::Inside,
    );
    let inner = field.shrink2(vec2(SPACE_3, SPACE_2));
    let mut text_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(inner)
            .layout(Layout::top_down(Align::Min)),
    );
    text_ui.set_clip_rect(inner.intersect(ui.clip_rect()));
    style::scroll_area(
        &mut text_ui,
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(inner.height()),
        |ui| {
            ui.add(
                egui::Label::new(
                    RichText::new(LICENSE)
                        .font(egui::FontId::new(12.0, egui::FontFamily::Proportional))
                        .color(p.text_secondary),
                )
                .wrap(),
            );
        },
    );
    ui.add_space(SPACE_3);
    ui.checkbox(&mut wizard.accepted, tr!("setup-license-accept"));
}

fn folder(ui: &mut egui::Ui, wizard: &mut Wizard, view: &View) -> Option<Action> {
    let p = view.p;
    let mut action = None;
    title(ui, step_name(&Page::Folder), p);
    ui.add_space(SPACE_2);

    // Zwei gleich hohe Karten nebeneinander.
    let width = ui.available_width();
    let card_w = ((width - SPACE_2) / 2.0).max(0.0);
    let top = ui.cursor().min;
    let cards = [
        (false, tr!("setup-mode-user"), tr!("setup-mode-user-hint")),
        (true, tr!("setup-mode-all"), tr!("setup-mode-all-hint")),
    ];
    for (i, (all_users, name, hint)) in cards.into_iter().enumerate() {
        let rect = Rect::from_min_size(
            pos2(top.x + i as f32 * (card_w + SPACE_2), top.y),
            vec2(card_w, MODE_CARD_H),
        );
        if mode_card(ui, rect, wizard, all_users, (name, hint), p) {
            wizard.set_all_users(all_users);
        }
    }
    ui.allocate_rect(
        Rect::from_min_size(top, vec2(width, MODE_CARD_H)),
        Sense::hover(),
    );
    ui.add_space(SPACE_4);

    // Ordnerfeld mit „Durchsuchen“.
    ui.horizontal(|ui| {
        let browse_w = 110.0;
        let mut dir = wizard.dir.clone();
        let edit = egui::TextEdit::singleline(&mut dir)
            .desired_width(ui.available_width() - browse_w - SPACE_2)
            .margin(vec2(SPACE_2, 6.0));
        if ui.add_enabled(!wizard.locked, edit).changed() {
            wizard.set_dir(dir);
        }
        let can_browse = !wizard.locked && cfg!(windows);
        if ui
            .add_enabled(
                can_browse,
                egui::Button::new(label(Action::Browse)).min_size(vec2(browse_w, 0.0)),
            )
            .clicked()
        {
            action = Some(Action::Browse);
        }
    });
    ui.add_space(SPACE_3);
    ui.checkbox(&mut wizard.desktop_icon, tr!("setup-desktop-icon"));
    ui.add_space(SPACE_3);

    // Hinweiszeile.
    let hint = match wizard.hint {
        Hint::None => None,
        Hint::AppRunning => Some((tr!("setup-hint-running"), p.warning, true)),
        Hint::UacDeclined => Some((tr!("setup-hint-uac"), p.warning, false)),
        Hint::InvalidFolder => Some((tr!("setup-hint-folder"), p.error, false)),
    };
    if let Some((text, color, recheck)) = hint {
        ui.horizontal_wrapped(|ui| {
            ui.label(
                RichText::new(ICON_WARNING.to_string())
                    .font(egui::FontId::new(16.0, fonts::icons()))
                    .color(color),
            );
            ui.label(RichText::new(text).font(style::body_font()).color(color));
        });
        if recheck {
            ui.add_space(SPACE_2);
            if ui.button(label(Action::Recheck)).clicked() {
                action = Some(Action::Recheck);
            }
        }
    }
    action
}

/// Eine Modus-Karte; `true`, wenn sie angeklickt wurde (gesperrt: nie).
fn mode_card(
    ui: &mut egui::Ui,
    rect: Rect,
    wizard: &Wizard,
    all_users: bool,
    (name, hint): (String, String),
    p: &Palette,
) -> bool {
    let selected = wizard.all_users == all_users;
    let sense = if wizard.locked {
        Sense::hover()
    } else {
        Sense::click()
    };
    let response = ui.interact(rect, ui.id().with(("mode-card", all_users)), sense);
    let stroke = if selected {
        Stroke::new(1.0, p.accent)
    } else if response.hovered() && !wizard.locked {
        Stroke::new(1.0, p.text_tertiary)
    } else {
        Stroke::new(1.0, p.border_subtle)
    };
    let painter = ui.painter();
    painter.rect(
        rect,
        RADIUS_MD,
        style::card_fill(p, selected),
        stroke,
        StrokeKind::Inside,
    );
    // Gesperrt (Update am selben Ort): die nicht gewählte Karte blasser.
    let dim = |c: egui::Color32| {
        if wizard.locked && !selected {
            c.gamma_multiply(0.5)
        } else {
            c
        }
    };
    let x = rect.left() + SPACE_3;
    let mut y = rect.top() + SPACE_3;
    let name_galley = painter.layout_no_wrap(name, style::label_font(), dim(p.text_primary));
    let name_h = name_galley.size().y;
    painter.galley(pos2(x, y), name_galley, p.text_primary);
    y += name_h + SPACE_1;
    let hint_galley = painter.layout(
        hint,
        style::small_font(),
        dim(p.text_secondary),
        rect.width() - 2.0 * SPACE_3,
    );
    let hint_h = hint_galley.size().y;
    painter.galley(pos2(x, y), hint_galley, p.text_secondary);
    y += hint_h + SPACE_2;
    let path = default_dir(all_users).to_string_lossy().into_owned();
    let path_galley = painter.layout(
        path,
        style::small_font(),
        dim(p.text_tertiary),
        rect.width() - 2.0 * SPACE_3,
    );
    painter
        .with_clip_rect(rect.shrink(1.0))
        .galley(pos2(x, y), path_galley, p.text_tertiary);
    response.clicked() && !wizard.locked
}

fn installing(ui: &mut egui::Ui, view: &View) {
    let p = view.p;
    title(ui, tr!("setup-installing"), p);
    ui.add_space(SPACE_1);
    // Balken in Akzentblau.
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    ui.painter().rect_filled(bar, 3.0, p.bg_field);
    let fraction = f32::from(view.progress.percent.min(100)) / 100.0;
    if fraction > 0.0 {
        let filled = Rect::from_min_size(bar.min, vec2(bar.width() * fraction, bar.height()));
        ui.painter().rect_filled(filled, 3.0, p.accent);
    }
    ui.add_space(SPACE_2);
    ui.label(
        RichText::new(progress_line(view.progress))
            .font(style::small_font())
            .color(p.text_tertiary),
    );
    ui.add_space(SPACE_4);

    // Drei gleich hohe Karten mit Bild und Unterschrift.
    let avail = ui.available_rect_before_wrap();
    let area = Rect::from_min_max(avail.min, pos2(avail.right(), avail.bottom() - SPACE_3));
    if area.height() < 40.0 {
        return;
    }
    ui.allocate_rect(area, Sense::hover());
    let gap = SPACE_2;
    let col = teaser_columns(area.width(), gap);
    let caption_h = 18.0;
    let captions = [
        tr!("setup-teaser-sketch"),
        tr!("setup-teaser-drawing"),
        tr!("setup-teaser-section"),
    ];
    // Die Karten wachsen bis zum unteren Rand (Entwurf C), das Bild wird mittig beschnitten;
    // alle drei Karten sind gleich hoch, die Unterschrift steht darunter.
    let shot_h = teaser_shot_height(area.height(), caption_h, SPACE_1);
    for (i, caption) in captions.into_iter().enumerate() {
        let x = area.left() + i as f32 * (col + gap);
        let shot = Rect::from_min_size(pos2(x, area.top()), vec2(col, shot_h));
        picture(ui, shot, view.assets.teasers[i].as_ref(), p);
        ui.painter().text(
            pos2(x, shot.bottom() + SPACE_1 + 1.0),
            egui::Align2::LEFT_TOP,
            caption,
            style::small_font(),
            p.text_tertiary,
        );
    }
}

fn done(ui: &mut egui::Ui, wizard: &mut Wizard, view: &View) {
    let p = view.p;
    title(ui, step_name(&Page::Done), p);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(ICON_CHECK_CIRCLE.to_string())
                .font(egui::FontId::new(18.0, fonts::icons()))
                .color(p.ok),
        );
        ui.label(
            RichText::new(tr!("setup-done-text"))
                .font(style::body_font())
                .color(p.text_secondary),
        );
    });
    ui.add_space(SPACE_4);
    ui.checkbox(&mut wizard.start_app, tr!("setup-start-app"));
}

fn failed(ui: &mut egui::Ui, log: &std::path::Path, view: &View) {
    let p = view.p;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(ICON_WARNING.to_string())
                .font(egui::FontId::new(20.0, fonts::icons()))
                .color(p.error),
        );
        ui.label(
            RichText::new(tr!("setup-failed-title"))
                .font(style::title_font())
                .color(p.text_primary),
        );
    });
    ui.add_space(SPACE_1 + 2.0);
    body(ui, tr!("setup-failed-text"), p);
    if view.log_exists {
        ui.add_space(SPACE_2);
        ui.label(
            RichText::new(log.display().to_string())
                .font(style::small_font())
                .color(p.text_tertiary),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::existing::Existing;
    use planora_i18n::{english, german, with_lang};
    use std::path::PathBuf;

    fn at(page: Page) -> Wizard {
        let mut w = Wizard::new(None);
        w.page = page;
        w
    }

    fn actions(w: &Wizard, log: bool) -> Vec<(Action, bool, bool)> {
        buttons(w, log)
            .into_iter()
            .map(|b| (b.action, b.primary, b.enabled))
            .collect()
    }

    #[test]
    fn buttons_per_page() {
        use Action::*;
        assert_eq!(
            actions(&at(Page::Welcome), false),
            [(Cancel, false, true), (Next, true, true)]
        );
        let mut license = at(Page::License);
        assert_eq!(actions(&license, false)[2], (Next, true, false));
        license.accepted = true;
        assert_eq!(actions(&license, false)[2], (Next, true, true));
        assert_eq!(actions(&at(Page::Folder), false)[2], (Install, true, true));
        assert_eq!(
            actions(&at(Page::Installing), false),
            [(Cancel, false, false)]
        );
        assert_eq!(actions(&at(Page::Done), false), [(Finish, true, true)]);
        let failed = at(Page::Failed {
            log: PathBuf::from("x"),
        });
        assert_eq!(
            actions(&failed, false),
            [(OpenLog, false, false), (Close, true, true)]
        );
        assert_eq!(actions(&failed, true)[0], (OpenLog, false, true));
        // Genau ein Hauptknopf, außer während der Installation.
        for page in [Page::Welcome, Page::License, Page::Folder, Page::Done] {
            let primaries = buttons(&at(page), true)
                .iter()
                .filter(|b| b.primary)
                .count();
            assert_eq!(primaries, 1);
        }
    }

    #[test]
    fn progress_line_in_words() {
        let p = Progress {
            percent: 62,
            step: Step::Install,
        };
        with_lang(&german(), || {
            assert_eq!(progress_line(p), "Dateien werden kopiert \u{b7} 62 %");
        });
        with_lang(&english(), || {
            assert_eq!(progress_line(p), "Copying files \u{b7} 62%");
            assert_eq!(step_text(Step::Done), "Finished");
        });
    }

    #[test]
    fn welcome_text_mentions_update_for_existing_install() {
        let fresh = Wizard::new(None);
        let update = Wizard::new(Some(Existing {
            all_users: false,
            dir: PathBuf::from(r"C:\X"),
        }));
        with_lang(&english(), || {
            assert_eq!(
                welcome_text(&fresh, "1.2.3"),
                "This program installs PlanoraCAD 1.2.3 on your computer."
            );
            assert!(welcome_text(&update, "1.2.3").contains("updates PlanoraCAD"));
        });
    }

    #[test]
    fn cover_uv_crops_centered() {
        let full = cover_uv(16.0 / 9.0, 16.0 / 9.0);
        assert!((full.width() - 1.0).abs() < 1e-6 && (full.height() - 1.0).abs() < 1e-6);
        // Hohe, schmale Fläche: links und rechts abschneiden.
        let narrow = cover_uv(1.0, 2.0);
        assert!((narrow.min.x - 0.25).abs() < 1e-6 && (narrow.max.x - 0.75).abs() < 1e-6);
        assert_eq!((narrow.min.y, narrow.max.y), (0.0, 1.0));
        // Breite Fläche: oben und unten abschneiden.
        let wide = cover_uv(4.0, 2.0);
        assert!((wide.min.y - 0.25).abs() < 1e-6 && (wide.max.y - 0.75).abs() < 1e-6);
        assert_eq!(
            cover_uv(0.0, 1.0),
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
        );
    }

    #[test]
    fn three_teaser_columns_fill_width() {
        let col = teaser_columns(616.0, 8.0);
        assert!((3.0 * col + 2.0 * 8.0 - 616.0).abs() < 1e-3);
        assert_eq!(teaser_columns(4.0, 8.0), 0.0);
    }

    #[test]
    fn teaser_shots_fill_remaining_height() {
        // Bild füllt die Fläche bis auf Unterschrift und Abstand.
        assert_eq!(teaser_shot_height(300.0, 18.0, 4.0), 278.0);
        assert_eq!(teaser_shot_height(10.0, 18.0, 4.0), 0.0);
    }
}
