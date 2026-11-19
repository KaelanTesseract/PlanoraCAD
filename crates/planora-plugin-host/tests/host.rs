//! Der `PluginHost` (Plug-ins Etappe 1, Task 4): Start im Hintergrund, Befehle, Ausfälle, Neustart,
//! Rechte-Nachfrage und Netz über das Ereignis-Postfach. Die Test-Pakete werden in eigene
//! Temp-Fundorte installiert.

use planora_plugin_host::manifest::Right;
use planora_plugin_host::package::Package;
use planora_plugin_host::places::{self, Places};
use planora_plugin_host::rights::Answer;
use planora_plugin_host::runtime::{HttpResponse, RequestResult};
use planora_plugin_host::{EntryState, HostEvent, PluginHost};
use planora_project::config::PluginConfig;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const HALLO: &str = "planora.beispiel-hallo";
const FAELLE: &str = "planora.test-faelle";

fn orte(name: &str, pakete: &[&str]) -> Places {
    let root =
        std::env::temp_dir().join(format!("planora-plugin-host-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let p = Places {
        bundled: vec![root.join("mitgeliefert")],
        user: root.join("benutzer"),
        cache: root.join("cache"),
    };
    for paket in pakete {
        let file = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data")
            .join(format!("{paket}.pcplugin"));
        let pkg = Package::read(&std::fs::read(file).unwrap()).unwrap();
        places::install(&p, &pkg).unwrap();
    }
    p
}

fn aufraeumen(p: &Places) {
    let _ = std::fs::remove_dir_all(p.user.parent().unwrap());
}

fn state(h: &PluginHost, id: &str) -> EntryState {
    h.entries()
        .iter()
        .find(|e| e.id == id)
        .unwrap()
        .state
        .clone()
}

fn failures(h: &PluginHost, id: &str) -> u8 {
    h.entries().iter().find(|e| e.id == id).unwrap().failures
}

/// Ruft `poll` auf, bis `id` nicht mehr lädt (höchstens 60 s); gibt alle Ereignisse zurück.
fn warten(h: &mut PluginHost, id: &str) -> Vec<HostEvent> {
    let start = Instant::now();
    let mut events = Vec::new();
    loop {
        events.extend(h.poll());
        if state(h, id) != EntryState::Loading {
            return events;
        }
        assert!(start.elapsed() < Duration::from_secs(60), "lädt zu lange");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn gestartet(name: &str, pakete: &[&str]) -> (PluginHost, Places) {
    let p = orte(name, pakete);
    let mut h = PluginHost::new(p.clone());
    h.start_all(&BTreeMap::new());
    for id in pakete {
        let id = format!("planora.{id}");
        warten(&mut h, &id);
        assert_eq!(state(&h, &id), EntryState::Running);
    }
    (h, p)
}

fn ausfaelle(events: &[HostEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, HostEvent::Failed { .. }))
        .count()
}

#[test]
fn start_registers_commands() {
    let (h, p) = gestartet("befehle", &["beispiel-hallo"]);
    let cmds = h.commands();
    let hallo = cmds
        .iter()
        .find(|c| c.full_id == "planora.beispiel-hallo.hallo")
        .expect("Befehl fehlt");
    assert_eq!(hallo.plugin, HALLO);
    assert_eq!(hallo.label, "Say hello");
    aufraeumen(&p);
}

#[test]
fn disabled_not_started() {
    let p = orte("aus", &["beispiel-hallo"]);
    let mut h = PluginHost::new(p.clone());
    let config = BTreeMap::from([(
        HALLO.to_string(),
        PluginConfig {
            enabled: false,
            rights: BTreeMap::new(),
        },
    )]);
    h.start_all(&config);
    assert_eq!(state(&h, HALLO), EntryState::Off);
    std::thread::sleep(Duration::from_millis(300));
    assert!(h.poll().is_empty());
    assert_eq!(state(&h, HALLO), EntryState::Off);
    assert!(h.commands().is_empty());
    assert_eq!(
        h.plugin_configs().get(HALLO).map(|c| c.enabled),
        Some(false)
    );

    // Einschalten startet es und meldet die Änderung zum Speichern.
    h.set_enabled(HALLO, true);
    let events = warten(&mut h, HALLO);
    assert!(events.iter().any(|e| matches!(e, HostEvent::Changed)));
    assert_eq!(state(&h, HALLO), EntryState::Running);
    assert!(
        !h.plugin_configs().contains_key(HALLO),
        "Voreinstellung wird nicht geschrieben"
    );
    h.set_enabled(HALLO, false);
    assert_eq!(state(&h, HALLO), EntryState::Off);
    assert!(h.commands().is_empty());
    assert!(h.poll().iter().any(|e| matches!(e, HostEvent::Changed)));
    aufraeumen(&p);
}

#[test]
fn failure_counts_and_stops() {
    let (mut h, p) = gestartet("stopp", &["test-faelle"]);
    for n in 1..=3u8 {
        h.run_command("planora.test-faelle.panik");
        h.poll();
        assert_eq!(failures(&h, FAELLE), n);
        if n < 3 {
            assert!(matches!(state(&h, FAELLE), EntryState::Failed(_)));
            assert!(
                h.commands().is_empty(),
                "ein ausgefallenes Plug-in hat keine Befehle"
            );
            h.restart(FAELLE);
            warten(&mut h, FAELLE);
            assert_eq!(state(&h, FAELLE), EntryState::Running);
        }
    }
    assert_eq!(state(&h, FAELLE), EntryState::Stopped);
    h.restart(FAELLE);
    std::thread::sleep(Duration::from_millis(300));
    h.poll();
    assert_eq!(state(&h, FAELLE), EntryState::Stopped);
    h.set_enabled(FAELLE, true);
    assert_eq!(state(&h, FAELLE), EntryState::Stopped);

    // Erst ein neuer Host (Programmneustart) startet es wieder.
    let mut h = PluginHost::new(p.clone());
    h.start_all(&BTreeMap::new());
    warten(&mut h, FAELLE);
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    assert_eq!(failures(&h, FAELLE), 0);
    aufraeumen(&p);
}

#[test]
fn failure_event_once_per_failure() {
    let (mut h, p) = gestartet("ereignis", &["test-faelle"]);
    h.run_command("planora.test-faelle.panik");
    let events = h.poll();
    assert_eq!(ausfaelle(&events), 1, "{events:?}");
    let Some(HostEvent::Failed { plugin, reason }) = events
        .iter()
        .find(|e| matches!(e, HostEvent::Failed { .. }))
    else {
        unreachable!()
    };
    assert_eq!(plugin, FAELLE);
    assert!(!reason.is_empty());
    assert_eq!(ausfaelle(&h.poll()), 0);
    // Weitere Befehle an das ausgefallene Plug-in tun nichts.
    h.run_command("planora.test-faelle.panik");
    assert_eq!(ausfaelle(&h.poll()), 0);
    assert_eq!(failures(&h, FAELLE), 1);

    // Ein Fehler, den das Plug-in selbst meldet, ist kein Ausfall, sondern eine Meldung.
    h.restart(FAELLE);
    warten(&mut h, FAELLE);
    h.run_command("planora.test-faelle.fehler");
    let events = h.poll();
    assert_eq!(ausfaelle(&events), 0);
    assert!(events.iter().any(
        |e| matches!(e, HostEvent::Message { plugin, text, .. } if plugin == FAELLE && text.contains("absichtlich"))
    ));
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    aufraeumen(&p);
}

/// Die Nachfragen (`Ask`) in `events`: (Anfrage, Recht, Generation).
fn asks(events: &[HostEvent]) -> Vec<(u64, Right, u64)> {
    events
        .iter()
        .filter_map(|e| match e {
            HostEvent::Ask {
                plugin,
                request,
                right,
                generation,
            } if plugin == FAELLE => Some((*request, *right, *generation)),
            _ => None,
        })
        .collect()
}

/// Die Netzanfragen in `events`: (Anfrage, Adresse, Generation).
fn nets(events: &[HostEvent]) -> Vec<(u64, String, u64)> {
    events
        .iter()
        .filter_map(|e| match e {
            HostEvent::Net {
                plugin,
                request,
                url,
                generation,
            } if plugin == FAELLE => Some((*request, url.clone(), *generation)),
            _ => None,
        })
        .collect()
}

#[test]
fn restart_resets_instance() {
    let (mut h, p) = gestartet("neustart", &["test-faelle"]);
    h.run_command("planora.test-faelle.recht");
    let first = asks(&h.poll());
    assert_eq!(first.len(), 1);
    let (_, _, g1) = first[0];

    h.run_command("planora.test-faelle.panik");
    h.poll();
    h.restart(FAELLE);
    warten(&mut h, FAELLE);
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    assert!(
        h.commands()
            .iter()
            .any(|c| c.full_id == "planora.test-faelle.panik")
    );

    h.run_command("planora.test-faelle.recht");
    let second = asks(&h.poll());
    assert_eq!(second.len(), 1, "die neue Instanz fragt wieder");
    let (req2, _, g2) = second[0];
    assert_ne!(g1, g2, "neue Generation");

    // Eine Antwort an die alte Generation wird still verworfen.
    h.answer(FAELLE, g1, req2, Answer::Allow);
    assert!(h.poll().is_empty());
    assert!(!h.plugin_configs().contains_key(FAELLE));
    h.answer(FAELLE, g2, req2, Answer::Allow);
    assert!(h.poll().iter().any(|e| matches!(e, HostEvent::Changed)));
    assert_eq!(
        h.plugin_configs()[FAELLE]
            .rights
            .get("net")
            .map(String::as_str),
        Some("allowed")
    );
    aufraeumen(&p);
}

#[test]
fn rights_and_net_through_mailbox() {
    let (mut h, p) = gestartet("netz", &["test-faelle"]);
    // Zwei Netzanfragen, solange gefragt wird: nur eine Nachfrage.
    h.run_command("planora.test-faelle.netz");
    h.run_command("planora.test-faelle.netz");
    let events = h.poll();
    let a = asks(&events);
    assert_eq!(a.len(), 1, "{events:?}");
    assert!(nets(&events).is_empty());
    let (req, right, g) = a[0];
    assert_eq!(right, Right::Net);

    // „Nur diesmal“: beide Anfragen gehen ins Netz, nichts zu speichern.
    h.answer(FAELLE, g, req, Answer::Once);
    let events = h.poll();
    let n = nets(&events);
    assert_eq!(n.len(), 2, "{events:?}");
    assert!(
        n.iter()
            .all(|(_, url, generation)| url == "https://example.org/" && *generation == g)
    );
    assert!(!events.iter().any(|e| matches!(e, HostEvent::Changed)));

    // Ergebnisse kommen beim nächsten `poll` an; eine alte Generation wird verworfen.
    h.net_result(
        FAELLE,
        g,
        n[0].0,
        RequestResult::Http(HttpResponse {
            status: 200,
            body: b"ok".to_vec(),
        }),
    );
    h.net_result(FAELLE, g + 1000, n[1].0, RequestResult::Failed("x".into()));
    assert!(h.poll().is_empty());
    assert_eq!(state(&h, FAELLE), EntryState::Running);

    // Jetzt ist `net` erlaubt: direkt ins Netz, ohne Nachfrage.
    h.run_command("planora.test-faelle.netz");
    let events = h.poll();
    assert!(asks(&events).is_empty());
    assert_eq!(nets(&events).len(), 1);

    // Zurückgenommen: es wird wieder gefragt, und die Änderung ist zu speichern.
    h.revoke(FAELLE, Right::Net);
    h.run_command("planora.test-faelle.netz");
    let events = h.poll();
    assert!(events.iter().any(|e| matches!(e, HostEvent::Changed)));
    assert_eq!(asks(&events).len(), 1, "{events:?}");
    aufraeumen(&p);
}

#[test]
fn saved_rights_from_config() {
    let p = orte("gespeichert", &["test-faelle"]);
    let mut h = PluginHost::new(p.clone());
    let config = BTreeMap::from([(
        FAELLE.to_string(),
        PluginConfig {
            enabled: true,
            rights: BTreeMap::from([("net".to_string(), "denied".to_string())]),
        },
    )]);
    h.start_all(&config);
    warten(&mut h, FAELLE);
    // Abgelehnt: keine Nachfrage, kein Netz; das Ergebnis geht still an das Plug-in.
    h.run_command("planora.test-faelle.netz");
    h.run_command("planora.test-faelle.recht");
    let events = h.poll();
    assert!(asks(&events).is_empty(), "{events:?}");
    assert!(nets(&events).is_empty());
    assert_eq!(state(&h, FAELLE), EntryState::Running);
    assert_eq!(h.plugin_configs(), config);
    aufraeumen(&p);
}
