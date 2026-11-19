//! Prüf-Plug-in für die Laufzeit: je ein Befehl für einen Grenzfall (Endlosschleife, Speicherhunger,
//! Absturz, Fehlermeldung, Netz, Rechte, langsamer Dialog, leere Umgebung).

wit_bindgen::generate!({
    path: "../../crates/planora-plugin-api/wit",
    world: "plugin",
    generate_all,
});

use planora::plugin::types::{Command, Dialog, Level, Right};
use planora::plugin::{net, rights, ui};

struct Faelle;

const BEFEHLE: [&str; 9] = [
    "endlos",
    "speicher",
    "panik",
    "fehler",
    "netz",
    "recht",
    "langsam-dialog",
    "umgebung",
    "schlafen",
];

impl Guest for Faelle {
    fn init(_info: HostInfo) -> Result<Registration, String> {
        Ok(Registration {
            commands: BEFEHLE
                .iter()
                .map(|id| Command {
                    id: (*id).into(),
                    label_key: format!("{id}-befehl"),
                    icon: None,
                    shortcut: None,
                })
                .collect(),
        })
    }

    fn run_command(id: String) -> Result<(), String> {
        match id.as_str() {
            "endlos" => {
                // Absichtlich ohne Ende: die Laufzeit muss den Aufruf abbrechen.
                let mut n: u64 = 0;
                loop {
                    n = std::hint::black_box(n.wrapping_add(1));
                }
            }
            "speicher" => {
                // Wächst je 1 MB, bis die Speichergrenze zuschlägt.
                let mut blöcke: Vec<Vec<u8>> = Vec::new();
                loop {
                    blöcke.push(vec![1u8; 1 << 20]);
                    std::hint::black_box(&blöcke);
                }
            }
            "schlafen" => {
                // Wartet praktisch ewig: die Wartezeit muss an der Frist scheitern, nicht den Aufruf blockieren.
                std::thread::sleep(std::time::Duration::from_secs(3600));
                Ok(())
            }
            "panik" => panic!("absichtlicher Absturz"),
            "fehler" => Err("absichtlich".to_string()),
            "netz" => net::get("https://example.org/").map(|_| ()),
            "recht" => {
                rights::request(Right::Net);
                Ok(())
            }
            "langsam-dialog" => ui::open_dialog(&Dialog {
                id: "langsam".into(),
                title_key: "langsam-titel".into(),
                fields: vec![],
            }),
            "umgebung" => {
                let variablen = std::env::vars().count();
                let ordner = match std::fs::read_dir("/") {
                    Ok(_) => "ok",
                    Err(_) => "Fehler",
                };
                let zeit = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(u64::MAX);
                ui::message(
                    Level::Info,
                    &format!("{variablen} Variablen, Ordner: {ordner}, Zeit: {zeit}"),
                );
                Ok(())
            }
            _ => Err(format!("unbekannter Befehl: {id}")),
        }
    }

    fn handle_dialog(id: String, _event: DialogEvent) -> Result<(), String> {
        if id == "langsam" {
            // Absichtlich ohne Ende: ein Oberflächenaufruf hat nur 200 ms.
            let mut n: u64 = 0;
            loop {
                n = std::hint::black_box(n.wrapping_add(1));
            }
        }
        Ok(())
    }

    fn on_result(_id: u64, _result: RequestResult) {}

    fn shutdown() {}
}

export!(Faelle);
