//! Tastenereignisse der abgedockten Fenster für die App.

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

pub(super) fn child_keys_id() -> egui::Id {
    egui::Id::unique("detach_kind_tasten")
}

/// Nimmt die unverbrauchten gedrückten Tasten aus der Eingabe des Fensters, in dem gerade
/// gezeichnet wird, und legt sie für [`take_child_keys`] ab. Ereignisse kommen nur im ersten
/// Durchgang eines Bilds; herausgenommen zählt jedes höchstens einmal, auch wenn der Inhalt in
/// einem Bild mehrmals läuft.
pub(super) fn collect_keys(ctx: &egui::Context, dialog: bool) {
    // Eingabe im Kindfenster: Das Hauptfenster hat seine 3D-Ansicht in diesem Bild schon
    // gezeichnet (Dialoge laufen danach). Ein weiteres Bild gibt den geänderten Wert (Vorschau,
    // Griffe, Hervorhebung) ohne Mausbewegung im Hauptfenster weiter; mehrfaches Anfordern ist
    // unschädlich.
    if ctx.input(|i| !i.events.is_empty()) {
        ctx.request_repaint_of(egui::ViewportId::ROOT);
    }
    // Strg+C/X/V sind für egui `Copy`/`Cut`/`Paste`: zu Tasten machen (Skizzen 11b), außer ein
    // Feld im Fenster hat die Tastatur; der Riegel für Strg+V liegt je Fenster.
    crate::keymap::clipboard_keys::convert_input(
        ctx,
        crate::viewport::typing(ctx),
        crate::keymap::clipboard_keys::sketch_open(ctx),
    );
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

pub(super) fn child_touched_id() -> egui::Id {
    egui::Id::unique("detach_kind_beruehrt")
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
