use crate::dialog::{Dialog, DialogSize};
use crate::msg::Msg;
use crate::state::AppState;
use planora_i18n::tr;
use planora_kernel::boolean::BoolOp;

/// Offener Dialog „Boolesche Operation“: Körper A, Körper B und die Operation.
#[derive(Clone, Debug, PartialEq)]
pub struct BooleanDialog {
    pub a: usize,
    pub b: usize,
    pub op: BoolOp,
    /// Fehlermeldung des letzten Ausführens samt der Auswahl (A, B, Operation), für die sie
    /// galt; der Dialog bleibt dann offen und zeigt sie an, solange die Auswahl gleich bleibt.
    error: Option<((usize, usize, BoolOp), Msg)>,
}

impl BooleanDialog {
    /// Voreinstellung: A ist der gewählte Körper (sonst der erste), B der erste andere Körper.
    pub fn new(state: &AppState) -> Self {
        Self::with_op(state, BoolOp::Difference)
    }

    /// Wie `new`, aber mit vorgewählter Operation (für die Einträge im Menü „Bearbeiten“).
    pub fn with_op(state: &AppState, op: BoolOp) -> Self {
        // Netz-Körper lassen sich nicht verrechnen (Spec §6): nur gewöhnliche Körper anbieten.
        let a = state
            .selected_body
            .filter(|&i| i < state.bodies.len() && !state.is_mesh(i))
            .or_else(|| (0..state.bodies.len()).find(|&i| !state.is_mesh(i)))
            .unwrap_or(0);
        let b = (0..state.bodies.len())
            .find(|&i| i != a && !state.is_mesh(i))
            .unwrap_or(0);
        Self {
            a,
            b,
            op,
            error: None,
        }
    }

    /// Meldung des letzten fehlgeschlagenen Ausführens, solange A, B und die Operation noch
    /// dieselben sind; nach einer Änderung der Auswahl ist sie hinfällig.
    pub fn error(&self) -> Option<String> {
        self.error
            .as_ref()
            .filter(|(key, _)| *key == (self.a, self.b, self.op))
            .map(|(_, m)| m.text())
    }

    /// „Ausführen“ ist nur mit zwei verschiedenen Körpern möglich.
    pub fn can_run(&self) -> bool {
        self.a != self.b
    }

    /// Führt die Operation aus. Liefert `true`, wenn der Dialog offen bleiben soll: bei einem
    /// Fehler steht die Meldung dann in `error` (und in der Statuszeile); nach Erfolg schließt er.
    pub fn execute(&mut self, state: &mut AppState) -> bool {
        state.message = None;
        if !self.can_run() {
            // Der Grund bleibt im Fenster und wird beim Zeichnen übersetzt.
            let e = crate::msg!("state-pick-two-bodies");
            state.message = Some(e.text());
            self.error = Some(((self.a, self.b, self.op), e));
            return true;
        }
        match state.add_boolean(self.a, self.b, self.op) {
            Ok(_) => {
                self.error = None;
                false
            }
            Err(e) => {
                state.message = Some(e.clone());
                self.error = Some(((self.a, self.b, self.op), Msg::Raw(e)));
                true
            }
        }
    }

    /// Zeigt den Dialog; `true`, solange er offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let mut open = true;
        let (mut run, mut cancel) = (false, false);
        Dialog::new(tr!("boolean-title"), DialogSize::Normal)
            .id(egui::Id::new("boolean_dialog"))
            .open(&mut open)
            .show(ctx, |ui| {
                let names: Vec<String> = state.bodies.iter().map(|b| b.name.clone()).collect();
                let meshes: Vec<bool> = (0..names.len()).map(|i| state.is_mesh(i)).collect();
                for (label, slot) in [
                    (tr!("boolean-body-a"), &mut self.a),
                    (tr!("boolean-body-b"), &mut self.b),
                ] {
                    egui::ComboBox::from_label(label)
                        .selected_text(names.get(*slot).cloned().unwrap_or_default())
                        .show_ui(ui, |ui| {
                            for (i, n) in names.iter().enumerate() {
                                if !meshes[i] {
                                    ui.selectable_value(slot, i, n);
                                }
                            }
                        });
                }
                ui.add_space(4.0);
                ui.radio_value(&mut self.op, BoolOp::Union, tr!("chip-union"));
                ui.radio_value(&mut self.op, BoolOp::Difference, tr!("boolean-subtract"));
                ui.radio_value(&mut self.op, BoolOp::Intersection, tr!("chip-intersect"));
                ui.add_space(4.0);
                if let Some(e) = self.error() {
                    ui.colored_label(crate::browser::error_color(ui.visuals().dark_mode), e);
                    ui.add_space(4.0);
                }
                let can_run = self.can_run();
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(can_run, egui::Button::new(tr!("boolean-run")))
                        .on_disabled_hover_text(tr!("boolean-same-body"))
                        .clicked()
                    {
                        run = true;
                    }
                    if ui.button(tr!("common-cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        if run {
            return self.execute(state);
        }
        open && !cancel
    }
}
