//! Entscheidung, ob ein Platz abgedockt angezeigt wird.

use super::*;

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
pub(super) struct FieldMemo {
    frame: u64,
    saved: Option<DetachedWindow>,
    detached: bool,
}

pub(super) fn field_memo_id(p: Place) -> egui::Id {
    egui::Id::unique(("detach_feld_entschieden", p.key()))
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
