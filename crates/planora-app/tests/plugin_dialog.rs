//! Beschriebene Dialoge der Plug-ins (Plug-ins Etappe 1, Task 6): Das Plug-in beschreibt Titel
//! und Felder, das Programm zeichnet sie und meldet `changed`, `ok` und `cancel`. Ohne Fenster:
//! Rohereignisse je Bild gegen einen egui-Kontext; die Ende-zu-Ende-Fälle mit den Test-Paketen
//! aus `planora-plugin-host` in eigenen Temp-Fundorten.

mod common;

use common::{Seen, collect_seen, find, join};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, vec2};
use planora_app::plugin_dialog::{self, OpenPluginDialog};
use planora_app::plugins::{self, Plugins};
use planora_app::workspace::Workspace;
use planora_app::{fonts, theme};
use planora_plugin_host::EntryState;
use planora_plugin_host::package::Package;
use planora_plugin_host::places::{self as plugin_places, Places};
use planora_plugin_host::runtime::{Dialog, DialogEvent, Field, FieldKind, FieldValue};
use planora_project::config::Config;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const HALLO: &str = "planora.beispiel-hallo";
const FAELLE: &str = "planora.test-faelle";

// ---- Der Dialog allein ----------------------------------------------------------------------

fn feld(id: &str, label_key: &str, kind: FieldKind) -> Field {
    Field {
        id: id.into(),
        label_key: label_key.into(),
        kind,
    }
}

/// Ein Dialog mit allen vier Feldarten.
fn beschreibung() -> Dialog {
    Dialog {
        id: "probe".into(),
        title_key: "probe-titel".into(),
        fields: vec![
            feld("hinweis", "probe-hinweis", FieldKind::Label),
            feld("name", "probe-name", FieldKind::Text(String::new())),
            feld("anzahl", "probe-anzahl", FieldKind::Number(2.5)),
            feld("laut", "probe-laut", FieldKind::Toggle(false)),
        ],
    }
}

/// Die Texte des Probe-Plug-ins: deutsch oder englisch nach der Sprache des Programms.
fn probe_texte(key: &str) -> String {
    let en = planora_i18n::current().id() == "en";
    match (key, en) {
        ("probe-titel", false) => "Probedialog",
        ("probe-titel", true) => "Probe dialog",
        ("probe-hinweis", _) => "Nur ein Hinweis",
        ("probe-name", _) => "Name",
        ("probe-anzahl", false) => "Anzahl",
        ("probe-anzahl", true) => "Count",
        ("probe-laut", false) => "Laut",
        ("probe-laut", true) => "Loud",
        _ => key,
    }
    .to_string()
}

fn kontext() -> egui::Context {
    let ctx = egui::Context::default();
    ctx.set_fonts(fonts::definitions());
    theme::apply(&ctx, true);
    ctx
}

/// Ein Bild nur mit dem Dialog `d`; liefert sein Ereignis und die Texte.
fn dialog_bild(
    ctx: &egui::Context,
    d: &mut OpenPluginDialog,
    events: Vec<Event>,
) -> (Option<DialogEvent>, Vec<Seen>) {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
        events,
        ..Default::default()
    };
    let mut got = None;
    let out = ctx.run_ui(raw, |ui| {
        got = plugin_dialog::show(ui.ctx(), d, &probe_texte)
    });
    let seen = collect_seen(&out.shapes);
    out.drop_without_applying_deltas();
    (got, seen)
}

fn druck(at: Pos2) -> [Vec<Event>; 3] {
    let b = |pressed| Event::PointerButton {
        pos: at,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    [vec![Event::PointerMoved(at)], vec![b(true)], vec![b(false)]]
}

/// Klick an `at` (drei Bilder); gesammelt werden alle Ereignisse des Dialogs.
fn dialog_klick(ctx: &egui::Context, d: &mut OpenPluginDialog, at: Pos2) -> Vec<DialogEvent> {
    druck(at)
        .into_iter()
        .filter_map(|e| dialog_bild(ctx, d, e).0)
        .collect()
}

/// Mitte des Eingabeelements in der Zeile mit der Beschriftung `zeile`: rechte Spalte, gleich
/// hinter der breitesten Beschriftung `labels`; `x` Punkte in die Spalte hinein.
fn spalte(seen: &[Seen], labels: &[&str], zeile: &str, x: f32) -> Pos2 {
    let rechts = labels
        .iter()
        .map(|l| find(seen, l).unwrap_or_else(|| panic!("„{l}“ fehlt: {}", join(seen))))
        .map(|s| s.rect.max.x)
        .fold(f32::MIN, f32::max);
    let y = find(seen, zeile).unwrap().rect.center().y;
    Pos2::new(rechts + 12.0 + x, y)
}

fn taste(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn fields_render_and_ok() {
    planora_i18n::with_lang(&planora_i18n::german(), || {
        let ctx = kontext();
        let mut d = OpenPluginDialog::new("planora.probe", beschreibung());
        let (ev, seen) = dialog_bild(&ctx, &mut d, vec![]);
        assert_eq!(ev, None);
        for t in [
            "Probedialog",
            "Nur ein Hinweis",
            "Name",
            "Anzahl",
            "Laut",
            "2,5",
            "OK",
            "Abbrechen",
        ] {
            assert!(find(&seen, t).is_some(), "„{t}“ fehlt: {}", join(&seen));
        }
        let labels = ["Nur ein Hinweis", "Name", "Anzahl", "Laut"];

        // Text: anklicken und tippen meldet `changed`.
        let mut got = dialog_klick(&ctx, &mut d, spalte(&seen, &labels, "Name", 40.0));
        got.extend(dialog_bild(&ctx, &mut d, vec![Event::Text("Dennis".into())]).0);
        assert_eq!(
            got,
            vec![DialogEvent::Changed((
                "name".into(),
                FieldValue::Text("Dennis".into())
            ))]
        );

        // Zahl: alles markieren, mit Komma neu tippen.
        let mut got = dialog_klick(&ctx, &mut d, spalte(&seen, &labels, "Anzahl", 40.0));
        dialog_bild(&ctx, &mut d, vec![taste(Key::A, Modifiers::COMMAND)]);
        got.extend(dialog_bild(&ctx, &mut d, vec![Event::Text("3,75".into())]).0);
        assert_eq!(
            got,
            vec![DialogEvent::Changed((
                "anzahl".into(),
                FieldValue::Number(3.75)
            ))]
        );
        // Ungültig: kein `changed`, der letzte gültige Wert bleibt.
        let (ev, _) = dialog_bild(&ctx, &mut d, vec![Event::Text("x".into())]);
        assert_eq!(ev, None);
        dialog_bild(&ctx, &mut d, vec![taste(Key::Backspace, Modifiers::NONE)]);

        // Haken: anklicken meldet `changed`.
        let got = dialog_klick(&ctx, &mut d, spalte(&seen, &labels, "Laut", 7.0));
        assert_eq!(
            got,
            vec![DialogEvent::Changed((
                "laut".into(),
                FieldValue::Toggle(true)
            ))]
        );

        // OK: alle Werte in der Reihenfolge der Felder.
        let (_, seen) = dialog_bild(&ctx, &mut d, vec![]);
        let ok = find(&seen, "OK").unwrap().rect.center();
        let got = dialog_klick(&ctx, &mut d, ok);
        assert_eq!(
            got,
            vec![DialogEvent::Ok(vec![
                ("name".into(), FieldValue::Text("Dennis".into())),
                ("anzahl".into(), FieldValue::Number(3.75)),
                ("laut".into(), FieldValue::Toggle(true)),
            ])]
        );
    });
}

#[test]
fn cancel_on_escape() {
    planora_i18n::with_lang(&planora_i18n::german(), || {
        let ctx = kontext();
        let mut d = OpenPluginDialog::new("planora.probe", beschreibung());
        let (_, seen) = dialog_bild(&ctx, &mut d, vec![]);
        let (ev, _) = dialog_bild(&ctx, &mut d, vec![taste(Key::Escape, Modifiers::NONE)]);
        assert_eq!(ev, Some(DialogEvent::Cancel), "Esc");
        // „Abbrechen“ ebenso.
        let at = find(&seen, "Abbrechen").unwrap().rect.center();
        assert_eq!(dialog_klick(&ctx, &mut d, at), vec![DialogEvent::Cancel]);
    });
}

#[test]
fn reopen_keeps_values() {
    let mut d = OpenPluginDialog::new("planora.probe", beschreibung());
    d.values
        .insert("name".into(), FieldValue::Text("Dennis".into()));
    d.values.insert("laut".into(), FieldValue::Toggle(true));
    let mut neu = beschreibung();
    // „laut“ ist jetzt eine Zahl, „anzahl“ fehlt, „neu“ kommt dazu.
    neu.fields = vec![
        feld("name", "probe-name", FieldKind::Text("leer".into())),
        feld("laut", "probe-laut", FieldKind::Number(1.0)),
        feld("neu", "probe-neu", FieldKind::Toggle(true)),
    ];
    d.reopen(neu.clone());
    assert_eq!(d.spec, neu);
    assert_eq!(
        d.all_values(),
        vec![
            ("name".into(), FieldValue::Text("Dennis".into())),
            ("laut".into(), FieldValue::Number(1.0)),
            ("neu".into(), FieldValue::Toggle(true)),
        ]
    );

    // Über die Plug-ins: `open-dialog` mit derselben id ersetzt die Felder und behält den Wert.
    let (mut ws, ctx) = arbeitsbereich("wieder", &["beispiel-hallo"]);
    ws.plugins
        .host
        .run_command("planora.beispiel-hallo", "hallo");
    bild(&mut ws, &ctx, vec![]);
    ws.plugins.dialogs[0]
        .values
        .insert("name".into(), FieldValue::Text("Dennis".into()));
    ws.plugins
        .host
        .run_command("planora.beispiel-hallo", "hallo");
    let seen = bild(&mut ws, &ctx, vec![]);
    assert_eq!(ws.plugins.dialogs.len(), 1, "ersetzt, nicht doppelt");
    assert!(find(&seen, "Dennis").is_some(), "{}", join(&seen));
}

#[test]
fn language_switch_retranslates() {
    let (mut ws, ctx) = arbeitsbereich("sprache", &["beispiel-hallo"]);
    ws.plugins
        .host
        .run_command("planora.beispiel-hallo", "hallo");
    let seen = bild(&mut ws, &ctx, vec![]);
    assert!(find(&seen, "Hallo").is_some(), "{}", join(&seen));
    assert!(find(&seen, "Laut").is_some(), "{}", join(&seen));
    let seen = planora_i18n::with_lang(&planora_i18n::english(), || bild(&mut ws, &ctx, vec![]));
    assert!(find(&seen, "Hello").is_some(), "{}", join(&seen));
    assert!(find(&seen, "Loud").is_some(), "{}", join(&seen));
    assert!(find(&seen, "Cancel").is_some(), "{}", join(&seen));
    assert!(find(&seen, "Hallo").is_none(), "{}", join(&seen));
}

// ---- Mit den Test-Paketen -------------------------------------------------------------------

/// Fundorte in Temp mit den Paketen `pakete` (Namen der Dateien in den Testdaten des Hosts).
fn orte(name: &str, pakete: &[&str]) -> Places {
    let root = common::temp_root(&format!("planora-plugin-dialog-{name}"));
    let p = Places {
        bundled: vec![root.join("mitgeliefert")],
        user: root.join("benutzer"),
        cache: root.join("cache"),
    };
    for paket in pakete {
        let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../planora-plugin-host/tests/data")
            .join(format!("{paket}.pcplugin"));
        let pkg = Package::read(&std::fs::read(file).unwrap()).unwrap();
        plugin_places::install(&p, &pkg, None).unwrap();
    }
    p
}

/// Arbeitsbereich mit Konfiguration in Temp und gestarteten Plug-ins aus `pakete`.
fn arbeitsbereich(name: &str, pakete: &[&str]) -> (Workspace, egui::Context) {
    let mut ws = Workspace::new(None, Config::default());
    let cfg = common::temp_root(&format!("planora-plugin-dialog-cfg-{name}"));
    std::fs::create_dir_all(&cfg).unwrap();
    ws.config_path = cfg.join("config.json");
    ws.plugins = Plugins::new(orte(name, pakete), &ws.config);
    ws.plugins.host.set_language("de");
    let ctx = kontext();
    let start = Instant::now();
    while ws
        .plugins
        .host
        .entries()
        .iter()
        .any(|e| e.state == EntryState::Loading)
    {
        bild(&mut ws, &ctx, vec![]);
        assert!(start.elapsed() < Duration::from_secs(60), "lädt zu lange");
        std::thread::sleep(Duration::from_millis(10));
    }
    (ws, ctx)
}

/// Ein Bild mit `events`; zeichnet nur, was `plugins::show` zeichnet. Liefert die Texte.
fn bild(ws: &mut Workspace, ctx: &egui::Context, events: Vec<Event>) -> Vec<Seen> {
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1920.0, 1080.0))),
        events,
        ..Default::default()
    };
    let out = ctx.run_ui(raw, |ui| plugins::show(ui.ctx(), ws));
    let seen = collect_seen(&out.shapes);
    out.drop_without_applying_deltas();
    seen
}

fn klick_bei(ws: &mut Workspace, ctx: &egui::Context, at: Pos2) -> Vec<Seen> {
    let [a, b, c] = druck(at);
    bild(ws, ctx, a);
    bild(ws, ctx, b);
    bild(ws, ctx, c)
}

#[test]
fn dialogs_close_on_failure() {
    let (mut ws, ctx) = arbeitsbereich("ausfall", &["test-faelle"]);
    let edits = ws.active_tab().state.document.edit_count();
    ws.plugins
        .host
        .run_command("planora.test-faelle", "langsam-dialog");
    let seen = bild(&mut ws, &ctx, vec![]);
    assert_eq!(ws.plugins.dialogs.len(), 1);
    assert!(find(&seen, "Langsam").is_some(), "{}", join(&seen));

    // Ein Ereignis an den Dialog: `handle-dialog` läuft endlos → Zeitüberschreitung.
    ws.plugins.host.dialog_event(
        FAELLE,
        "langsam",
        DialogEvent::Changed(("x".into(), FieldValue::Toggle(true))),
    );
    let mut seen = bild(&mut ws, &ctx, vec![]);
    let failed = "Das Plug-in Test-Fälle wurde wegen eines Fehlers angehalten.";
    if find(&seen, failed).is_none() {
        seen = bild(&mut ws, &ctx, vec![]);
    }
    assert!(ws.plugins.dialogs.is_empty(), "Dialog weg");
    assert!(find(&seen, "Langsam").is_none(), "{}", join(&seen));
    assert!(
        find(&seen, failed).is_some(),
        "Fehlerleiste: {}",
        join(&seen)
    );
    assert_eq!(ws.active_tab().state.document.edit_count(), edits);
}

#[test]
fn hallo_end_to_end() {
    let (mut ws, ctx) = arbeitsbereich("hallo", &["beispiel-hallo"]);
    ws.plugins
        .host
        .run_command("planora.beispiel-hallo", "hallo");
    let seen = bild(&mut ws, &ctx, vec![]);
    assert_eq!(ws.plugins.dialogs.len(), 1);
    assert_eq!(ws.plugins.dialogs[0].plugin, HALLO);
    assert_eq!(ws.plugins.dialogs[0].spec.id, "gruss");
    for t in ["Hallo", "Name", "Laut", "OK", "Abbrechen"] {
        assert!(find(&seen, t).is_some(), "„{t}“ fehlt: {}", join(&seen));
    }
    // Name eintippen, OK.
    klick_bei(
        &mut ws,
        &ctx,
        spalte(&seen, &["Name", "Laut"], "Name", 40.0),
    );
    bild(&mut ws, &ctx, vec![Event::Text("Dennis".into())]);
    let ok = find(&seen, "OK").unwrap().rect.center();
    klick_bei(&mut ws, &ctx, ok);
    assert!(ws.plugins.dialogs.is_empty(), "nach OK zu");
    bild(&mut ws, &ctx, vec![]);
    assert_eq!(
        ws.active_tab().state.message.as_deref(),
        Some("Beispiel-Hallo: Hallo, Dennis!")
    );
}
