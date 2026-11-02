//! Maßfelder am Zeiger beim Zeichnen (wie Fusion): Die Felder der aktuellen Stufe folgen dem Zeiger,
//! nehmen keine Mausereignisse an, und das aktive hat die Tastatur, solange kein anderes Eingabefeld
//! sie hat. Bis zum ersten Zeichen zeigen sie den aktuellen Wert gerundet; Getipptes sperrt
//! ([`SketchSession::type_field`]), Tab wechselt das Feld, Eingabe setzt den Punkt, Esc bricht ab.

use crate::formula_field::FormulaField;
use crate::sketch_input::FieldKind;
use crate::sketch_state::SketchSession;
use crate::sketch_view::{format_number, pointer_radius};
use crate::state::AppState;
use crate::{theme, viewport};
use egui::{Event, ImeEvent, Key, Modifiers, Order, Pos2, Rect, pos2};
use planora_feature::SketchFeature;

/// Abstand des ersten Felds vom Zeiger, je Richtung.
const POINTER_GAP: f32 = 24.0;
/// Abstand der Felder untereinander.
const ROW_STEP: f32 = 30.0;
/// Breite des Textes im Feld.
const FIELD_WIDTH: f32 = 72.0;
/// Breite eines Felds samt Name, solange es noch nicht gemessen ist.
const ROW_WIDTH_GUESS: f32 = 200.0;

fn kind_index(kind: FieldKind) -> usize {
    kind as usize
}

fn area_id(kind: FieldKind) -> egui::Id {
    egui::Id::new("planora-draw-field-area").with(kind_index(kind))
}

/// Unter diesem Schlüssel steht die Kennung des Textfelds, sobald es einmal gezeichnet wurde.
fn widget_id_key(kind: FieldKind) -> egui::Id {
    egui::Id::new("planora-draw-field-widget").with(kind_index(kind))
}

fn widget_id(ctx: &egui::Context, kind: FieldKind) -> Option<egui::Id> {
    ctx.data(|d| d.get_temp(widget_id_key(kind)))
}

/// Linke obere Ecke des Felds `row` von `rows` für den Zeiger `anchor`: rechts unterhalb des
/// Zeigers, am Rand nach links beziehungsweise oben geklappt.
fn field_pos(rect: Rect, anchor: Pos2, width: f32, row: usize, rows: usize) -> Pos2 {
    let x = if anchor.x + POINTER_GAP + width > rect.right() {
        anchor.x - POINTER_GAP - width
    } else {
        anchor.x + POINTER_GAP
    };
    let block = rows as f32 * ROW_STEP;
    let top = if anchor.y + POINTER_GAP + block > rect.bottom() {
        anchor.y - POINTER_GAP - block
    } else {
        anchor.y + POINTER_GAP
    };
    pos2(x, top + row as f32 * ROW_STEP)
}

/// Der aktuelle Wert gerundet, wie ihn das Feld vor dem ersten Zeichen zeigt: Länge wie
/// [`planora_i18n::fmt_len`] ohne Einheit, Winkel mit einer Stelle.
fn suggestion(se: &SketchSession, sf: &SketchFeature, kind: FieldKind) -> String {
    match se.field_value(&sf.sketch, kind) {
        Some(v) if matches!(kind, FieldKind::Angle | FieldKind::Sweep) => {
            format_number(v.to_degrees(), 1)
        }
        Some(v) => planora_i18n::fmt_len_num(v),
        None => String::new(),
    }
}

/// Darf das aktive Feld die Tastatur nehmen? Beim Erscheinen der Felder, außer ein anderes
/// Eingabefeld hat sie; danach, wenn sie niemand hat. Wer später mit Absicht auf eine Fläche
/// wechselt, die nur den Fokus annimmt (Statuszeile, Zeichenfläche), behält ihn dort.
fn may_take_keyboard(ctx: &egui::Context, appeared: bool) -> bool {
    viewport::draw_field_has_keyboard(ctx)
        || ctx.memory(|m| m.focused()).is_none()
        || (appeared && !viewport::keyboard_taken(ctx))
}

/// Bringt das Ereignis Text ins Feld? Das erste ersetzt den Vorschlag. Eine Eingabehilfe (IME)
/// liefert statt `Text` ihre Vorschau und das Ergebnis.
fn starts_typing(event: &Event) -> bool {
    match event {
        Event::Text(_) | Event::Paste(_) => true,
        Event::Ime(ImeEvent::Preedit { text, .. } | ImeEvent::Commit(text)) => !text.is_empty(),
        _ => false,
    }
}

/// Zeigt die Felder der aktuellen Stufe und verarbeitet ihre Eingaben. Der Zeiger `pointer` setzt
/// die Lage; ohne ihn bleiben die Felder, wo sie zuletzt standen. `true`: Sperren oder Stufe haben
/// sich geändert, der aufgelöste Zeiger dieses Bilds ist überholt.
pub fn show(ui: &mut egui::Ui, rect: Rect, state: &mut AppState, pointer: Option<Pos2>) -> bool {
    let ctx = ui.ctx().clone();
    let radius = pointer.map_or(0.0, |p| pointer_radius(state, rect, p));
    let Some((se, sk)) = state.session_sketch_mut() else {
        return false;
    };
    se.sync_input(sk);
    if se.input.fields.is_empty() {
        // Sonst sperrte das verschwundene Feld die Kürzel noch ein Bild länger.
        if viewport::draw_field_has_keyboard(&ctx) {
            ctx.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        return false;
    }
    let scope = state.document.param_scope();
    let Some((se, sf)) = state.session_draft_mut() else {
        return false;
    };

    if viewport::draw_field_has_keyboard(&ctx) {
        // Im Bild nach einer Fokusanforderung gilt die Fokussperre noch nicht: egui hat aus Tab oder
        // einer Pfeiltaste dann schon eine Richtung gemacht und gäbe den Fokus an das nächste
        // Bedienelement weiter (`consume_key` macht das nicht mehr rückgängig).
        ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
        let back = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Tab));
        if back || ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Tab)) {
            se.next_field(back);
            // Vor dem Zeichnen: Ein Zeichen im selben Bild gehört schon dem neuen Feld.
            if let Some(id) = widget_id(&ctx, se.input.fields[se.input.active].kind) {
                ctx.memory_mut(|m| m.request_focus(id));
            }
        }
    }

    let anchor_key = egui::Id::new("planora-draw-field-anchor");
    let anchor = pointer
        .or_else(|| ctx.data(|d| d.get_temp(anchor_key)))
        .unwrap_or(rect.center());
    ctx.data_mut(|d| d.insert_temp(anchor_key, anchor));

    let frame_key = egui::Id::new("planora-draw-field-frame");
    let frame = ctx.cumulative_frame_nr();
    let appeared = ctx
        .data(|d| d.get_temp::<u64>(frame_key))
        .is_none_or(|last| last + 1 < frame);
    ctx.data_mut(|d| d.insert_temp(frame_key, frame));

    let rows = se.input.fields.len();
    let (mut enter, mut esc, mut typed) = (false, false, false);
    for i in 0..rows {
        let (kind, touched) = (se.input.fields[i].kind, se.input.fields[i].touched);
        let error = se.input.fields[i].error.clone();
        let has_keys = widget_id(&ctx, kind).is_some_and(|id| ctx.memory(|m| m.has_focus(id)));
        let mut text = if touched {
            se.input.fields[i].text.clone()
        } else {
            suggestion(se, sf, kind)
        };
        // Das erste Zeichen ersetzt den Vorschlag, die Rücktaste davor leert das Feld.
        let (replace, wipe) = ctx.input(|input| {
            (
                input.events.iter().any(starts_typing),
                input.key_pressed(Key::Backspace) || input.key_pressed(Key::Delete),
            )
        });
        let wipe = wipe && has_keys && !touched;
        if wipe || (replace && has_keys && !touched) {
            text.clear();
        }
        let width = ctx
            .memory(|m| m.area_rect(area_id(kind)))
            .map_or(ROW_WIDTH_GUESS, |r| r.width());
        let response = egui::Area::new(area_id(kind))
            .fixed_pos(field_pos(rect, anchor, width, i, rows))
            .order(Order::Foreground)
            .interactable(false)
            .show(&ctx, |ui| {
                let p = theme::palette(ui.visuals().dark_mode);
                egui::Frame::new()
                    .fill(p.bg_panel)
                    .corner_radius(3.0)
                    .inner_margin(1.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(kind.label());
                            let outcome = FormulaField::new(
                                ("planora-draw-field", kind_index(kind)),
                                &mut text,
                                kind.unit(),
                            )
                            .lock_focus(true)
                            .error_below(false)
                            .width(FIELD_WIDTH)
                            .label(kind.label())
                            .show(ui, &scope);
                            if let Some(e) = &error {
                                ui.colored_label(p.error, e);
                            }
                            outcome.response
                        })
                        .inner
                    })
                    .inner
            })
            .inner;
        viewport::set_draw_field(&ctx, response.id);
        ctx.data_mut(|d| d.insert_temp(widget_id_key(kind), response.id));
        if response.changed() || wipe {
            se.type_field(i, &text, &scope);
            typed = true;
        }
        if response.lost_focus() {
            enter |= ctx.input(|input| input.key_pressed(Key::Enter));
            esc |= ctx.input(|input| input.key_pressed(Key::Escape));
        }
    }

    if enter {
        let raw = se.draw_cursor.as_ref().map(|c| c.raw).or(se.cursor);
        if let Some(raw) = raw {
            // Fehler stehen in der Meldung der Sitzung und im Feld.
            let _ = se.commit_input(sf, raw, radius, false, &scope);
        }
    }
    if esc {
        se.cancel(&mut sf.sketch);
        return true;
    }
    se.sync_input(&sf.sketch);
    // Nach dem Bearbeiten: Eingabe und ein Klick neben das Feld nehmen ihm den Fokus. Das nächste
    // Bild soll trotzdem wieder hier tippen; ein anderes Eingabefeld behält seinen. Nur wenn er
    // fehlt: Eine neue Anforderung löschte die Fokussperre, und Tab spränge aus dem Feld.
    if may_take_keyboard(&ctx, appeared)
        && let Some(field) = se.input.fields.get(se.input.active)
        && let Some(id) = widget_id(&ctx, field.kind)
        && !ctx.memory(|m| m.has_focus(id))
    {
        ctx.memory_mut(|m| m.request_focus(id));
    }
    typed || enter
}
