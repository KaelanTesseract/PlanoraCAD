//! Beispiel-Plug-in: ein Befehl „Hallo sagen“, der einen Dialog öffnet und den Gruß als Meldung zeigt.

wit_bindgen::generate!({
    path: "../../crates/planora-plugin-api/wit",
    world: "plugin",
    generate_all,
});

use planora::plugin::types::{Command, Dialog, Field, FieldKind, FieldValue, Level};
use planora::plugin::{i18n, ui};

struct Hallo;

impl Guest for Hallo {
    fn init(_info: HostInfo) -> Result<Registration, String> {
        Ok(Registration {
            commands: vec![Command {
                id: "hallo".into(),
                label_key: "hallo-befehl".into(),
                icon: None,
                shortcut: None,
            }],
        })
    }

    fn run_command(id: String) -> Result<(), String> {
        if id != "hallo" {
            return Err(format!("unbekannter Befehl: {id}"));
        }
        ui::open_dialog(&Dialog {
            id: "gruss".into(),
            title_key: "hallo-titel".into(),
            fields: vec![
                Field {
                    id: "name".into(),
                    label_key: "hallo-name".into(),
                    kind: FieldKind::Text(String::new()),
                },
                Field {
                    id: "laut".into(),
                    label_key: "hallo-laut".into(),
                    kind: FieldKind::Toggle(false),
                },
            ],
        })
    }

    fn handle_dialog(id: String, event: DialogEvent) -> Result<(), String> {
        if id != "gruss" {
            return Ok(());
        }
        if let DialogEvent::Ok(values) = event {
            let mut name = String::new();
            let mut laut = false;
            for (feld, wert) in values {
                match (feld.as_str(), wert) {
                    ("name", FieldValue::Text(t)) => name = t,
                    ("laut", FieldValue::Toggle(b)) => laut = b,
                    _ => {}
                }
            }
            let mut text = i18n::text("hallo-gruss", &[("name".to_string(), name)]);
            if laut {
                text = text.to_uppercase();
            }
            ui::message(Level::Info, &text);
        }
        Ok(())
    }

    fn on_result(_id: u64, _result: RequestResult) {}

    fn shutdown() {}
}

export!(Hallo);
