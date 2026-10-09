use crate::state::AppState;
use planora_kernel::boolean::BoolOp;

/// Offener Dialog „Boolesche Operation“: Körper A, Körper B und die Operation.
#[derive(Clone, Debug, PartialEq)]
pub struct BooleanDialog {
    pub a: usize,
    pub b: usize,
    pub op: BoolOp,
}

impl BooleanDialog {
    /// Voreinstellung: A ist der gewählte Körper (sonst der erste), B der erste andere Körper.
    pub fn new(state: &AppState) -> Self {
        Self::with_op(state, BoolOp::Difference)
    }

    /// Wie `new`, aber mit vorgewählter Operation (für die Einträge im Menü „Bearbeiten“).
    pub fn with_op(state: &AppState, op: BoolOp) -> Self {
        let a = state
            .selected_body
            .filter(|&i| i < state.bodies.len())
            .unwrap_or(0);
        let b = (0..state.bodies.len()).find(|&i| i != a).unwrap_or(0);
        Self { a, b, op }
    }

    /// Zeigt den Dialog; `true`, solange er offen bleiben soll.
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) -> bool {
        let mut open = true;
        let (mut run, mut cancel) = (false, false);
        egui::Window::new("Boolesche Operation")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                let names: Vec<String> = state.bodies.iter().map(|b| b.name.clone()).collect();
                for (label, slot) in [("Körper A", &mut self.a), ("Körper B", &mut self.b)] {
                    egui::ComboBox::from_label(label)
                        .selected_text(names.get(*slot).cloned().unwrap_or_default())
                        .show_ui(ui, |ui| {
                            for (i, n) in names.iter().enumerate() {
                                ui.selectable_value(slot, i, n);
                            }
                        });
                }
                ui.add_space(4.0);
                ui.radio_value(&mut self.op, BoolOp::Union, "Vereinigen");
                ui.radio_value(&mut self.op, BoolOp::Difference, "Abziehen (A − B)");
                ui.radio_value(&mut self.op, BoolOp::Intersection, "Schneiden");
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("Ausführen").clicked() {
                        run = true;
                    }
                    if ui.button("Abbrechen").clicked() {
                        cancel = true;
                    }
                });
            });
        if run {
            state.message = None;
            if let Err(e) = state.boolean(self.a, self.b, self.op) {
                state.message = Some(e);
            }
            return false;
        }
        open && !cancel
    }
}
