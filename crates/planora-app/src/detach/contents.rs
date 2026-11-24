//! Inhalt des Kindfensters.

use super::*;

/// Inhalt des Kindfensters (läuft mit dessen Eingabe und Faktor).
pub(super) fn window_contents<R>(
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
                        ui.scope_id().with("detach_kopfzeile"),
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
                    // des Inhalts zieht das Fenster (statt den Inhalt zu rollen). Über den Knöpfen
                    // des Kopfs bleibt er offen: egui gibt einen Klick unter einem obenliegenden
                    // Nur-Ziehen-Feld niemandem (hit_test), die Knöpfe wären sonst tot.
                    let strip = egui::Rect::from_min_size(area.min, egui::vec2(area.width(), band));
                    let base = ui.scope_id().with("detach_kopfstreifen");
                    let mut drag: Option<egui::Response> = None;
                    for (i, piece) in strip_pieces(ui, strip).into_iter().enumerate() {
                        let r = ui.interact(piece, base.with(i), egui::Sense::drag());
                        drag = Some(match drag {
                            Some(d) => d | r,
                            None => r,
                        });
                    }
                    let drag = drag.unwrap_or_else(|| {
                        ui.interact(egui::Rect::NOTHING, base, egui::Sense::hover())
                    });
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
